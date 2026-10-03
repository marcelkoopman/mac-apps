use mac_ui::tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use mac_ui::winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use crate::config::{self, load_config};
use crate::dialogs::{self, prompt_text};
use crate::freshness::AssetStatus;
use crate::log_message;
use crate::menu_builder::{Freshness, MenuBuilder};
use crate::menu_ids::{self, RowRef};
use crate::plausibility::{TriggerGate, Verdict};
use crate::poll_gate::{Generation, PollGate};
use crate::price_fetcher::PriceFetcher;
use crate::price_history;
use crate::price_input;
use crate::price_watch::{
    AppUpdate, FileStamp, PriceWatch, WatchDirection, WatchList, update_watch_list_for_app,
    watch_file_stamp,
};
use crate::prices::{self, PriceRow};
use crate::watch_ui::{self, WatchUIBuilder};

const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// After the Mac wakes, the next poll comes this soon (Wi-Fi needs a moment to reconnect).
const POLL_AFTER_WAKE: Duration = Duration::from_secs(5);
/// A dialog item clicked while the menu is still tracking is retried this often ...
const MENU_RETRY: Duration = Duration::from_millis(50);
/// ... and opened anyway after this long.
const MENU_MAX_DEFER: Duration = Duration::from_secs(2);

/// Menu items whose handler opens a modal dialog.
fn opens_dialog(id: &str) -> bool {
    matches!(
        id,
        "add_watch" | "manage_watches" | "edit_asset" | "reset_assets"
    )
}

/// Sent to the event loop (wakes it up): fetch results from the fetch thread, menu clicks from
/// the muda event handler.
enum UserEvent {
    /// Id of the clicked menu item.
    Menu(String),
    PricesFetched {
        generation: Generation,
        /// One row per asset (NaN price where the fetch failed).
        rows: Vec<PriceRow>,
    },
    /// The Mac goes to sleep (`true`) or woke up (`false`). Only sent on macOS.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Sleep(bool),
}

struct App {
    tray: Rc<RefCell<TrayIcon>>,
    fetcher: Option<PriceFetcher>,
    proxy: EventLoopProxy<UserEvent>,
    poll_gate: PollGate,
    /// Clicked menu items not handled yet (dialog items wait until the menu has closed).
    pending_menu: VecDeque<String>,
    /// When the first pending item started waiting for the menu to close.
    menu_deferred_since: Option<Instant>,
    config: Option<crate::config::Config>,
    prices: Option<Vec<PriceRow>>,
    watch_list: WatchList,
    /// Per asset name: last successful fetch and failed polls since (stale marker in the menu).
    asset_status: HashMap<String, AssetStatus>,
    /// Plausibility check of fetched prices before they may set off a watch.
    trigger_gate: TriggerGate,
    /// The watch file exists but could not be read or moved aside: never overwrite it.
    watch_save_blocked: bool,
    /// Watch file stamp after our last load or save; a different stamp at a poll means the CLI
    /// changed the file, so it is reloaded.
    watch_stamp: Option<FileStamp>,
    /// Part of the row ids (`menu_ids`). Bumped whenever watch or asset rows are added, removed
    /// or reordered, so a click on a row of a menu built before that is ignored. Price updates
    /// keep it, so rows stay clickable while a poll refreshes the open menu.
    rows_generation: u64,
    next_check: SystemTime,
    /// Normal template icon and the coloured alert icon.
    glyphs: mac_ui::tray::Glyphs,
    config_loaded: bool,
    config_error: Option<String>,
    /// Between the will-sleep and did-wake notifications: no polls.
    asleep: bool,
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(id) => {
                log_message(&format!("menu: queued {id:?}"));
                self.pending_menu.push_back(id);
            }
            UserEvent::PricesFetched { generation, rows } => {
                let finished = self.poll_gate.finish(generation);
                if finished.apply {
                    self.apply_poll_result(rows);
                }
                if let Some(next) = finished.restart {
                    self.spawn_fetch(next);
                }
            }
            UserEvent::Sleep(true) => {
                log_message("power: going to sleep; polling paused");
                self.asleep = true;
            }
            UserEvent::Sleep(false) => {
                log_message(&format!(
                    "power: woke up; polling in {}s",
                    POLL_AFTER_WAKE.as_secs()
                ));
                self.asleep = false;
                self.next_check = SystemTime::now() + POLL_AFTER_WAKE;
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
            self.load_config_and_start();
            return;
        }

        let retry_at = self.handle_pending_menu(event_loop);

        if self.config.is_some() && !self.asleep && SystemTime::now() >= self.next_check {
            self.poll_prices();
            self.schedule_next_poll();
        }
        // Asleep: no timer; the did-wake event schedules the next poll.
        let poll_at = self
            .next_check
            .duration_since(SystemTime::now())
            .ok()
            .filter(|_| !self.asleep)
            .map(|d| Instant::now() + d);
        event_loop.set_control_flow(mac_ui::wake::control_flow([retry_at, poll_at]));
    }
}

impl App {
    /// First start, and *Retry* after a config error: (re)load the config, then the watches and
    /// the first poll; on an error show the error menu (whose *Retry* comes back here).
    fn load_config_and_start(&mut self) {
        self.config_loaded = true;
        match load_config() {
            Ok(config) => {
                let recovered = self.config_error.take().is_some();
                self.config = Some(config);
                self.load_watches();
                if recovered {
                    // Replace the error menu (the first start keeps "Loading..." until the poll).
                    log_message("config: loaded after an earlier error");
                    self.update_menu();
                }
                if self.fetcher.is_some() {
                    self.poll_prices();
                    self.schedule_next_poll();
                }
            }
            Err(e) => {
                log_message(&format!("config: cannot load: {e}"));
                self.config_error = Some(format!("Config error: {e}"));
                self.update_error_menu();
            }
        }
    }

    /// Runs queued menu actions in click order. A dialog action waits (up to `MENU_MAX_DEFER`)
    /// until the menu has stopped tracking; returns when to look again in that case.
    fn handle_pending_menu(&mut self, event_loop: &ActiveEventLoop) -> Option<Instant> {
        while let Some(id) = self.pending_menu.front() {
            if opens_dialog(id) && !dialogs::can_run_modal() {
                let since = *self.menu_deferred_since.get_or_insert_with(Instant::now);
                if since.elapsed() < MENU_MAX_DEFER {
                    return Some(Instant::now() + MENU_RETRY);
                }
                log_message(&format!(
                    "menu: {id:?} waited {MENU_MAX_DEFER:?} for the menu to close; opening anyway"
                ));
            }
            self.menu_deferred_since = None;
            let Some(id) = self.pending_menu.pop_front() else {
                break;
            };
            self.handle_menu_item(&id, event_loop);
        }
        None
    }

    fn handle_menu_item(&mut self, id: &str, event_loop: &ActiveEventLoop) {
        log_message(&format!("menu: handling {id:?}"));
        match id {
            mac_ui::tray::QUIT_ID => event_loop.exit(),
            "poll" => {
                if self.config.is_some() {
                    self.poll_prices();
                    self.schedule_next_poll();
                } else {
                    // Retry in the error menu: load the config again (it may have been fixed).
                    log_message("menu: no config loaded; reloading it");
                    self.config_loaded = false;
                    self.load_config_and_start();
                }
            }
            "copy" => {
                if let Err(e) = self.copy_prices_to_clipboard() {
                    log_message(&format!("copy: clipboard write failed: {e}"));
                }
            }
            "add_watch" => self.handle_add_watch(),
            "manage_watches" => self.handle_manage_watches(),
            "edit_asset" => self.handle_edit_asset(),
            "reset_assets" => self.handle_reset_assets(),
            id => match menu_ids::parse(id) {
                menu_ids::Parsed::Row { generation, row } => {
                    if generation != self.rows_generation {
                        log_message(&format!("menu: {id:?} is from an outdated menu; ignored"));
                    } else {
                        match row {
                            RowRef::Rearm(index) => self.rearm_watch_at(index),
                            RowRef::Remove(index) => self.remove_watch_at(index),
                            RowRef::Asset(row) => self.pin_menubar_from_row(row),
                        }
                    }
                }
                menu_ids::Parsed::Fixed => log_message(&format!("menu: unknown id {id:?}")),
            },
        }
        log_message(&format!("menu: done {id:?}"));
    }

    fn load_watches(&mut self) {
        self.update_watches(|_| ());
    }

    /// Reload the watch list when the file changed on disk since our last load or save (the CLI
    /// adds, removes and resets watches while the app runs). Checked at every poll.
    fn reload_watches_if_changed(&mut self) {
        let stamp = watch_file_stamp();
        if stamp != self.watch_stamp {
            log_message("watches: file changed on disk; reloading");
            self.update_watches(|_| ());
        }
    }

    /// Load the watch file, apply `change` and save, all under the watch-file lock, then adopt
    /// the result: a change made by the CLI in the meantime is kept, not overwritten. When the
    /// file cannot be locked, read or saved, `change` is applied to the list in memory instead
    /// (not saved), hence `FnMut`.
    fn update_watches<T>(&mut self, mut change: impl FnMut(&mut WatchList) -> T) -> T {
        match update_watch_list_for_app(&mut change) {
            Ok(AppUpdate {
                list,
                recovered,
                value,
                stamp,
            }) => {
                if let Some((backup, error)) = recovered {
                    log_message(&format!(
                        "watches: file unreadable ({error}); moved to {}",
                        backup.display()
                    ));
                    watch_ui::send_macos_notification(
                        "Ticker",
                        &format!(
                            "Watch list was unreadable and has been reset. Old file kept as {}",
                            backup.display()
                        ),
                    );
                }
                if !list.same_rows(&self.watch_list) {
                    self.rows_changed();
                }
                self.watch_list = list;
                self.watch_stamp = stamp;
                self.watch_save_blocked = false;
                value
            }
            Err(e) => {
                if !self.watch_save_blocked {
                    log_message(&format!(
                        "watches: cannot load ({e}); changes will not be saved"
                    ));
                    watch_ui::send_macos_notification(
                        "Ticker",
                        &format!(
                            "Cannot read the watch list ({e}). Watch changes will not be saved."
                        ),
                    );
                }
                self.watch_save_blocked = true;
                // Remember the stamp so a broken file is not retried (and reported) every poll;
                // any later change to it is picked up again.
                self.watch_stamp = watch_file_stamp();
                let before = self.watch_list.clone();
                let value = change(&mut self.watch_list);
                if !before.same_rows(&self.watch_list) {
                    self.rows_changed();
                }
                value
            }
        }
    }

    fn rows_changed(&mut self) {
        self.rows_generation = self.rows_generation.wrapping_add(1);
    }

    fn remove_watch_at(&mut self, index: usize) {
        // The row index belongs to the list the menu was built from; remove that watch wherever
        // it is in the file now.
        let Some(watch) = self.watch_list.watches.get(index).cloned() else {
            return;
        };
        if self.update_watches(|list| list.remove_matching(&watch)) {
            log_message(&format!(
                "watches: removed {} {:.2}",
                watch.asset_name, watch.target_price
            ));
        }
        self.update_menu();
    }

    fn rearm_watch_at(&mut self, index: usize) {
        let Some(watch) = self.watch_list.watches.get(index).cloned() else {
            return;
        };
        if self.update_watches(|list| list.rearm_matching(&watch)) {
            log_message(&format!(
                "watches: re-armed {} {} {}",
                watch.asset_name,
                watch.direction.as_str(),
                watch.target_price
            ));
        }
        self.update_menu();
    }

    fn pin_menubar_from_row(&mut self, row: usize) {
        let Some(rows) = &self.prices else {
            return;
        };
        let Some(name) = MenuBuilder::asset_name_at(rows, row) else {
            return;
        };
        if let Some(config) = &mut self.config {
            config.menubar_asset = Some(name.clone());
        }
        if let Err(e) = config::save_menubar_pin(&name) {
            log_message(&format!("Failed to save menubar pin: {e}"));
        }
        self.update_menu();
    }

    fn handle_edit_asset(&mut self) {
        let Some(config) = &self.config else {
            log_message("edit_asset: no config loaded");
            return;
        };
        if config.assets.is_empty() {
            log_message("edit_asset: no assets in config");
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
                // New URL or unit: its next price is not compared with the old one.
                self.trigger_gate.retain(|name| name != asset_name);
                watch_ui::send_macos_notification(
                    "Ticker",
                    &format!("{asset_name} saved ({unit})"),
                );
                self.prices = None;
                self.rows_changed();
                self.repoll_after_config_change();
            }
            Err(e) => watch_ui::send_macos_notification("Ticker", &e),
        }
    }

    fn handle_reset_assets(&mut self) {
        // Cancel is the default button (Return).
        let confirmed = dialogs::buttons(
            "Reset assets to defaults?",
            "This deletes your edited asset settings (~/.ticker_config.toml) and goes back to the \
             bundled assets. Price watches are kept.",
            &["Cancel", "Reset"],
        ) == Some(1);
        if !confirmed {
            return;
        }
        match config::reset_user_config() {
            Ok(config) => {
                self.config = Some(config);
                self.trigger_gate = TriggerGate::new();
                self.prices = None;
                self.rows_changed();
                watch_ui::send_macos_notification("Ticker", "Assets reset to defaults.");
                self.repoll_after_config_change();
            }
            Err(e) => watch_ui::send_macos_notification("Ticker", &format!("Reset failed: {e}")),
        }
    }

    fn copy_prices_to_clipboard(&self) -> Result<(), arboard::Error> {
        let tsv = MenuBuilder::prices_as_tsv(self.prices.as_deref().unwrap_or_default());
        arboard::Clipboard::new()?.set_text(tsv)
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

    /// Assets were edited or reset (`prices` already cleared): a fetch still running for the old
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
        let assets = config.fetchable_assets();
        let previous = self.prices.clone();
        let proxy = self.proxy.clone();
        let spawned = std::thread::Builder::new()
            .name("ticker-fetch".into())
            .spawn(move || {
                let day_opens = price_history::load_day_opens();
                let rows = fetcher.poll(&assets, previous.as_deref(), &day_opens);
                // Fails only when the event loop has already exited (app quitting).
                let _ = proxy.send_event(UserEvent::PricesFetched { generation, rows });
            });
        if let Err(e) = spawned {
            log_message(&format!("Poll failed: cannot start fetch thread: {e}"));
            self.poll_gate.abort();
        }
    }

    /// Main thread: merge a finished fetch into state, save history, fire watch alerts and
    /// refresh the menu (unchanged from the former synchronous poll).
    fn apply_poll_result(&mut self, mut rows: Vec<PriceRow>) {
        self.record_fetch_status(&rows);
        // Only prices fetched by this poll, and only plausible ones, may set off a watch.
        let current = self.plausible_prices(&rows);
        if let Some(prev) = &self.prices {
            prices::fill_nan_from_prev(&mut rows, prev);
        }
        if let Err(e) = save_poll_history(&rows) {
            log_message(&format!("Poll: saving price history failed: {e}"));
        }
        self.reload_watches_if_changed();
        // Only take the lock and touch the file when a watch actually goes off.
        if !fired_watches(&mut self.watch_list.clone(), &current).is_empty() {
            for (w, p) in self.update_watches(|list| fired_watches(list, &current)) {
                let unit = self.unit_of(&w.asset_name);
                let msg = WatchUIBuilder::format_trigger_notification(&w, p, &unit);
                watch_ui::send_macos_notification("Ticker Price Alert", &msg);
            }
        }
        self.prices = Some(rows);
        self.update_menu();
    }

    /// The `(asset, price)` pairs of this poll that pass the [`TriggerGate`]; the others are
    /// logged.
    fn plausible_prices(&mut self, rows: &[PriceRow]) -> Vec<(String, f64)> {
        let Some(config) = &self.config else {
            return Vec::new();
        };
        let mut plausible = Vec::new();
        for row in rows.iter().filter(|r| r.has_price()) {
            let Some(asset) = config.assets.iter().find(|a| a.name == row.name) else {
                continue;
            };
            match self.trigger_gate.check(asset, row.price) {
                Verdict::Accept => plausible.push((row.name.clone(), row.price)),
                Verdict::Reject(why) => log_message(&format!(
                    "watches: {} price not used for watches: {why}",
                    row.name
                )),
                Verdict::Unconfirmed { from, pct } => log_message(&format!(
                    "watches: {} jumped {pct:.0}% ({from} → {}); waiting for the next poll to \
                     confirm before checking watches",
                    row.name, row.price
                )),
            }
        }
        self.trigger_gate
            .retain(|name| rows.iter().any(|r| r.name == name));
        plausible
    }

    fn handle_add_watch(&mut self) {
        let asset_names: Vec<String> = self
            .prices
            .iter()
            .flatten()
            .map(|r| r.name.clone())
            .collect();
        if asset_names.is_empty() {
            log_message("add_watch: no prices loaded yet, nothing to pick");
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
        let current = self
            .prices
            .as_ref()
            .and_then(|rows| prices::price_of(rows, &asset))
            .filter(|p| p.is_finite());
        let unit = self.unit_of(&asset);
        let allow_negative = self
            .asset_config(&asset)
            .is_some_and(config::Asset::allows_negative);
        // Prefilled in the menu's Dutch notation, which parse_watch_target reads back.
        let target_price: f64 = match prompt_text(
            &format!("Target price for {asset} ({}):", self.unit_label(&asset)),
            &MenuBuilder::format_price(current.unwrap_or(0.0)),
        ) {
            Some(s) => match price_input::parse_watch_target(&s, allow_negative) {
                Ok(v) => v,
                Err(e) => {
                    log_message(&format!("add_watch: {e}"));
                    watch_ui::send_macos_notification("Ticker", &format!("Invalid price: {e}"));
                    return;
                }
            },
            None => return,
        };
        // Up to a higher target, down to a lower one; only asked when the target is the current
        // price or there is none.
        let direction = match current.and_then(|now| WatchDirection::toward(target_price, now)) {
            Some(direction) => direction,
            None => match dialogs::choose("Trigger when price goes:", &["above", "below"]) {
                Some(0) => WatchDirection::Above,
                Some(1) => WatchDirection::Below,
                _ => return,
            },
        };
        let added = self.update_watches(|list| {
            if list
                .watches
                .iter()
                .any(|w| w.asset_name == asset && (w.target_price - target_price).abs() < 0.01)
            {
                return false;
            }
            list.add_watch(asset.clone(), target_price, direction.clone());
            true
        });
        if !added {
            watch_ui::send_macos_notification(
                "Ticker",
                &format!(
                    "Watch already exists for {asset} at {}",
                    MenuBuilder::format_money(&unit, target_price)
                ),
            );
            return;
        }
        watch_ui::send_macos_notification(
            "Ticker",
            &format!(
                "Watch set: {asset} {} {}{}",
                direction.as_str(),
                MenuBuilder::format_money(&unit, target_price),
                current
                    .map(|now| format!(" (now {})", MenuBuilder::format_money(&unit, now)))
                    .unwrap_or_default()
            ),
        );
        self.update_menu();
    }

    /// The configured unit of `asset` (case-insensitive, like watches), or "" when unknown.
    fn unit_of(&self, asset: &str) -> String {
        self.asset_config(asset)
            .map(|a| a.unit.clone())
            .unwrap_or_default()
    }

    /// Unit and hint for a prompt: `EUR/kWh`, `EUR / troy oz`.
    fn unit_label(&self, asset: &str) -> String {
        match self.asset_config(asset) {
            Some(a) if a.unit_hint.trim().is_empty() => a.unit.clone(),
            Some(a) if a.unit_hint.trim().starts_with('/') => {
                format!("{}{}", a.unit, a.unit_hint.trim())
            }
            Some(a) => format!("{} / {}", a.unit, a.unit_hint.trim()),
            None => "price".to_string(),
        }
    }

    fn asset_config(&self, asset: &str) -> Option<&config::Asset> {
        let wanted = asset.to_lowercase();
        self.config
            .as_ref()?
            .assets
            .iter()
            .find(|a| a.name == asset || a.name.to_lowercase() == wanted)
    }

    fn handle_manage_watches(&mut self) {
        self.reload_watches_if_changed();
        if self.watch_list.watches.is_empty() {
            log_message("manage_watches: no watches configured");
            watch_ui::send_macos_notification("Ticker", "No watches configured.");
            return;
        }
        let mut lines = String::new();
        for (i, w) in self.watch_list.watches.iter().enumerate() {
            lines.push_str(&format!(
                "{}. {} {} {}{}\n",
                i + 1,
                w.asset_name,
                w.direction.as_str(),
                MenuBuilder::format_money(&self.unit_of(&w.asset_name), w.target_price),
                if w.triggered { " (triggered)" } else { "" }
            ));
        }
        lines.push_str(
            "\nRe-arm or remove a single watch from its submenu in the menu, or choose Clear All.",
        );
        // "Close" stays the default button (Return), as in the old osascript dialog.
        if dialogs::buttons("Current watches:", &lines, &["Close", "Clear All"]) != Some(1) {
            return;
        }
        let count = self.watch_list.watches.len();
        let confirmed = dialogs::buttons(
            "Clear all watches?",
            &format!(
                "This removes all {count} price watch{}.",
                if count == 1 { "" } else { "es" }
            ),
            &["Cancel", "Clear All"],
        ) == Some(1);
        if confirmed {
            let removed = self.update_watches(|list| std::mem::take(&mut list.watches).len());
            log_message(&format!("watches: cleared {removed}"));
            watch_ui::send_macos_notification("Ticker", "All watches cleared.");
            self.update_menu();
        }
    }

    fn schedule_next_poll(&mut self) {
        self.next_check = SystemTime::now() + POLL_INTERVAL;
    }

    /// Before the NaN prices are filled in from the last poll: which assets this poll fetched.
    fn record_fetch_status(&mut self, rows: &[PriceRow]) {
        let now = chrono::Local::now();
        let mut seen = HashMap::new();
        for row in rows {
            let fetched = row.has_price();
            let mut status = self
                .asset_status
                .get(&row.name)
                .copied()
                .unwrap_or_default();
            status.record(fetched, now);
            if !fetched {
                log_message(&format!(
                    "poll: no price for {} ({} failed poll(s) in a row)",
                    row.name, status.failed_polls
                ));
            }
            seen.insert(row.name.clone(), status);
        }
        // Assets that were edited away or reset drop out.
        self.asset_status = seen;
    }

    fn has_alert(&self) -> bool {
        self.watch_list.watches.iter().any(|w| w.triggered)
    }

    fn update_menu(&self) {
        let rows = self.prices.as_deref().unwrap_or_default();
        let freshness = Freshness {
            status: &self.asset_status,
            now: chrono::Local::now(),
        };
        let skipped = self
            .config
            .as_ref()
            .map(config::Config::skipped_assets)
            .unwrap_or_default();
        let menu = MenuBuilder::build(
            rows,
            &self.watch_list,
            self.rows_generation,
            &freshness,
            &skipped,
        );
        let pin = self.config.as_ref().and_then(|c| c.menubar_asset_name());
        let title = MenuBuilder::menubar_title(rows, pin, &freshness);
        if let Ok(tray) = self.tray.try_borrow_mut() {
            tray.set_menu(Some(Box::new(menu)));
            // The normal icon is a template that follows the menu bar colours. The alert icon
            // keeps its own colours (red badge), so an alert still stands out.
            let glyph = if self.has_alert() {
                mac_ui::tray::Glyph::Alert
            } else {
                mac_ui::tray::Glyph::Normal
            };
            if let Err(e) = self.glyphs.show(&tray, glyph) {
                log_message(&format!("menu: setting the menubar icon failed: {e}"));
            }
            tray.set_title(Some(&title));
            mac_ui::tray::set_accessibility_label(
                &tray,
                &MenuBuilder::tray_accessibility_label(&title, self.has_alert()),
            );
        }
    }

    fn update_error_menu(&self) {
        let menu = Menu::new();
        if let Some(e) = &self.config_error {
            let _ = menu.append(&MenuItem::new(format!("❌ {e}"), false, None));
        }
        let _ = menu.append(&MenuItem::with_id("poll", "🔄 Retry", true, None));
        let _ = menu.append(&mac_ui::tray::quit_item("Quit"));
        if let Ok(tray) = self.tray.try_borrow_mut() {
            tray.set_menu(Some(Box::new(menu)));
        }
    }
}

/// Checks every `(asset, price)` against `list` (marking the watches that go off) and returns
/// those watches with the price that set them off.
fn fired_watches(list: &mut WatchList, prices: &[(String, f64)]) -> Vec<(PriceWatch, f64)> {
    prices
        .iter()
        .flat_map(|(name, price)| {
            list.check_price(name, *price)
                .into_iter()
                .map(move |w| (w, *price))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Saves each asset's price and day open from a finished poll to the price history file.
/// Both saves are tried; the first error is returned.
fn save_poll_history(rows: &[PriceRow]) -> Result<(), Box<dyn std::error::Error>> {
    let history = prices::prices_by_name(rows);
    let opens: HashMap<String, f64> = rows
        .iter()
        .filter_map(|r| Some((r.name.clone(), r.day_open.filter(|o| !o.is_nan())?)))
        .collect();
    let history_saved = if history.is_empty() {
        Ok(())
    } else {
        price_history::save_price_history(&history)
    };
    if !opens.is_empty() {
        price_history::save_day_opens(&opens)?;
    }
    history_saved
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

fn log_icon_fallback(name: &str, e: &mac_ui::icon::IconError) {
    log_message(&format!(
        "menubar: icon {name} not loaded ({e}); using a plain one"
    ));
}

/// The bundled icon `name` as a template glyph (its light parts, see
/// `mac_ui::icon::template_mask`), or a plain square when it cannot be loaded.
fn load_template_icon_or(name: &str) -> Result<Icon, mac_ui::icon::IconError> {
    let path = bundle_assets_dir().join(name);
    mac_ui::icon::template_from_image_file_or_plain(&path, |e| log_icon_fallback(name, e))
}

/// The bundled icon `name`, or a plain square in the given color when it cannot be loaded.
fn load_icon_or(name: &str, rgb: [u8; 3]) -> Result<Icon, mac_ui::icon::IconError> {
    let path = bundle_assets_dir().join(name);
    mac_ui::icon::from_image_file_or_plain(&path, rgb, |e| log_icon_fallback(name, e))
}

pub fn run_menubar() -> Result<(), Box<dyn std::error::Error>> {
    watch_ui::request_notification_permission();
    let fetcher = PriceFetcher::new()?;
    let normal_icon = load_template_icon_or("normal.png")?;
    let alert_icon = load_icon_or("update.png", [255, 80, 80])?;
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::new("⏳ Loading...", false, None));
    let _ = menu.append(&MenuItem::with_id("poll", "🔄 Retry", true, None));
    let _ = menu.append(&mac_ui::tray::quit_item("Quit"));
    let tray_icon = mac_ui::tray::with_icon(TrayIconBuilder::new(), normal_icon.clone(), true)
        .with_menu(Box::new(menu))
        .with_tooltip("Price Ticker")
        .with_title("Ticker")
        .build()?;
    mac_ui::tray::set_accessibility_label(
        &tray_icon,
        &MenuBuilder::tray_accessibility_label("Ticker", false),
    );
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    // Menu clicks go through the event loop proxy instead of `MenuEvent::receiver()`: sending a
    // user event wakes the loop, so a click is never left sitting in the channel until the next
    // timer tick. muda calls this on the main thread, from the menu item action.
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        log_message(&format!("menu: clicked {:?}", event.id.0));
        if menu_proxy.send_event(UserEvent::Menu(event.id.0)).is_err() {
            log_message("menu: event loop closed, click dropped");
        }
    }));
    let mut app = App {
        tray: Rc::new(RefCell::new(tray_icon)),
        fetcher: Some(fetcher),
        proxy: event_loop.create_proxy(),
        poll_gate: PollGate::new(),
        pending_menu: VecDeque::new(),
        menu_deferred_since: None,
        config: None,
        prices: None,
        watch_list: WatchList::new(),
        asset_status: HashMap::new(),
        trigger_gate: TriggerGate::new(),
        watch_save_blocked: false,
        watch_stamp: None,
        rows_generation: 0,
        next_check: SystemTime::now(),
        glyphs: mac_ui::tray::Glyphs {
            normal: normal_icon,
            flash: None,
            alert: Some(alert_icon),
        },
        config_loaded: false,
        config_error: None,
        asleep: false,
    };
    #[cfg(target_os = "macos")]
    let _sleep_wake = {
        let proxy = event_loop.create_proxy();
        mac_ui::wake::observe_sleep_wake(move |power| {
            let asleep = power == mac_ui::wake::Power::WillSleep;
            // Fails only when the event loop has already exited (app quitting).
            let _ = proxy.send_event(UserEvent::Sleep(asleep));
        })
    };
    log_message("menubar: event loop starting");
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn asset(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(name)
    }

    #[test]
    fn normal_icon_becomes_a_cropped_template_glyph() {
        let (rgba, w, h) =
            mac_ui::icon::template_rgba_from_image_file(&asset("normal.png")).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Cropped to the chart glyph: smaller than the 256 px tile, still a real glyph.
        assert!(w < 256 && h < 256, "{w}x{h}");
        assert!(w > 64 && h > 64, "{w}x{h}");
        assert!(rgba.chunks(4).all(|px| px[..3] == [0, 0, 0]));
        let opaque = rgba.chunks(4).filter(|px| px[3] > 128).count();
        let total = (w * h) as usize;
        // The bars and the trend line, not the whole tile.
        assert!(
            opaque > total / 20 && opaque < total / 2,
            "{opaque} of {total}"
        );
    }

    #[test]
    fn bundled_icons_load() {
        assert!(mac_ui::icon::template_from_image_file(&asset("normal.png")).is_ok());
        assert!(mac_ui::icon::from_image_file(&asset("update.png")).is_ok());
    }
}
