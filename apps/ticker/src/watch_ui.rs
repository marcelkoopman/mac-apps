use polars::prelude::*;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

use crate::price_watch::{PriceWatch, WatchDirection, WatchList};

pub struct WatchUIBuilder;

impl WatchUIBuilder {
    /// Standalone watch submenu (menu is currently built in `MenuBuilder`).
    #[allow(dead_code)]
    pub fn build_watch_menu(watch_list: &WatchList, _prices_df: &Option<DataFrame>) -> Menu {
        let menu = Menu::new();

        if watch_list.watches.is_empty() {
            let _ = menu.append(&MenuItem::new("No price watches set", false, None));
        } else {
            let _ = menu.append(&MenuItem::new("📊 Price Watches:", false, None));
            let _ = menu.append(&PredefinedMenuItem::separator());

            for watch in &watch_list.watches {
                let status = if watch.triggered { "✓" } else { " " };
                let direction_emoji = watch.direction.emoji();

                let item_text = format!(
                    "[{}] {} {} - €{:.2}",
                    status, direction_emoji, watch.asset_name, watch.target_price
                );

                let item_id = Self::watch_id(watch);

                let _ = menu.append(&MenuItem::with_id(&item_id, &item_text, true, None));
            }
        }

        let _ = menu.append(&PredefinedMenuItem::separator());

        let _ = menu.append(&MenuItem::with_id(
            "add_watch",
            "➕ Add Price Watch",
            true,
            None,
        ));

        let _ = menu.append(&MenuItem::with_id(
            "manage_watches",
            "⚙️ Manage Watches",
            true,
            None,
        ));

        menu
    }

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

    /// Generate a unique ID for a watch menu item (asset name is lowercased).
    #[allow(dead_code)]
    fn watch_id(watch: &PriceWatch) -> String {
        format!(
            "watch_{}_{}",
            watch.asset_name.to_lowercase().replace(' ', "_"),
            watch.target_price
        )
    }

    /// Parse a watch ID back into its asset name and target price.
    /// Asset name is lowercased (same as `watch_id`); callers should match
    /// case-insensitively when removing watches.
    pub fn parse_watch_id(id: &str) -> Option<(String, f64)> {
        let value = id.strip_prefix("watch_")?;
        let (asset_name, price_str) = value.rsplit_once('_')?;

        let asset_name = asset_name.replace('_', " ");
        let price = price_str.parse::<f64>().ok()?;

        Some((asset_name, price))
    }
}

/// Send a native notification on macOS.
#[cfg(target_os = "macos")]
pub fn send_macos_notification(title: &str, message: &str) {
    use std::process::Command;

    let escape_applescript_string = |value: &str| value.replace('\\', "\\\\").replace('"', "\\\"");

    let escaped_title = escape_applescript_string(title);
    let escaped_message = escape_applescript_string(message);

    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escaped_message, escaped_title
    );

    let _ = Command::new("osascript").args(["-e", &script]).status();
}

/// No-op notification implementation on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn send_macos_notification(_title: &str, _message: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_watch_id_generation() {
        let watch = PriceWatch {
            asset_name: "Bitcoin".to_string(),
            target_price: 70000.5,
            direction: WatchDirection::Above,
            created_at: 0,
            triggered: false,
        };

        let id = WatchUIBuilder::watch_id(&watch);

        assert!(id.starts_with("watch_"));
        assert!(id.contains("bitcoin"));

        let parsed = WatchUIBuilder::parse_watch_id(&id);
        assert!(parsed.is_some());

        let (name, price) = parsed.unwrap();

        // IDs store a lowercased asset name; remove_watch matches case-insensitively.
        assert_eq!(name, "bitcoin");
        assert!((price - 70000.5).abs() < 0.01);
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
