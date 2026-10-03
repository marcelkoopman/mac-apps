use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

impl PriceWatch {
    fn same_watch(&self, other: &PriceWatch) -> bool {
        self.asset_name == other.asset_name
            && self.target_price == other.target_price
            && self.direction == other.direction
            && self.created_at == other.created_at
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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

    /// Remove a watch (CLI). The asset name matches case-insensitively, also for non-ASCII
    /// letters.
    pub fn remove_watch(&mut self, asset_name: &str, target_price: f64) -> bool {
        let initial_len = self.watches.len();
        self.watches.retain(|w| {
            !(same_asset(&w.asset_name, asset_name) && (w.target_price - target_price).abs() < 0.01)
        });
        self.watches.len() < initial_len
    }

    /// Remove `watch` (same asset, target, direction and creation time), wherever it is now: the
    /// list may have been reloaded since the menu row was built.
    pub fn remove_matching(&mut self, watch: &PriceWatch) -> bool {
        let before = self.watches.len();
        self.watches.retain(|w| !w.same_watch(watch));
        self.watches.len() < before
    }

    /// Same watches in the same order (ignoring `triggered`): menu rows still point at the
    /// same watches.
    pub fn same_rows(&self, other: &WatchList) -> bool {
        self.watches.len() == other.watches.len()
            && self
                .watches
                .iter()
                .zip(&other.watches)
                .all(|(a, b)| a.same_watch(b))
    }

    /// Filter watches for a single asset (CLI / future UI).
    #[allow(dead_code)]
    pub fn get_watches_for_asset(&self, asset_name: &str) -> Vec<&PriceWatch> {
        self.watches
            .iter()
            .filter(|w| same_asset(&w.asset_name, asset_name))
            .collect()
    }

    /// Check if current price triggers any watches.
    /// Returns triggered watches and updates their state.
    pub fn check_price(&mut self, asset_name: &str, current_price: f64) -> Vec<PriceWatch> {
        let mut triggered = Vec::new();

        for watch in &mut self.watches {
            if same_asset(&watch.asset_name, asset_name) && !watch.triggered {
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
            if same_asset(&watch.asset_name, asset_name)
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

/// Case-insensitive asset name comparison (Unicode lowercase, so "Ölpreis" == "ölpreis").
fn same_asset(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
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

/// Strict load: a missing file is an empty list, an unreadable or corrupt file is an error (and
/// nothing is written).
fn load_strict(path: &Path) -> Result<WatchList, Box<dyn Error>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(WatchList::new()),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_str(&content).map_err(|e| format!("{}: {e}", path.display()).into())
}

/// `<file>.lock` next to the watch file (`~/.ticker_watches.json.lock`).
fn lock_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "watches.json".into());
    path.with_file_name(format!("{name}.lock"))
}

/// Exclusive lock (`File::lock`, blocks until free) on the lock file next to `path`, held while
/// a CLI command or the menu bar app loads, changes and saves the watch list, so neither
/// overwrites the other's change. Released when the returned file is dropped.
fn lock_watch_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path(path))?;
    file.lock()?;
    Ok(file)
}

/// CLI: load, change and (when the list changed) save under the watch-file lock. A corrupt file
/// is an error and is left alone.
pub fn update_watch_list<T>(f: impl FnOnce(&mut WatchList) -> T) -> Result<T, Box<dyn Error>> {
    update_strict_at(&watch_list_path()?, f)
}

fn update_strict_at<T>(
    path: &Path,
    f: impl FnOnce(&mut WatchList) -> T,
) -> Result<T, Box<dyn Error>> {
    let _lock = lock_watch_file(path)?;
    let mut list = load_strict(path)?;
    let before = list.clone();
    let value = f(&mut list);
    if list != before {
        save_to(path, &list)?;
    }
    Ok(value)
}

/// Modification time and length of the watch file: the menu bar app compares it on every poll
/// to pick up changes made by the CLI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// `None` when the file does not exist (or cannot be inspected).
pub fn watch_file_stamp() -> Option<FileStamp> {
    stamp_of(&watch_list_path().ok()?)
}

fn stamp_of(path: &Path) -> Option<FileStamp> {
    let meta = fs::metadata(path).ok()?;
    Some(FileStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Result of [`update_watch_list_for_app`].
#[derive(Debug)]
pub struct AppUpdate<T> {
    /// The list after the change (as saved).
    pub list: WatchList,
    /// The file did not parse: it was moved to this backup (with the parse error) and the change
    /// was applied to an empty list.
    pub recovered: Option<(PathBuf, String)>,
    /// What the change returned.
    pub value: T,
    /// Stamp of the file after the save.
    pub stamp: Option<FileStamp>,
}

/// Menu bar app: load, change and (when the list changed) save under the watch-file lock. Corrupt
/// JSON is moved aside to a `.bak` file instead of being overwritten. `Err` when the file cannot
/// be locked, read or moved: the caller must then not save.
pub fn update_watch_list_for_app<T>(
    f: impl FnOnce(&mut WatchList) -> T,
) -> Result<AppUpdate<T>, Box<dyn Error>> {
    Ok(update_app_at(&watch_list_path()?, f)?)
}

fn update_app_at<T>(path: &Path, f: impl FnOnce(&mut WatchList) -> T) -> io::Result<AppUpdate<T>> {
    let _lock = lock_watch_file(path)?;
    let (mut list, recovered) = match load_or_backup(path)? {
        WatchLoad::Loaded(list) => (list, None),
        WatchLoad::Recovered {
            list,
            backup,
            error,
        } => (list, Some((backup, error))),
    };
    let before = list.clone();
    let value = f(&mut list);
    if list != before {
        save_to(path, &list)?;
    }
    Ok(AppUpdate {
        list,
        recovered,
        value,
        stamp: stamp_of(path),
    })
}

/// Result of [`load_or_backup`].
#[derive(Debug)]
enum WatchLoad {
    /// File read (or missing: empty list).
    Loaded(WatchList),
    /// The file did not parse. It was moved to `backup` and the list starts empty, so later
    /// saves cannot overwrite the user's data.
    Recovered {
        list: WatchList,
        backup: PathBuf,
        error: String,
    },
}

/// Corrupt JSON is moved aside to a `.bak` file instead of being overwritten by the next save.
fn load_or_backup(path: &Path) -> io::Result<WatchLoad> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(WatchLoad::Loaded(WatchList::new()));
        }
        Err(e) => return Err(e),
    };
    match serde_json::from_str(&content) {
        Ok(list) => Ok(WatchLoad::Loaded(list)),
        Err(e) => {
            let backup = backup_path(path);
            fs::rename(path, &backup)?;
            Ok(WatchLoad::Recovered {
                list: WatchList::new(),
                backup,
                error: e.to_string(),
            })
        }
    }
}

/// `<file>.bak`, or `<file>.<n>.bak` when earlier backups exist (never replaces one).
fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "watches.json".into());
    let candidate = path.with_file_name(format!("{name}.bak"));
    if !candidate.exists() {
        return candidate;
    }
    (1u32..)
        .map(|n| path.with_file_name(format!("{name}.{n}.bak")))
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

/// Write to a temporary file next to `path`, then rename over it: a crash mid-write leaves the
/// old file intact instead of a truncated one.
fn save_to(path: &Path, watch_list: &WatchList) -> io::Result<()> {
    let content = serde_json::to_string_pretty(watch_list)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "watches.json".into());
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ticker-watch-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_empty() {
        let path = temp_dir("missing").join("w.json");
        assert!(
            matches!(load_or_backup(&path).unwrap(), WatchLoad::Loaded(l) if l.watches.is_empty())
        );
    }

    #[test]
    fn valid_file_round_trips_through_atomic_save() {
        let path = temp_dir("valid").join("w.json");
        let mut list = WatchList::new();
        list.add_watch("Ölpreis".into(), 80.5, WatchDirection::Below);
        save_to(&path, &list).unwrap();
        let WatchLoad::Loaded(loaded) = load_or_backup(&path).unwrap() else {
            panic!("expected Loaded");
        };
        assert_eq!(loaded.watches, list.watches);
        assert!(!path.with_file_name(".w.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_moved_to_bak_not_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join("w.json");
        fs::write(&path, "{ not json").unwrap();
        let WatchLoad::Recovered {
            list,
            backup,
            error,
        } = load_or_backup(&path).unwrap()
        else {
            panic!("expected Recovered");
        };
        assert!(list.watches.is_empty());
        assert!(!error.is_empty());
        assert_eq!(backup, dir.join("w.json.bak"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{ not json");
        assert!(!path.exists());

        // A second corruption keeps the first backup.
        fs::write(&path, "[").unwrap();
        let WatchLoad::Recovered { backup: second, .. } = load_or_backup(&path).unwrap() else {
            panic!("expected Recovered");
        };
        assert_eq!(second, dir.join("w.json.1.bak"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), "{ not json");
        assert_eq!(fs::read_to_string(&second).unwrap(), "[");
    }

    #[test]
    fn strict_update_saves_only_changes_and_refuses_corrupt_files() {
        let dir = temp_dir("strict");
        let path = dir.join("w.json");
        // No change: nothing is written.
        let n = update_strict_at(&path, |l| l.watches.len()).unwrap();
        assert_eq!(n, 0);
        assert!(!path.exists());
        update_strict_at(&path, |l| {
            l.add_watch("Gold".into(), 2000.0, WatchDirection::Below)
        })
        .unwrap();
        assert_eq!(load_strict(&path).unwrap().watches.len(), 1);
        assert!(dir.join("w.json.lock").exists());
        fs::write(&path, "{ not json").unwrap();
        assert!(update_strict_at(&path, |l| l.watches.clear()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[test]
    fn app_update_merges_with_changes_made_on_disk() {
        let path = temp_dir("merge").join("w.json");
        // The app has loaded an empty list ...
        let first = update_app_at(&path, |_| ()).unwrap();
        assert!(first.list.watches.is_empty());
        // ... then the CLI adds a watch ...
        update_strict_at(&path, |l| {
            l.add_watch("Gold".into(), 2000.0, WatchDirection::Below)
        })
        .unwrap();
        // ... and the app adds its own: both are kept.
        let second = update_app_at(&path, |l| {
            l.add_watch("Bitcoin".into(), 68000.0, WatchDirection::Above)
        })
        .unwrap();
        let names: Vec<_> = second
            .list
            .watches
            .iter()
            .map(|w| w.asset_name.as_str())
            .collect();
        assert_eq!(names, ["Gold", "Bitcoin"]);
        assert_eq!(load_strict(&path).unwrap(), second.list);
        assert_eq!(second.stamp, stamp_of(&path));
    }

    #[test]
    fn app_update_recovers_a_corrupt_file() {
        let dir = temp_dir("app-corrupt");
        let path = dir.join("w.json");
        fs::write(&path, "[").unwrap();
        let update = update_app_at(&path, |l| {
            l.add_watch("Gold".into(), 1.0, WatchDirection::Above)
        })
        .unwrap();
        let (backup, _) = update.recovered.unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "[");
        assert_eq!(load_strict(&path).unwrap().watches.len(), 1);
    }

    #[test]
    fn lock_is_exclusive_across_handles() {
        let path = temp_dir("lock").join("w.json");
        let held = lock_watch_file(&path).unwrap();
        let other = OpenOptions::new()
            .write(true)
            .open(lock_path(&path))
            .unwrap();
        assert!(other.try_lock().is_err());
        drop(held);
        assert!(other.try_lock().is_ok());
    }

    #[test]
    fn stamp_changes_when_the_file_changes() {
        let path = temp_dir("stamp").join("w.json");
        assert_eq!(stamp_of(&path), None);
        fs::write(&path, "{}").unwrap();
        let a = stamp_of(&path).unwrap();
        fs::write(&path, "{\"watches\": []}").unwrap();
        assert_ne!(stamp_of(&path), Some(a));
    }

    #[test]
    fn remove_matching_and_same_rows() {
        let mut list = WatchList::new();
        list.add_watch("Gold".into(), 2000.0, WatchDirection::Below);
        list.add_watch("Bitcoin".into(), 1.0, WatchDirection::Above);
        let mut other = list.clone();
        other.watches[0].triggered = true;
        assert!(list.same_rows(&other));
        let gold = list.watches[0].clone();
        other.watches.reverse();
        assert!(!list.same_rows(&other));
        assert!(other.remove_matching(&gold));
        assert!(!other.remove_matching(&gold));
        assert_eq!(other.watches.len(), 1);
    }

    #[test]
    fn non_ascii_names_match_case_insensitively() {
        let mut list = WatchList::new();
        list.add_watch("Ölpreis".into(), 80.0, WatchDirection::Above);
        assert_eq!(list.check_price("ÖLPREIS", 81.0).len(), 1);
        assert!(list.remove_watch("ölpreis", 80.0));
    }

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
