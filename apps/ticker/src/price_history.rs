use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};

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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct PriceHistoryFile {
    /// Legacy / last-poll prices (name → snapshot).
    #[serde(default)]
    prices: HashMap<String, PriceSnapshot>,
    /// Day-open prices (name → day open).
    #[serde(default)]
    day_opens: HashMap<String, DayOpen>,
}

/// `None` without a home directory: the history is then kept in memory only ([`MEMORY`]).
fn price_history_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ticker_price_history.json"))
}

/// History of this run when there is no home directory to save it in.
static MEMORY: LazyLock<Mutex<PriceHistoryFile>> = LazyLock::new(Default::default);
static MEMORY_NOTE: Once = Once::new();

/// Run `f` on the stored history (the file, or [`MEMORY`] without a home directory) and save the
/// result when `save` is set.
fn with_history<T>(
    save: bool,
    f: impl FnOnce(&mut PriceHistoryFile) -> T,
) -> Result<T, Box<dyn Error>> {
    match price_history_path() {
        Some(path) => {
            let mut file = load_file(&path);
            let value = f(&mut file);
            if save {
                save_file(&path, &file)?;
            }
            Ok(value)
        }
        None => {
            MEMORY_NOTE.call_once(|| {
                crate::log_message(
                    "price history: no home directory; day opens and last prices are kept in \
                     memory only (lost when Ticker quits)",
                );
            });
            let mut memory = MEMORY
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(f(&mut memory))
        }
    }
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
    with_history(false, |file| todays_opens(file, &today_local())).unwrap_or_default()
}

#[cfg(test)]
pub fn load_day_opens_from(path: &Path) -> HashMap<String, f64> {
    todays_opens(&load_file(path), &today_local())
}

fn todays_opens(file: &PriceHistoryFile, today: &str) -> HashMap<String, f64> {
    file.day_opens
        .iter()
        .filter(|(_, open)| open.date == today)
        .map(|(name, open)| (name.clone(), open.value))
        .collect()
}

/// Persist day opens for today. Only writes entries for the current local date.
pub fn save_day_opens(opens: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    with_history(true, |file| set_day_opens(file, opens, &today_local()))
}

#[cfg(test)]
pub fn save_day_opens_to(path: &Path, opens: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    let mut file = load_file(path);
    set_day_opens(&mut file, opens, &today_local());
    save_file(path, &file)
}

fn set_day_opens(file: &mut PriceHistoryFile, opens: &HashMap<String, f64>, today: &str) {
    // Drop opens from other days, then write today's.
    file.day_opens.retain(|_, open| open.date == today);

    for (name, value) in opens {
        if value.is_nan() || *value == 0.0 {
            continue;
        }
        file.day_opens.insert(
            name.clone(),
            DayOpen {
                date: today.to_string(),
                value: *value,
            },
        );
    }
}

/// Save last-poll prices (legacy helper, still used as optional baseline).
pub fn save_price_history(prices: &HashMap<String, f64>) -> Result<(), Box<dyn Error>> {
    with_history(true, |file| set_prices(file, prices))
}

#[cfg(test)]
pub fn save_price_history_to(
    path: &Path,
    prices: &HashMap<String, f64>,
) -> Result<(), Box<dyn Error>> {
    let mut file = load_file(path);
    set_prices(&mut file, prices);
    save_file(path, &file)
}

fn set_prices(file: &mut PriceHistoryFile, prices: &HashMap<String, f64>) {
    file.prices = prices
        .iter()
        .map(|(name, value)| (name.clone(), PriceSnapshot { value: *value }))
        .collect();
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
    fn memory_store_keeps_todays_opens() {
        let mut file = PriceHistoryFile::default();
        set_day_opens(
            &mut file,
            &HashMap::from([("Gold".to_string(), 2.0)]),
            "2026-10-03",
        );
        set_prices(&mut file, &HashMap::from([("Gold".to_string(), 2.5)]));
        assert_eq!(todays_opens(&file, "2026-10-03").get("Gold"), Some(&2.0));
        assert!(todays_opens(&file, "2026-10-04").is_empty());
        assert_eq!(file.prices.get("Gold").map(|p| p.value), Some(2.5));
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
