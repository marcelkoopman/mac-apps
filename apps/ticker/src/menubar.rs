use mac_ui::tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use mac_ui::winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
};
use polars::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use crate::config::{self, load_config};
use crate::dialogs::{self, prompt_text};
use crate::menu_builder::MenuBuilder;
use crate::poll_gate::{Generation, PollGate};
use crate::price_fetcher::PriceFetcher;
use crate::price_history;
use crate::price_watch::{WatchDirection, WatchList, load_watch_list, save_watch_list};
use crate::watch_ui::{self, WatchUIBuilder};

const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Sent from the fetch thread to the event loop (wakes it up).
enum UserEvent {
    PricesFetched {
        generation: Generation,
        /// `Box<dyn Error>` is not `Send`; the error is only logged anyway.
        result: Result<DataFrame, String>,
    },
}

struct App {
    tray: Rc<RefCell<TrayIcon>>,
    fetcher: Option<PriceFetcher>,
    proxy: EventLoopProxy<UserEvent>,
    poll_gate: PollGate,
    config: Option<crate::config::Config>,
    prices_df: Option<DataFrame>,
    watch_list: WatchList,
    next_check: SystemTime,
    normal_icon: Icon,
    alert_icon: Icon,
    config_loaded: bool,
    config_error: Option<String>,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::PricesFetched { generation, result } => {
                let finished = self.poll_gate.finish(generation);
                if finished.apply {
                    self.apply_poll_result(result);
                }
                if let Some(next) = finished.restart {
                    self.spawn_fetch(next);
                }
            }
        }
    }

    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: mac_ui::winit::window::WindowId,
        _: WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.config_loaded {
            self.config_loaded = true;
            match load_config() {
                Ok(config) => {
                    self.config = Some(config);
                    if let Ok(list) = load_watch_list() {
                        self.watch_list = list;
                    }
                    if self.fetcher.is_some() {
                        self.poll_prices();
                        self.schedule_next_poll();
                    }
                }
                Err(e) => {
                    self.config_error = Some(format!("Config error: {e}"));
                    self.update_error_menu();
                }
            }
            return;
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                "quit" => event_loop.exit(),
                "poll" => {
                    if self.config.is_some() {
                        self.poll_prices();
                        self.schedule_next_poll();
                    }
                }
                "copy" => self.copy_prices_to_clipboard(),
                "add_watch" => self.handle_add_watch(),
                "manage_watches" => self.handle_manage_watches(),
                "edit_asset" => self.handle_edit_asset(),
                "reset_assets" => self.handle_reset_assets(),
                id if id.starts_with("watch_") => {
                    if let Some((asset, price)) = WatchUIBuilder::parse_watch_id(id)
                        && self.watch_list.remove_watch(&asset, price)
                    {
                        let _ = save_watch_list(&self.watch_list);
                        self.update_menu();
                    }
                }
                id => self.pin_menubar_from_item(id),
            }
        }

        if self.config.is_some() && SystemTime::now() >= self.next_check {
            self.poll_prices();
            self.schedule_next_poll();
        }
        if let Ok(d) = self.next_check.duration_since(SystemTime::now()) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + d));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

impl App {
    fn pin_menubar_from_item(&mut self, item_id: &str) {
        let Some(df) = &self.prices_df else {
            return;
        };
        let Some(name) = MenuBuilder::asset_name_for_item_id(df, item_id) else {
            return;
        };
        if let Some(config) = &mut self.config {
            config.menubar_asset = Some(name.clone());
        }
        if let Err(e) = config::save_menubar_pin(&name) {
            eprintln!("Failed to save menubar pin: {e}");
        }
        self.update_menu();
    }

    fn handle_edit_asset(&mut self) {
        let Some(config) = &self.config else {
            return;
        };
        if config.assets.is_empty() {
            watch_ui::send_macos_notification("Ticker", "No assets in config.");
            return;
        }
        let names: Vec<&str> = config.assets.iter().map(|a| a.name.as_str()).collect();
        let Some(asset_name) = dialogs::choose("Asset to edit:", &names)
            .and_then(|i| names.get(i))
            .map(|name| name.to_string())
        else {
            return;
        };
        let Some(current) = config.assets.iter().find(|a| a.name == asset_name).cloned() else {
            return;
        };

        let Some(url) = prompt_text(&format!("URL for {asset_name}:"), &current.url) else {
            return;
        };
        let Some(unit) = prompt_text(
            &format!("Currency / unit for {asset_name} (EUR, USD, GBP, …):"),
            &current.unit,
        ) else {
            return;
        };
        let Some(price_path) = prompt_text(
            &format!("JSON price path for {asset_name}:"),
            &current.price_path,
        ) else {
            return;
        };

        let apply_result = {
            let Some(config) = self.config.as_mut() else {
                return;
            };
            let Some(row) = config.assets.iter_mut().find(|a| a.name == asset_name) else {
                return;
            };
            match config::apply_asset_edit(row, &url, &unit, &price_path) {
                Ok(()) => config::save_user_config(config)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            }
        };

        match apply_result {
            Ok(()) => {
                watch_ui::send_macos_notification(
                    "Ticker",
                    &format!("{asset_name} saved ({unit})"),
                );
                self.prices_df = None;
                self.repoll_after_config_change();
            }
            Err(e) => watch_ui::send_macos_notification("Ticker", &e),
        }
    }

    fn handle_reset_assets(&mut self) {
        match config::reset_user_config() {
            Ok(config) => {
                self.config = Some(config);
                self.prices_df = None;
                watch_ui::send_macos_notification("Ticker", "Assets reset to defaults.");
                self.repoll_after_config_change();
            }
            Err(e) => watch_ui::send_macos_notification("Ticker", &format!("Reset failed: {e}")),
        }
    }

    fn copy_prices_to_clipboard(&self) {
        let empty = Self::empty_df();
        let df = self.prices_df.as_ref().unwrap_or(&empty);
        let tsv = MenuBuilder::dataframe_as_tsv(df);
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(tsv);
        }
    }

    /// Timer, "Poll now" and startup: start a background fetch unless one is already running
    /// (its result is just as fresh, so the request is dropped).
    fn poll_prices(&mut self) {
        if self.config.is_none() || self.fetcher.is_none() {
            return;
        }
        if let Some(generation) = self.poll_gate.try_start() {
            self.spawn_fetch(generation);
        }
    }

    /// Assets were edited or reset (`prices_df` already cleared): a fetch still running for the old
    /// assets is discarded and a fresh one starts (now, or as soon as the running one returns).
    fn repoll_after_config_change(&mut self) {
        if self.config.is_none() || self.fetcher.is_none() {
            return;
        }
        if let Some(generation) = self.poll_gate.invalidate() {
            self.spawn_fetch(generation);
        }
    }

    /// Runs the blocking HTTP requests on a worker thread; the result comes back as
    /// `UserEvent::PricesFetched` and is applied on the main thread in `apply_poll_result`.
    fn spawn_fetch(&mut self, generation: Generation) {
        let (Some(config), Some(fetcher)) = (&self.config, &self.fetcher) else {
            self.poll_gate.abort();
            return;
        };
        let fetcher = fetcher.clone();
        let assets = config.assets.clone();
        let previous = self.prices_df.clone();
        let proxy = self.proxy.clone();
        let spawned = std::thread::Builder::new()
            .name("ticker-fetch".into())
            .spawn(move || {
                let day_opens = price_history::load_day_opens();
                let result = match &previous {
                    None => fetcher.build_initial_dataframe(&assets, &day_opens),
                    Some(prev) => fetcher.update_dataframe(prev, &assets, &day_opens),
                }
                .map_err(|e| e.to_string());
                // Fails only when the event loop has already exited (app quitting).
                let _ = proxy.send_event(UserEvent::PricesFetched { generation, result });
            });
        if let Err(e) = spawned {
            eprintln!("Poll failed: cannot start fetch thread: {e}");
            self.poll_gate.abort();
        }
    }

    /// Main thread: merge a finished fetch into state, save history, fire watch alerts and
    /// refresh the menu (unchanged from the former synchronous poll).
    fn apply_poll_result(&mut self, result: Result<DataFrame, String>) {
        let mut df = match result {
            Ok(df) => df,
            Err(e) => {
                eprintln!("Poll failed: {e}");
                return;
            }
        };
        if let Some(prev) = &self.prices_df {
            let _ = fill_nan_from_prev(&mut df, prev);
        }
        let mut history = HashMap::new();
        let mut opens = HashMap::new();
        if let (Ok(names), Ok(prices), Ok(day_open_col)) =
            (df.column("name"), df.column("price"), df.column("day_open"))
            && let (Ok(ns), Ok(ps), Ok(os)) = (names.str(), prices.f64(), day_open_col.f64())
        {
            for i in 0..df.height() {
                if let (Some(n), Some(p)) = (ns.get(i), ps.get(i))
                    && !p.is_nan()
                {
                    history.insert(n.to_string(), p);
                }
                if let (Some(n), Some(o)) = (ns.get(i), os.get(i))
                    && !o.is_nan()
                {
                    opens.insert(n.to_string(), o);
                }
            }
        }
        if !history.is_empty() {
            let _ = price_history::save_price_history(&history);
        }
        if !opens.is_empty() {
            let _ = price_history::save_day_opens(&opens);
        }
        if let (Ok(names), Ok(prices)) = (df.column("name"), df.column("price"))
            && let (Ok(ns), Ok(ps)) = (names.str(), prices.f64())
        {
            let mut any = false;
            for i in 0..df.height() {
                if let (Some(n), Some(p)) = (ns.get(i), ps.get(i))
                    && !p.is_nan()
                {
                    for w in self.watch_list.check_price(n, p) {
                        let msg = WatchUIBuilder::format_trigger_notification(&w, p);
                        watch_ui::send_macos_notification("Ticker Price Alert", &msg);
                        any = true;
                    }
                }
            }
            if any {
                let _ = save_watch_list(&self.watch_list);
            }
        }
        self.prices_df = Some(df);
        self.update_menu();
    }

    fn handle_add_watch(&mut self) {
        let asset_names: Vec<String> = if let Some(df) = &self.prices_df {
            df.column("name")
                .ok()
                .and_then(|c| c.str().ok())
                .map(|ca| {
                    (0..df.height())
                        .filter_map(|i| ca.get(i).map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if asset_names.is_empty() {
            watch_ui::send_macos_notification(
                "Ticker",
                "No prices loaded yet. Press Poll now first.",
            );
            return;
        }
        let options: Vec<&str> = asset_names.iter().map(String::as_str).collect();
        let Some(asset) = dialogs::choose("Select asset for price watch:", &options)
            .and_then(|i| asset_names.get(i))
            .cloned()
        else {
            return;
        };
        let default_price = self
            .prices_df
            .as_ref()
            .and_then(|df| current_price_for(df, &asset))
            .unwrap_or(0.0);
        let target_price: f64 = match prompt_text(
            &format!("Target price for {asset} (€):"),
            &format!("{default_price:.2}"),
        ) {
            Some(s) => match s.replace(',', ".").parse() {
                Ok(v) => v,
                Err(_) => {
                    watch_ui::send_macos_notification("Ticker", "Invalid price entered.");
                    return;
                }
            },
            None => return,
        };
        let direction = match dialogs::choose("Trigger when price goes:", &["above", "below"]) {
            Some(0) => WatchDirection::Above,
            Some(1) => WatchDirection::Below,
            _ => return,
        };
        if self
            .watch_list
            .watches
            .iter()
            .any(|w| w.asset_name == asset && (w.target_price - target_price).abs() < 0.01)
        {
            watch_ui::send_macos_notification(
                "Ticker",
                &format!("Watch already exists for {} at €{:.2}", asset, target_price),
            );
            return;
        }
        self.watch_list
            .add_watch(asset.clone(), target_price, direction.clone());
        let _ = save_watch_list(&self.watch_list);
        watch_ui::send_macos_notification(
            "Ticker",
            &format!(
                "Watch set: {} {} €{:.2}",
                direction.emoji(),
                asset,
                target_price
            ),
        );
        self.update_menu();
    }

    fn handle_manage_watches(&mut self) {
        if self.watch_list.watches.is_empty() {
            watch_ui::send_macos_notification("Ticker", "No watches configured.");
            return;
        }
        let mut lines = String::new();
        for (i, w) in self.watch_list.watches.iter().enumerate() {
            lines.push_str(&format!(
                "{}. {} {} €{:.2}{}\n",
                i + 1,
                w.direction.emoji(),
                w.asset_name,
                w.target_price,
                if w.triggered { " ✓" } else { "" }
            ));
        }
        lines.push_str("\nClick a watch in the menu to remove it, or choose Clear All.");
        // "Close" stays the default button (Return), as in the old osascript dialog.
        if dialogs::buttons("Current watches:", &lines, &["Close", "Clear All"]) == Some(1) {
            self.watch_list = WatchList::new();
            let _ = save_watch_list(&self.watch_list);
            watch_ui::send_macos_notification("Ticker", "All watches cleared.");
            self.update_menu();
        }
    }

    fn schedule_next_poll(&mut self) {
        self.next_check = SystemTime::now() + POLL_INTERVAL;
    }

    fn has_alert(&self) -> bool {
        self.watch_list.watches.iter().any(|w| w.triggered)
    }

    fn update_menu(&self) {
        let empty = Self::empty_df();
        let df = self.prices_df.clone().unwrap_or(empty);
        let menu = MenuBuilder::build(&df, &self.watch_list);
        let pin = self.config.as_ref().and_then(|c| c.menubar_asset_name());
        let title = MenuBuilder::menubar_title(&df, pin);
        if let Ok(tray) = self.tray.try_borrow_mut() {
            tray.set_menu(Some(Box::new(menu)));
            let icon = if self.has_alert() {
                self.alert_icon.clone()
            } else {
                self.normal_icon.clone()
            };
            let _ = tray.set_icon(Some(icon));
            tray.set_title(Some(&title));
        }
    }

    fn update_error_menu(&self) {
        let menu = Menu::new();
        if let Some(e) = &self.config_error {
            let _ = menu.append(&MenuItem::new(format!("❌ {e}"), false, None));
        }
        let _ = menu.append(&MenuItem::with_id("poll", "🔄 Retry", true, None));
        let _ = menu.append(&MenuItem::with_id("quit", " Quit", true, None));
        if let Ok(tray) = self.tray.try_borrow_mut() {
            tray.set_menu(Some(Box::new(menu)));
        }
    }

    fn empty_df() -> DataFrame {
        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), Vec::<String>::new()).into(),
            Series::new("name".into(), Vec::<String>::new()).into(),
            Series::new("price".into(), Vec::<f64>::new()).into(),
            Series::new("unit".into(), Vec::<String>::new()).into(),
            Series::new("unit_hint".into(), Vec::<String>::new()).into(),
            Series::new("prev_price".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("change".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("pct_change".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("direction".into(), Vec::<String>::new()).into(),
            Series::new("day_open".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("change_day".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("pct_day".into(), Vec::<Option<f64>>::new()).into(),
            Series::new("direction_day".into(), Vec::<String>::new()).into(),
        ])
        .expect("empty")
    }
}

fn current_price_for(df: &DataFrame, asset: &str) -> Option<f64> {
    let names = df.column("name").ok()?.str().ok()?;
    let prices = df.column("price").ok()?.f64().ok()?;
    for i in 0..df.height() {
        if names.get(i) == Some(asset) {
            let p = prices.get(i)?;
            if !p.is_nan() {
                return Some(p);
            }
        }
    }
    None
}

fn fill_nan_from_prev(
    df: &mut DataFrame,
    prev: &DataFrame,
) -> Result<(), Box<dyn std::error::Error>> {
    let pn = prev.column("name")?.str()?;
    let pp = prev.column("price")?.f64()?;
    let mut map = HashMap::new();
    for i in 0..prev.height() {
        if let (Some(n), Some(p)) = (pn.get(i), pp.get(i))
            && !p.is_nan()
        {
            map.insert(n.to_string(), p);
        }
    }
    let name_ca = df.column("name")?.str()?;
    let height = df.height();
    let mut names: Vec<String> = Vec::with_capacity(height);
    for i in 0..height {
        names.push(name_ca.get(i).unwrap_or("").to_string());
    }
    let prices = df.column("price")?.f64()?;
    let mut out = Vec::with_capacity(height);
    for (i, name) in names.iter().enumerate() {
        let p = prices.get(i).unwrap_or(f64::NAN);
        out.push(if p.is_nan() {
            map.get(name).copied().unwrap_or(f64::NAN)
        } else {
            p
        });
    }
    df.with_column(Series::new("price".into(), out).into())?;
    Ok(())
}

fn bundle_assets_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(app) = exe.ancestors().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".app"))
                .unwrap_or(false)
        })
    {
        let a = app.join("Contents/Resources/assets");
        if a.exists() {
            return a;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_icon(name: &str) -> Result<Icon, Box<dyn std::error::Error>> {
    let path = bundle_assets_dir().join(name);
    Ok(mac_ui::icon::from_image_file(&path)?)
}

fn fallback_icon(r: u8, g: u8, b: u8) -> Icon {
    mac_ui::icon::Canvas::filled(16, 16, [r, g, b, 255])
        .into_icon()
        .expect("icon")
}

pub fn run_menubar() -> Result<(), Box<dyn std::error::Error>> {
    watch_ui::request_notification_permission();
    let fetcher = PriceFetcher::new()?;
    let normal_icon = load_icon("normal.png").unwrap_or_else(|_| fallback_icon(255, 255, 255));
    let alert_icon = load_icon("update.png").unwrap_or_else(|_| fallback_icon(255, 80, 80));
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new("⏳ Loading...", false, None));
    let _ = menu.append(&MenuItem::with_id("poll", "🔄 Retry", true, None));
    let _ = menu.append(&MenuItem::with_id("quit", " Quit", true, None));
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(normal_icon.clone())
        .with_tooltip("Price Ticker")
        .with_title("Ticker")
        .build()?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App {
        tray: Rc::new(RefCell::new(tray_icon)),
        fetcher: Some(fetcher),
        proxy: event_loop.create_proxy(),
        poll_gate: PollGate::new(),
        config: None,
        prices_df: None,
        watch_list: WatchList::new(),
        next_check: SystemTime::now(),
        normal_icon,
        alert_icon,
        config_loaded: false,
        config_error: None,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
