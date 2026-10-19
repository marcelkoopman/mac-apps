use crate::menu_builder::MenuBuilder;
use crate::price_watch::{PriceWatch, WatchDirection, WatchList};

pub struct WatchUIBuilder;

impl WatchUIBuilder {
    /// Watch trigger notification; prices in the asset's `unit`.
    pub fn format_trigger_notification(
        watch: &PriceWatch,
        current_price: f64,
        unit: &str,
    ) -> String {
        let direction_text = match watch.direction {
            WatchDirection::Above => "rose above",
            WatchDirection::Below => "dropped below",
        };

        format!(
            "🔔 {} Price Alert!\n\n\
             {} has {} your watch price of {}\n\n\
             Current Price: {}",
            watch.asset_name,
            watch.asset_name,
            direction_text,
            MenuBuilder::format_money(unit, watch.target_price),
            MenuBuilder::format_money(unit, current_price)
        )
    }

    /// Build status indicator for watches.
    pub fn watch_status_indicator(watch_list: &WatchList) -> String {
        let total = watch_list.watches.len();

        let triggered = watch_list
            .watches
            .iter()
            .filter(|watch| watch.triggered)
            .count();

        let watches = if total == 1 { "watch" } else { "watches" };
        if total == 0 {
            "No watches".to_string()
        } else if triggered > 0 {
            format!("{triggered} of {total} {watches} triggered")
        } else {
            format!("{total} {watches}")
        }
    }
}

/// Where a notification is delivered (bundle vs debug fallback vs release silence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyDelivery {
    /// App bundle: `mac_ui::notify` only.
    Native,
    /// No bundle, debug build: `osascript` fallback.
    Osascript,
    /// No bundle, release: nothing (release always ships as a bundle).
    None,
}

/// Permission as ticker uses it for decisions (mirrors `mac_ui::notify::Permission`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyPermission {
    NotDetermined,
    Denied,
    Granted,
}

/// What to do when a price alert fires under a given permission and hint-shown flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceAlertAction {
    /// Post (or request permission then post when still undetermined).
    Deliver,
    /// Notifications are off: show the System Settings hint once.
    ShowHint,
    /// Denied and the hint was already shown: stay silent.
    Skip,
}

/// Bundle → native; without a bundle, debug → osascript, release → none.
pub fn notify_delivery(bundled: bool, debug_assertions: bool) -> NotifyDelivery {
    if bundled {
        NotifyDelivery::Native
    } else if debug_assertions {
        NotifyDelivery::Osascript
    } else {
        NotifyDelivery::None
    }
}

/// Fixed notification id per asset so a new alert replaces the previous instead of stacking.
pub fn price_notification_id(asset_name: &str) -> String {
    format!("ticker.price.{asset_name}")
}

/// Decide how to handle a price alert given the current permission and whether the hint was
/// already persisted.
pub fn price_alert_action(
    permission: NotifyPermission,
    hint_already_shown: bool,
) -> PriceAlertAction {
    match permission {
        NotifyPermission::Granted | NotifyPermission::NotDetermined => PriceAlertAction::Deliver,
        NotifyPermission::Denied if !hint_already_shown => PriceAlertAction::ShowHint,
        NotifyPermission::Denied => PriceAlertAction::Skip,
    }
}

/// System Settings › Notifications deep link (Ventura+).
pub const NOTIFICATIONS_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension";

/// Send a native notification on macOS. Without the app bundle (`cargo run`) the
/// UserNotifications framework cannot be used; a debug build then falls back on `osascript`
/// `display notification`, a release build only logs it (a release always runs from the bundle).
/// That fallback lives here, not in mac-ui: the shared layer starts no subprocesses (AGENTS.md).
/// Does not use a fixed id (general status messages); price alerts use [`send_price_alert`].
#[cfg(target_os = "macos")]
pub fn send_macos_notification(title: &str, message: &str) {
    match notify_delivery(mac_ui::notify::is_bundled(), cfg!(debug_assertions)) {
        NotifyDelivery::Native => {
            let _ = mac_ui::notify::send(title, message);
        }
        NotifyDelivery::Osascript => {
            #[cfg(debug_assertions)]
            send_with_osascript(title, message);
        }
        NotifyDelivery::None => {
            // Do not log the body: it may hold prices.
            crate::log_message(&format!("notification not shown (no app bundle): {title}"));
        }
    }
}

/// Price-watch alert: fixed id per asset, permission check, one-shot System Settings hint when
/// notifications are denied.
#[cfg(target_os = "macos")]
pub fn send_price_alert(asset_name: &str, title: &str, message: &str) {
    let delivery = notify_delivery(mac_ui::notify::is_bundled(), cfg!(debug_assertions));
    match delivery {
        NotifyDelivery::Osascript => {
            #[cfg(debug_assertions)]
            send_with_osascript(title, message);
        }
        NotifyDelivery::None => {
            crate::log_message(&format!("price alert not shown (no app bundle): {title}"));
        }
        NotifyDelivery::Native => {
            let id = price_notification_id(asset_name);
            let title = title.to_string();
            let message = message.to_string();
            mac_ui::notify::permission(move |permission| {
                let mapped = match permission {
                    mac_ui::notify::Permission::NotDetermined => NotifyPermission::NotDetermined,
                    mac_ui::notify::Permission::Denied => NotifyPermission::Denied,
                    mac_ui::notify::Permission::Granted => NotifyPermission::Granted,
                };
                let hint_shown = crate::config::load_notify_hint_shown();
                match price_alert_action(mapped, hint_shown) {
                    PriceAlertAction::Deliver => match mapped {
                        NotifyPermission::Granted => {
                            let _ = mac_ui::notify::post(&id, &title, &message);
                        }
                        NotifyPermission::NotDetermined => {
                            let id = id.clone();
                            let title = title.clone();
                            let message = message.clone();
                            mac_ui::notify::request_permission(move |granted| {
                                if granted {
                                    let _ = mac_ui::notify::post(&id, &title, &message);
                                }
                            });
                        }
                        NotifyPermission::Denied => {}
                    },
                    PriceAlertAction::ShowHint => {
                        // Persist first so a second alert in the same session does not stack hints.
                        let _ = crate::config::save_notify_hint_shown();
                        request_show_notify_hint();
                    }
                    PriceAlertAction::Skip => {}
                }
            });
        }
    }
}

/// AppleScript for the debug fallback. Title and body arrive as `argv` of the run handler, so
/// they are never parsed as AppleScript (no escaping to get wrong).
#[cfg(any(test, all(target_os = "macos", debug_assertions)))]
const NOTIFY_SCRIPT: &str =
    "on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run";

/// Arguments for `osascript`: the script, then title and body as its `argv`.
#[cfg(any(test, all(target_os = "macos", debug_assertions)))]
fn osascript_args<'a>(title: &'a str, body: &'a str) -> [&'a str; 4] {
    ["-e", NOTIFY_SCRIPT, title, body]
}

#[cfg(all(target_os = "macos", debug_assertions))]
fn send_with_osascript(title: &str, body: &str) {
    let _ = std::process::Command::new("osascript")
        .args(osascript_args(title, body))
        .status();
}

/// Ask for notification permission (first launch of the bundled app only), install the click
/// handler that opens the tray menu, and show notifications while ticker is frontmost too.
#[cfg(target_os = "macos")]
pub fn request_notification_permission() {
    mac_ui::notify::set_click_handler(on_notification_click);
    mac_ui::notify::request_authorization();
}

/// UI actions that must run on the main thread (event loop), requested from notification
/// callbacks on a system queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyUi {
    /// Open the status item menu (notification click).
    OpenMenu,
    /// Show the one-shot "notifications are off" hint dialog.
    ShowHint,
}

/// Install the handler for [`NotifyUi`] (Send: notification callbacks are not on the main
/// thread). Typically sends a winit user event that the menubar handles on the main thread.
#[cfg(target_os = "macos")]
type NotifyUiHandler = std::sync::Arc<dyn Fn(NotifyUi) + Send + Sync>;

#[cfg(target_os = "macos")]
pub fn set_notify_ui_handler(f: impl Fn(NotifyUi) + Send + Sync + 'static) {
    *NOTIFY_UI
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(std::sync::Arc::new(f));
}

#[cfg(target_os = "macos")]
static NOTIFY_UI: std::sync::Mutex<Option<NotifyUiHandler>> = std::sync::Mutex::new(None);

#[cfg(target_os = "macos")]
fn emit_notify_ui(event: NotifyUi) {
    let handler = NOTIFY_UI
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(handler) = handler {
        handler(event);
    }
}

/// Notification click → ask the app to open the status item menu.
#[cfg(target_os = "macos")]
fn on_notification_click(_id: &str) {
    emit_notify_ui(NotifyUi::OpenMenu);
}

#[cfg(target_os = "macos")]
fn request_show_notify_hint() {
    emit_notify_ui(NotifyUi::ShowHint);
}

#[cfg(target_os = "macos")]
pub fn show_notify_hint() {
    let open = crate::dialogs::button("open_settings");
    let close = crate::dialogs::button("close");
    let choice = crate::dialogs::buttons(
        crate::dialogs::text("notify_hint_title"),
        crate::dialogs::text("notify_hint_message"),
        &[open, close],
    );
    if choice == Some(0) {
        open_notifications_settings();
    }
}

/// Open System Settings › Notifications via NSWorkspace (no `open` subprocess).
#[cfg(target_os = "macos")]
pub fn open_notifications_settings() -> bool {
    use mac_ui::objc2_app_kit::NSWorkspace;
    use mac_ui::objc2_foundation::{NSString, NSURL};

    let Some(nsurl) = NSURL::URLWithString(&NSString::from_str(NOTIFICATIONS_SETTINGS_URL)) else {
        return false;
    };
    NSWorkspace::sharedWorkspace().openURL(&nsurl)
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn request_notification_permission() {}

/// No-op notification implementation on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn send_macos_notification(_title: &str, _message: &str) {}

#[cfg(not(target_os = "macos"))]
pub fn send_price_alert(_asset_name: &str, _title: &str, _message: &str) {}

#[cfg(not(target_os = "macos"))]
pub fn set_notify_ui_handler(_f: impl Fn(NotifyUi) + Send + Sync + 'static) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osascript_gets_text_as_arguments_not_as_script() {
        let body = r#"BTC "rose" \ end tell"#;
        let args = osascript_args("Ticker", body);
        assert_eq!(args, ["-e", NOTIFY_SCRIPT, "Ticker", body]);
        assert!(NOTIFY_SCRIPT.starts_with("on run argv"));
        assert!(!NOTIFY_SCRIPT.contains(body));
    }

    #[test]
    fn bundled_uses_native_delivery() {
        assert_eq!(notify_delivery(true, true), NotifyDelivery::Native);
        assert_eq!(notify_delivery(true, false), NotifyDelivery::Native);
    }

    #[test]
    fn without_bundle_debug_uses_osascript_release_is_silent() {
        assert_eq!(notify_delivery(false, true), NotifyDelivery::Osascript);
        assert_eq!(notify_delivery(false, false), NotifyDelivery::None);
    }

    #[test]
    fn price_id_is_stable_per_asset_and_differs_across_assets() {
        assert_eq!(price_notification_id("Bitcoin"), "ticker.price.Bitcoin");
        assert_eq!(price_notification_id("Gold"), "ticker.price.Gold");
        assert_eq!(price_notification_id("TTF Gas"), "ticker.price.TTF Gas");
        assert_ne!(
            price_notification_id("Bitcoin"),
            price_notification_id("Gold")
        );
    }

    #[test]
    fn hint_shown_once_when_denied() {
        assert_eq!(
            price_alert_action(NotifyPermission::Denied, false),
            PriceAlertAction::ShowHint
        );
        assert_eq!(
            price_alert_action(NotifyPermission::Denied, true),
            PriceAlertAction::Skip
        );
    }

    #[test]
    fn granted_and_undetermined_deliver() {
        assert_eq!(
            price_alert_action(NotifyPermission::Granted, false),
            PriceAlertAction::Deliver
        );
        assert_eq!(
            price_alert_action(NotifyPermission::Granted, true),
            PriceAlertAction::Deliver
        );
        assert_eq!(
            price_alert_action(NotifyPermission::NotDetermined, false),
            PriceAlertAction::Deliver
        );
    }

    #[test]
    fn settings_url_points_at_notifications_extension() {
        assert!(NOTIFICATIONS_SETTINGS_URL.contains("Notifications-Settings"));
        assert!(NOTIFICATIONS_SETTINGS_URL.starts_with("x-apple.systempreferences:"));
    }

    #[test]
    fn test_watch_status_indicator_no_watches() {
        let list = WatchList::new();
        let status = WatchUIBuilder::watch_status_indicator(&list);

        assert_eq!(status, "No watches");
    }

    #[test]
    fn test_watch_status_indicator_with_watches() {
        let mut list = WatchList::new();

        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);

        list.add_watch("Bitcoin".to_string(), 65000.0, WatchDirection::Below);

        let status = WatchUIBuilder::watch_status_indicator(&list);

        assert!(status.contains("2 watches"));
    }

    #[test]
    fn test_format_trigger_notification_above() {
        let watch = PriceWatch {
            asset_name: "Bitcoin".to_string(),
            target_price: 70000.0,
            direction: WatchDirection::Above,
            created_at: 0,
            triggered: true,
        };

        let notification = WatchUIBuilder::format_trigger_notification(&watch, 71000.0, "EUR");

        assert!(notification.contains("rose above"));
        assert!(notification.contains("€70.000,00"));
        assert!(notification.contains("€71.000,00"));
    }

    #[test]
    fn test_format_trigger_notification_below() {
        let watch = PriceWatch {
            asset_name: "Gold".to_string(),
            target_price: 2000.0,
            direction: WatchDirection::Below,
            created_at: 0,
            triggered: true,
        };

        let notification = WatchUIBuilder::format_trigger_notification(&watch, 1950.0, "USD");

        assert!(notification.contains("dropped below"));
        assert!(notification.contains("$2.000,00"));
        assert!(notification.contains("$1.950,00"));
        assert!(!notification.contains('€'));
    }
}
