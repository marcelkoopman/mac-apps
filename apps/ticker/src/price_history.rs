use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Last known poll price (used as fallback / legacy).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PriceSnapshot {
    pub value: f64,
}

/// First observed price of the local calendar day for an asset.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DayOpen {
    /// Local date as `YYYY-MM-DD`.
    pub date: String,
    pub value: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PriceHistoryFile {
    /// Legacy / last-poll prices (name → snapshot).
    #[serde(default)]
    prices: HashMap<String, PriceSnapshot>,
    /// Day-open prices (name → day open).
    #[serde(default)]
    day_opens: HashMap<String, DayOpen>,
}

fn price_history_path() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot find home directory");
    home.join(".ticker_price_history.json")
}

fn today_local() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn load_file(path: &Path) -> PriceHistoryFile {
    match fs::read_to_string(path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => PriceHistoryFile::default(),
    }
}

fn save_file(path: &Path, history: &PriceHistoryFile) -> Result<(), Box<dyn Error>> {
    let data = serde_json::to_string_pretty(history)?;
    fs::write(path, data)?;
    Ok(())
}

/// Load day-open prices for today.
///
/// - If a stored open is from today → keep it.
/// - If missing or from a previous day → not returned (caller should set current price as open).
pub fn load_day_opens() -> HashMap<String, f64> {
    load_day_opens_from(&price_history_path())
}

pub fn load_day_opens_from(path: &Path) -> HashMap<String, f64> {
    let today = today_local();
    let file = load_file(path);
    file.day_opens
        .into_iter()
        .filter(|(_, open)| open.date == today)
        .map(|(name, open)| (name, open.value))
        .collect()
}

/// Persist day opens for today. Only writes entries for the current local date.
pub fn save_day_opens(opens: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    save_day_opens_to(&price_history_path(), opens)
}

pub fn save_day_opens_to(path: &Path, opens: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    let today = today_local();
    let mut file = load_file(path);

    // Drop opens from other days, then write today's.
    file.day_opens.retain(|_, open| open.date == today);

    for (name, value) in opens {
        if value.is_nan() || *value == 0.0 {
            continue;
        }
        file.day_opens.insert(
            name.clone(),
            DayOpen {
                date: today.clone(),
                value: *value,
            },
        );
    }

    save_file(path, &file)
}

/// Save last-poll prices (legacy helper, still used as optional baseline).
pub fn save_price_history(prices: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    save_price_history_to(&price_history_path(), prices)
}

pub fn save_price_history_to(
    path: &Path,
    prices: &HashMap<String, f64>,
) -> Result<(), Box<dyn Error>> {
    let mut file = load_file(path);
    file.prices = prices
        .iter()
        .map(|(name, value)| (name.clone(), PriceSnapshot { value: *value }))
        .collect();
    save_file(path, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("ticker_test_{name}.json"))
    }

    #[test]
    fn day_open_roundtrip_same_day() {
        let path = temp_path("day_open_roundtrip");
        let _ = fs::remove_file(&path);

        let mut opens = HashMap::new();
        opens.insert("Bitcoin".to_string(), 94000.0);
        save_day_opens_to(&path, &opens).unwrap();

        let loaded = load_day_opens_from(&path);
        assert_eq!(loaded.get("Bitcoin"), Some(&94000.0));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_empty() {
        let path = temp_path("missing_file");
        let _ = fs::remove_file(&path);
        let loaded = load_day_opens_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn day_open_from_other_date_is_ignored() {
        let path = temp_path("stale_day_open");
        let _ = fs::remove_file(&path);

        let file = PriceHistoryFile {
            prices: HashMap::new(),
            day_opens: HashMap::from([(
                "Bitcoin".to_string(),
                DayOpen {
                    date: "1999-01-01".to_string(),
                    value: 1.0,
                },
            )]),
        };
        save_file(&path, &file).unwrap();

        let loaded = load_day_opens_from(&path);
        assert!(
            loaded.is_empty(),
            "opens from another calendar day must be dropped"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_day_opens_skips_nan_and_zero() {
        let path = temp_path("skip_nan_zero");
        let _ = fs::remove_file(&path);

        let mut opens = HashMap::new();
        opens.insert("Bitcoin".to_string(), f64::NAN);
        opens.insert("Gold".to_string(), 0.0);
        opens.insert("Gas".to_string(), 35.5);
        save_day_opens_to(&path, &opens).unwrap();

        let loaded = load_day_opens_from(&path);
        assert_eq!(loaded.get("Gas"), Some(&35.5));
        assert!(!loaded.contains_key("Bitcoin"));
        assert!(!loaded.contains_key("Gold"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn last_poll_history_roundtrip() {
        let path = temp_path("last_poll");
        let _ = fs::remove_file(&path);

        let mut prices = HashMap::new();
        prices.insert("Bitcoin".to_string(), 94000.0);
        save_price_history_to(&path, &prices).unwrap();

        let file = load_file(&path);
        assert_eq!(file.prices.get("Bitcoin").map(|s| s.value), Some(94000.0));
        let _ = fs::remove_file(&path);
    }
}
