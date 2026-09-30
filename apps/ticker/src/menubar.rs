use image::ImageReader;
use polars::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::config::{self, load_config};
use crate::menu_builder::MenuBuilder;
use crate::price_fetcher::PriceFetcher;
use crate::price_history;
use crate::price_watch::{WatchDirection, WatchList, load_watch_list, save_watch_list};
use crate::watch_ui::{self, WatchUIBuilder};
use std::process::Command;

const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

struct App {
    tray: Rc<RefCell<TrayIcon>>,
    fetcher: Option<PriceFetcher>,
    config: Option<crate::config::Config>,
    prices_df: Option<DataFrame>,
    watch_list: WatchList,
    next_check: SystemTime,
    normal_icon: Icon,
    alert_icon: Icon,
    config_loaded: bool,
    config_error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn window_event(&mut self, _: &ActiveEventLoop, _: winit::window::WindowId, _: WindowEvent) {}

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
        let names: Vec<String> = config.assets.iter().map(|a| a.name.clone()).collect();
        let list = names
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(", ");
        let pick = format!(
            "choose from list {{{}}} with prompt \"Asset to edit:\" default items {{\"{}\"}}",
            list,
            names.first().cloned().unwrap_or_default()
        );
        let asset_name = match run_osascript_output(&pick) {
            Some(s) if s != "false" => s.trim().to_string(),
            _ => return,
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
                self.poll_prices();
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
                self.poll_prices();
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

    fn poll_prices(&mut self) {
        let Some(config) = &self.config else { return };
        let Some(fetcher) = &self.fetcher else { return };
        let day_opens = price_history::load_day_opens();
        let result = match &self.prices_df {
            None => fetcher.build_initial_dataframe(&config.assets, &day_opens),
            Some(prev) => fetcher.update_dataframe(prev, &config.assets, &day_opens),
        };
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
        let asset_list = asset_names
            .iter()
            .map(|n| format!("\"{}\"", n))
            .collect::<Vec<_>>()
            .join(", ");
        let pick_script = format!(
            "choose from list {{{}}} with prompt \"Select asset for price watch:\" default items {{\"{}\"}}",
            asset_list,
            asset_names.first().cloned().unwrap_or_default()
        );
        let asset = match run_osascript_output(&pick_script) {
            Some(s) if s != "false" => s.trim().to_string(),
            _ => return,
        };
        let default_price = self
            .prices_df
            .as_ref()
            .and_then(|df| current_price_for(df, &asset))
            .unwrap_or(0.0);
        let price_script = format!(
            "text returned of (display dialog \"Target price for {} (€):\" default answer \"{:.2}\" buttons {{\"Cancel\", \"OK\"}} default button \"OK\")",
            asset, default_price
        );
        let target_price: f64 = match run_osascript_output(&price_script) {
            Some(s) => match s.trim().replace(',', ".").parse() {
                Ok(v) => v,
                Err(_) => {
                    watch_ui::send_macos_notification("Ticker", "Invalid price entered.");
                    return;
                }
            },
            None => return,
        };
        let direction = match run_osascript_output(
            "choose from list {\"above\", \"below\"} with prompt \"Trigger when price goes:\" default items {\"above\"}",
        ) {
            Some(s) if s.trim() == "below" => WatchDirection::Below,
            Some(s) if s.trim() == "above" => WatchDirection::Above,
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
        let mut lines = String::from("Current watches:\\n");
        for (i, w) in self.watch_list.watches.iter().enumerate() {
            lines.push_str(&format!(
                "{}. {} {} €{:.2}{}\\n",
                i + 1,
                w.direction.emoji(),
                w.asset_name,
                w.target_price,
                if w.triggered { " ✓" } else { "" }
            ));
        }
        lines.push_str("\\nClick a watch in the menu to remove it, or choose Clear All.");
        let script = format!(
            "display dialog \"{}\" buttons {{\"Close\", \"Clear All\"}} default button \"Close\"",
            lines
        );
        if let Some(btn) = run_osascript_button(&script)
            && btn.contains("Clear All")
        {
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

fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn prompt_text(message: &str, default: &str) -> Option<String> {
    let script = format!(
        "text returned of (display dialog \"{}\" default answer \"{}\" buttons {{\"Cancel\", \"OK\"}} default button \"OK\")",
        applescript_escape(message),
        applescript_escape(default)
    );
    run_osascript_output(&script).map(|s| s.trim().to_string())
}

fn run_osascript_output(script: &str) -> Option<String> {
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn run_osascript_button(script: &str) -> Option<String> {
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
    let img = ImageReader::open(&path)?.decode()?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok(Icon::from_rgba(img.into_raw(), w, h)?)
}

fn fallback_icon(r: u8, g: u8, b: u8) -> Icon {
    let mut rgba = Vec::with_capacity(1024);
    for _ in 0..256 {
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    Icon::from_rgba(rgba, 16, 16).expect("icon")
}

pub fn run_menubar() -> Result<(), Box<dyn std::error::Error>> {
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
    let mut app = App {
        tray: Rc::new(RefCell::new(tray_icon)),
        fetcher: Some(fetcher),
        config: None,
        prices_df: None,
        watch_list: WatchList::new(),
        next_check: SystemTime::now(),
        normal_icon,
        alert_icon,
        config_loaded: false,
        config_error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    Ok(())
}
