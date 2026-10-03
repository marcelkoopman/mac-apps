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

        if total == 0 {
            "No watches".to_string()
        } else if triggered > 0 {
            format!("🔔 {triggered}/{total} triggered")
        } else {
            format!("📊 {total} watches")
        }
    }
}

/// Send a native notification on macOS. Without the app bundle (`cargo run`) the
/// UserNotifications framework cannot be used; a debug build then falls back on `osascript`
/// `display notification`, a release build only logs it (a release always runs from the bundle).
/// That fallback lives here, not in mac-ui: the shared layer starts no subprocesses (AGENTS.md).
#[cfg(target_os = "macos")]
pub fn send_macos_notification(title: &str, message: &str) {
    if mac_ui::notify::send(title, message) {
        return;
    }
    #[cfg(debug_assertions)]
    send_with_osascript(title, message);
    #[cfg(not(debug_assertions))]
    crate::log_message(&format!(
        "notification not shown (no app bundle): {title}: {message}"
    ));
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

/// Ask for notification permission (first launch of the bundled app only) and show
/// notifications while ticker is frontmost too.
#[cfg(target_os = "macos")]
pub fn request_notification_permission() {
    mac_ui::notify::request_authorization();
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn request_notification_permission() {}

/// No-op notification implementation on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn send_macos_notification(_title: &str, _message: &str) {}

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
