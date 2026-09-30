use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceWatch {
    pub asset_name: String,
    pub target_price: f64,
    pub direction: WatchDirection,
    /// Timestamp when watch was created
    pub created_at: i64,
    /// Track if we've already triggered this watch to avoid spam
    pub triggered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WatchDirection {
    #[serde(rename = "above")]
    Above,
    #[serde(rename = "below")]
    Below,
}

impl WatchDirection {
    pub fn as_str(&self) -> &str {
        match self {
            WatchDirection::Above => "above",
            WatchDirection::Below => "below",
        }
    }

    pub fn emoji(&self) -> &str {
        match self {
            WatchDirection::Above => "📈",
            WatchDirection::Below => "📉",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchList {
    pub watches: Vec<PriceWatch>,
}

impl WatchList {
    pub fn new() -> Self {
        WatchList {
            watches: Vec::new(),
        }
    }

    pub fn add_watch(&mut self, asset_name: String, target_price: f64, direction: WatchDirection) {
        let watch = PriceWatch {
            asset_name,
            target_price,
            direction,
            created_at: chrono::Local::now().timestamp(),
            triggered: false,
        };
        self.watches.push(watch);
    }

    /// Remove a watch. Asset name match is case-insensitive because menu item
    /// IDs store a lowercased form of the name.
    pub fn remove_watch(&mut self, asset_name: &str, target_price: f64) -> bool {
        let initial_len = self.watches.len();
        self.watches.retain(|w| {
            !(w.asset_name.eq_ignore_ascii_case(asset_name)
                && (w.target_price - target_price).abs() < 0.01)
        });
        self.watches.len() < initial_len
    }

    /// Filter watches for a single asset (CLI / future UI).
    #[allow(dead_code)]
    pub fn get_watches_for_asset(&self, asset_name: &str) -> Vec<&PriceWatch> {
        self.watches
            .iter()
            .filter(|w| w.asset_name.eq_ignore_ascii_case(asset_name))
            .collect()
    }

    /// Check if current price triggers any watches.
    /// Returns triggered watches and updates their state.
    pub fn check_price(&mut self, asset_name: &str, current_price: f64) -> Vec<PriceWatch> {
        let mut triggered = Vec::new();

        for watch in &mut self.watches {
            if watch.asset_name.eq_ignore_ascii_case(asset_name) && !watch.triggered {
                let should_trigger = match watch.direction {
                    WatchDirection::Above => current_price >= watch.target_price,
                    WatchDirection::Below => current_price <= watch.target_price,
                };

                if should_trigger {
                    watch.triggered = true;
                    triggered.push(watch.clone());
                }
            }
        }

        triggered
    }

    /// Reset triggered state for a watch (e.g. new trading day).
    #[allow(dead_code)]
    pub fn reset_watch_state(&mut self, asset_name: &str, target_price: f64) {
        for watch in &mut self.watches {
            if watch.asset_name.eq_ignore_ascii_case(asset_name)
                && (watch.target_price - target_price).abs() < 0.01
            {
                watch.triggered = false;
            }
        }
    }

    /// Reset all triggered states.
    #[allow(dead_code)]
    pub fn reset_all_states(&mut self) {
        for watch in &mut self.watches {
            watch.triggered = false;
        }
    }
}

fn watch_list_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(path) = std::env::var("TICKER_WATCHES_PATH")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    Ok(home.join(".ticker_watches.json"))
}

pub fn load_watch_list() -> Result<WatchList, Box<dyn Error>> {
    let path = watch_list_path()?;

    if !path.exists() {
        return Ok(WatchList::new());
    }

    let content = fs::read_to_string(&path)?;
    let watch_list: WatchList = serde_json::from_str(&content)?;
    Ok(watch_list)
}

pub fn save_watch_list(watch_list: &WatchList) -> Result<(), Box<dyn Error>> {
    let path = watch_list_path()?;
    let content = serde_json::to_string_pretty(watch_list)?;
    fs::write(&path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_watch() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);

        assert_eq!(list.watches.len(), 1);
        assert_eq!(list.watches[0].asset_name, "Bitcoin");
        assert_eq!(list.watches[0].target_price, 70000.0);
    }

    #[test]
    fn test_remove_watch() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        list.add_watch("Gold".to_string(), 2000.0, WatchDirection::Below);

        assert!(list.remove_watch("Bitcoin", 70000.0));
        assert_eq!(list.watches.len(), 1);
        assert!(!list.remove_watch("Bitcoin", 70000.0));
    }

    #[test]
    fn test_remove_watch_case_insensitive() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);

        assert!(list.remove_watch("bitcoin", 70000.0));
        assert!(list.watches.is_empty());
    }

    #[test]
    fn test_get_watches_for_asset() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        list.add_watch("Bitcoin".to_string(), 65000.0, WatchDirection::Below);
        list.add_watch("Gold".to_string(), 2000.0, WatchDirection::Above);

        let btc_watches = list.get_watches_for_asset("Bitcoin");
        assert_eq!(btc_watches.len(), 2);
    }

    #[test]
    fn test_check_price_above() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);

        let triggered = list.check_price("Bitcoin", 71000.0);
        assert_eq!(triggered.len(), 1);
        assert!(list.watches[0].triggered);

        // Should not trigger again
        let triggered_again = list.check_price("Bitcoin", 72000.0);
        assert_eq!(triggered_again.len(), 0);
    }

    #[test]
    fn test_check_price_below() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 65000.0, WatchDirection::Below);

        let triggered = list.check_price("Bitcoin", 64000.0);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].direction, WatchDirection::Below);
    }

    #[test]
    fn test_reset_watch_state() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        list.check_price("Bitcoin", 71000.0);

        assert!(list.watches[0].triggered);
        list.reset_watch_state("Bitcoin", 70000.0);
        assert!(!list.watches[0].triggered);
    }

    #[test]
    fn test_watch_direction_display() {
        assert_eq!(WatchDirection::Above.as_str(), "above");
        assert_eq!(WatchDirection::Below.as_str(), "below");
        assert_eq!(WatchDirection::Above.emoji(), "📈");
        assert_eq!(WatchDirection::Below.emoji(), "📉");
    }

    #[test]
    fn test_check_price_exact_target_triggers() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        let triggered = list.check_price("Bitcoin", 70000.0);
        assert_eq!(triggered.len(), 1);
        assert!(list.watches[0].triggered);

        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 65000.0, WatchDirection::Below);
        let triggered = list.check_price("Bitcoin", 65000.0);
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_check_price_just_shy_of_target_does_not_fire() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        let triggered = list.check_price("Bitcoin", 69999.99);
        assert!(triggered.is_empty());
        assert!(!list.watches[0].triggered);
    }

    #[test]
    fn test_check_price_other_asset_ignored() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        let triggered = list.check_price("Gold", 80000.0);
        assert!(triggered.is_empty());
        assert!(!list.watches[0].triggered);
    }

    #[test]
    fn test_remove_watch_near_miss_tolerance() {
        let mut list = WatchList::new();
        list.add_watch("Bitcoin".to_string(), 70000.0, WatchDirection::Above);
        assert!(!list.remove_watch("Bitcoin", 70000.02));
        assert_eq!(list.watches.len(), 1);
        assert!(list.remove_watch("Bitcoin", 70000.005));
        assert!(list.watches.is_empty());
    }

    #[test]
    fn watch_list_json_roundtrip() {
        let mut list = WatchList::new();
        list.add_watch("Gold".to_string(), 2100.5, WatchDirection::Below);
        let json = serde_json::to_string(&list).unwrap();
        let loaded: WatchList = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.watches.len(), 1);
        assert_eq!(loaded.watches[0].asset_name, "Gold");
        assert_eq!(loaded.watches[0].direction, WatchDirection::Below);
        assert!((loaded.watches[0].target_price - 2100.5).abs() < f64::EPSILON);
    }
}
