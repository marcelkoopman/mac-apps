use crate::price_watch::{PriceWatch, WatchDirection, WatchList};

pub struct WatchUIBuilder;

impl WatchUIBuilder {
    /// Format watch trigger notification.
    pub fn format_trigger_notification(watch: &PriceWatch, current_price: f64) -> String {
        let direction_text = match watch.direction {
            WatchDirection::Above => "rose above",
            WatchDirection::Below => "dropped below",
        };

        format!(
            "🔔 {} Price Alert!\n\n\
             {} has {} your watch price of €{:.2}\n\n\
             Current Price: €{:.2}",
            watch.asset_name, watch.asset_name, direction_text, watch.target_price, current_price
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
/// UserNotifications framework cannot be used, so ticker falls back on `osascript` `display
/// notification`. That fallback lives here, not in mac-ui: the shared layer starts no
/// subprocesses (AGENTS.md).
#[cfg(target_os = "macos")]
pub fn send_macos_notification(title: &str, message: &str) {
    if !mac_ui::notify::send(title, message) {
        send_with_osascript(title, message);
    }
}

#[cfg(target_os = "macos")]
fn send_with_osascript(title: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(title)
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .status();
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
    fn escapes_quotes_and_backslashes() {
        assert_eq!(applescript_escape(r#"a "b" \c"#), r#"a \"b\" \\c"#);
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

        let notification = WatchUIBuilder::format_trigger_notification(&watch, 71000.0);

        assert!(notification.contains("rose above"));
        assert!(notification.contains("70000"));
        assert!(notification.contains("71000"));
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

        let notification = WatchUIBuilder::format_trigger_notification(&watch, 1950.0);

        assert!(notification.contains("dropped below"));
        assert!(notification.contains("2000"));
        assert!(notification.contains("1950"));
    }
}
