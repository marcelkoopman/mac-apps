use crate::atomic_file::write_atomic;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Name of the asset shown in the menu-bar title. Defaults to first priced row.
    #[serde(default)]
    pub menubar_asset: Option<String>,
    pub assets: Vec<Asset>,
}

impl Config {
    pub fn menubar_asset_name(&self) -> Option<&str> {
        self.menubar_asset.as_deref().filter(|s| !s.is_empty())
    }

    /// The assets that are polled: those without an [`asset_problem`].
    pub fn fetchable_assets(&self) -> Vec<Asset> {
        self.assets
            .iter()
            .filter(|a| asset_problem(a).is_none())
            .cloned()
            .collect()
    }

    /// Assets left out of polling, with the reason (one error line each in the menu). They stay
    /// in the config, so *Edit asset…* can fix them.
    pub fn skipped_assets(&self) -> Vec<SkippedAsset> {
        self.assets
            .iter()
            .filter_map(|a| {
                Some(SkippedAsset {
                    name: a.name.clone(),
                    reason: asset_problem(a)?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkippedAsset {
    pub name: String,
    pub reason: String,
}

/// Why `asset` cannot be polled, if it cannot: a URL that fails [`validate_asset_url`] (https
/// only) or an empty price path. One bad asset is skipped instead of failing the whole config.
pub fn asset_problem(asset: &Asset) -> Option<String> {
    if let Err(e) = validate_asset_url(&asset.url) {
        return Some(e);
    }
    if asset.price_path.trim().is_empty() {
        return Some("Price path cannot be empty".into());
    }
    None
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub price_path: String,
    pub unit: String,
    pub unit_hint: String,
    pub symbol: String,
    /// Prices at or below zero are real for this asset (day-ahead power), not a fetch glitch;
    /// they may set off watches, and targets at or below zero are accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_negative: Option<bool>,
    /// Largest move (in %) from the last plausible price that may set off a watch at once; a
    /// bigger jump only counts once the next poll confirms it. Default [`DEFAULT_MAX_JUMP_PCT`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_jump_pct: Option<f64>,
}

/// [`Asset::max_jump_pct`] when the config does not set it.
pub const DEFAULT_MAX_JUMP_PCT: f64 = 25.0;
/// What the bundled config (and the migration of an older user config) sets for day-ahead power,
/// whose hourly or quarter-hourly price can triple, or cross zero, from one period to the next.
const POWER_MAX_JUMP_PCT: f64 = 300.0;

impl Asset {
    pub fn allows_negative(&self) -> bool {
        self.allow_negative.unwrap_or(false)
    }

    /// [`Asset::max_jump_pct`], or the default when it is missing or not a positive number.
    pub fn max_jump(&self) -> f64 {
        self.max_jump_pct
            .filter(|p| p.is_finite() && *p > 0.0)
            .unwrap_or(DEFAULT_MAX_JUMP_PCT)
    }
}

pub fn bundled_config_path() -> Result<PathBuf, Box<dyn Error>> {
    let exe_path = std::env::current_exe()?;

    if let Some(app_dir) = exe_path.ancestors().find(|p| {
        p.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".app"))
            .unwrap_or(false)
    }) {
        return Ok(app_dir.join("Contents/Resources/config.toml"));
    }

    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.toml"))
}

pub fn user_config_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(path) = std::env::var("TICKER_USER_CONFIG_PATH")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    Ok(home.join(".ticker_config.toml"))
}

pub fn parse_config(config_str: &str) -> Result<Config, Box<dyn Error>> {
    toml::from_str(config_str).map_err(|e| e.into())
}

pub fn load_config_from(path: &Path) -> Result<Config, Box<dyn Error>> {
    let config_str =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
    let mut config = parse_config(&config_str)?;
    migrate_power_path(&mut config);
    for skipped in config.skipped_assets() {
        crate::log_message(&format!(
            "config: {:?} skipped: {}",
            skipped.name, skipped.reason
        ));
    }
    Ok(config)
}

/// Power price path of the default config before `@now` existed: `data.0` is always the 00:00
/// entry of the day-ahead list, so the menu showed midnight's price all day. A user config written
/// by *Edit asset…* still has it; switch it to the entry of the current period.
const OLD_POWER_PATH: &str = "data.0.price";
const POWER_HOST: &str = "dap.xadi.eu";

/// Also gives a power asset written before `allow_negative` / `max_jump_pct` existed the bundled
/// values (only where the file does not set them).
fn migrate_power_path(config: &mut Config) {
    for asset in &mut config.assets {
        let on_power_host = reqwest::Url::parse(&asset.url)
            .ok()
            .is_some_and(|url| url.host_str() == Some(POWER_HOST));
        if !on_power_host {
            continue;
        }
        if asset.price_path == OLD_POWER_PATH {
            crate::log_message(&format!(
                "config: {}: price path {OLD_POWER_PATH} → data.@now.price (current period)",
                asset.name
            ));
            asset.price_path = "data.@now.price".to_string();
        }
        if asset.allow_negative.is_none() {
            asset.allow_negative = Some(true);
        }
        if asset.max_jump_pct.is_none() {
            asset.max_jump_pct = Some(POWER_MAX_JUMP_PCT);
        }
    }
}

pub fn load_config() -> Result<Config, Box<dyn Error>> {
    eprintln!("📋 Looking for config.toml...");
    // Without a home directory there is no user config: use the bundled one.
    let user = user_config_path().ok().filter(|user| user.exists());
    let path = if let Some(user) = user {
        eprintln!("📂 Reading user config from: {:?}", user);
        user
    } else {
        let bundled = bundled_config_path()?;
        eprintln!("📂 Reading bundled config from: {:?}", bundled);
        bundled
    };
    let mut config = load_config_from(&path)?;
    if let Some(pin) = load_menubar_pin() {
        config.menubar_asset = Some(pin);
    }
    Ok(config)
}

pub fn save_user_config(config: &Config) -> Result<PathBuf, Box<dyn Error>> {
    let path = user_config_path()?;
    let body = toml::to_string_pretty(config)?;
    write_atomic(&path, body.as_bytes())?;
    Ok(path)
}

pub fn reset_user_config() -> Result<Config, Box<dyn Error>> {
    let path = user_config_path()?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    load_config()
}

/// Overwrite fetch settings for an asset. URL may be a completely different endpoint.
pub fn apply_asset_edit(
    asset: &mut Asset,
    url: &str,
    unit: &str,
    price_path: &str,
) -> Result<(), String> {
    let url = url.trim();
    let unit = unit.trim();
    let price_path = price_path.trim();
    validate_asset_url(url)?;
    if unit.is_empty() {
        return Err("Unit cannot be empty".into());
    }
    if price_path.is_empty() {
        return Err("Price path cannot be empty".into());
    }
    asset.url = url.to_string();
    asset.unit = unit.to_uppercase();
    asset.price_path = price_path.to_string();
    Ok(())
}

/// Longest URL accepted by *Edit asset…*.
const MAX_URL_LEN: usize = 2048;

/// URL check for *Edit asset…* and at config load: `https://` with a host, no credentials, no whitespace. The
/// bundled config only uses https; plain http would let anyone on the network change prices
/// (and, through watches, trigger alerts).
pub fn validate_asset_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL cannot be empty".into());
    }
    if url.len() > MAX_URL_LEN {
        return Err(format!("URL is longer than {MAX_URL_LEN} characters"));
    }
    if url.chars().any(char::is_whitespace) {
        return Err("URL cannot contain spaces".into());
    }
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("URL must start with https://".into());
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("URL has no host".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URL cannot contain a user name or password".into());
    }
    Ok(())
}

/// Poll interval when none was set: 5 minutes.
pub const DEFAULT_POLL_MINUTES: u64 = 5;

fn poll_interval_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ticker_poll_interval"))
}

/// Minutes between polls set with *Poll interval…*, or [`DEFAULT_POLL_MINUTES`]. A file that
/// does not hold a valid value (see `price_input::parse_interval_minutes`) is ignored.
pub fn load_poll_minutes() -> u64 {
    poll_interval_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|raw| crate::price_input::parse_interval_minutes(&raw).ok())
        .unwrap_or(DEFAULT_POLL_MINUTES)
}

pub fn save_poll_minutes(minutes: u64) -> Result<(), Box<dyn Error>> {
    let path = poll_interval_path().ok_or("Cannot find home directory")?;
    write_atomic(&path, format!("{minutes}\n").as_bytes())?;
    Ok(())
}

fn menubar_pin_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Ok(path) = std::env::var("TICKER_MENUBAR_PIN_PATH")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    Ok(home.join(".ticker_menubar_asset"))
}

/// Last asset the user pinned by clicking a price row.
pub fn load_menubar_pin() -> Option<String> {
    let path = menubar_pin_path().ok()?;
    let raw = fs::read_to_string(path).ok()?;
    let name = raw.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn save_menubar_pin(name: &str) -> Result<(), Box<dyn Error>> {
    let path = menubar_pin_path()?;
    write_atomic(&path, name.trim().as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_toml() -> &'static str {
        r#"
[[assets]]
name = "Bitcoin"
url = "https://example.com/btc"
price_path = "bitcoin.eur"
unit = "EUR"
unit_hint = "/BTC"
symbol = "💰"

[[assets]]
name = "Gold"
url = "https://example.com/gold"
price_path = "xau.price"
unit = "EUR"
unit_hint = "/troy oz"
symbol = "🥇"
"#
    }

    fn gecko_btc() -> Asset {
        Asset {
            name: "Bitcoin".into(),
            url: "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=eur"
                .into(),
            price_path: "bitcoin.eur".into(),
            unit: "EUR".into(),
            unit_hint: "/BTC".into(),
            symbol: "💰".into(),
            allow_negative: None,
            max_jump_pct: None,
        }
    }

    #[test]
    fn parse_config_valid() {
        let config = parse_config(sample_toml()).expect("should parse");
        assert_eq!(config.assets.len(), 2);
        assert_eq!(config.assets[0].name, "Bitcoin");
        assert_eq!(config.assets[0].price_path, "bitcoin.eur");
        assert_eq!(config.assets[0].symbol, "💰");
        assert_eq!(config.assets[1].name, "Gold");
        assert_eq!(config.assets[1].unit, "EUR");
        assert!(config.menubar_asset.is_none());
    }

    #[test]
    fn parse_config_menubar_asset() {
        let toml = "menubar_asset = \"Gold\"\n\n[[assets]]\nname = \"Gold\"\nurl = \"https://example.com\"\nprice_path = \"xau.price\"\nunit = \"EUR\"\nunit_hint = \"/oz\"\nsymbol = \"🥇\"\n";
        let config = parse_config(toml).expect("should parse");
        assert_eq!(config.menubar_asset_name(), Some("Gold"));
    }

    #[test]
    fn parse_config_empty_assets() {
        let config = parse_config("assets = []").expect("should parse");
        assert!(config.assets.is_empty());
    }

    #[test]
    fn parse_config_invalid_toml() {
        assert!(parse_config("not valid toml {{{{").is_err());
    }

    #[test]
    fn parse_config_missing_required_field() {
        let bad = r#"
[[assets]]
name = "Bitcoin"
url = "https://example.com"
"#;
        assert!(parse_config(bad).is_err());
    }

    #[test]
    fn load_config_from_file() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ticker-config-test-{}.toml", stamp));
        fs::write(&path, sample_toml()).unwrap();

        let config = load_config_from(&path).expect("should load");
        assert_eq!(config.assets.len(), 2);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_config_from_missing_file() {
        let path = PathBuf::from("/tmp/ticker-config-does-not-exist-xyz.toml");
        assert!(load_config_from(&path).is_err());
    }

    #[test]
    fn bundled_config_path_returns_some_path() {
        let path = bundled_config_path().expect("should resolve");
        assert!(path.to_string_lossy().contains("config.toml"));
    }

    #[test]
    fn menubar_pin_roundtrip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ticker-menubar-pin-{stamp}"));
        unsafe {
            std::env::set_var("TICKER_MENUBAR_PIN_PATH", &path);
        }
        save_menubar_pin("Gold").expect("save");
        assert_eq!(load_menubar_pin().as_deref(), Some("Gold"));
        let _ = fs::remove_file(&path);
        unsafe {
            std::env::remove_var("TICKER_MENUBAR_PIN_PATH");
        }
    }

    #[test]
    fn apply_asset_edit_replaces_url_and_unit() {
        let mut asset = gecko_btc();
        apply_asset_edit(
            &mut asset,
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd",
            "usd",
            "bitcoin.usd",
        )
        .unwrap();
        assert!(asset.url.contains("vs_currencies=usd"));
        assert_eq!(asset.unit, "USD");
        assert_eq!(asset.price_path, "bitcoin.usd");
    }

    #[test]
    fn apply_asset_edit_rejects_empty_url() {
        let mut asset = gecko_btc();
        assert!(apply_asset_edit(&mut asset, "  ", "USD", "bitcoin.usd").is_err());
    }

    #[test]
    fn apply_asset_edit_requires_https_and_keeps_asset_on_error() {
        let mut asset = gecko_btc();
        let before = asset.url.clone();
        let err = apply_asset_edit(&mut asset, "http://example.com/p", "USD", "p").unwrap_err();
        assert!(err.contains("https"), "{err}");
        assert_eq!(asset.url, before);
    }

    #[test]
    fn validate_asset_url_cases() {
        for ok in [
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=eur",
            "HTTPS://example.com/x",
            "https://example.com:8443/p",
            "https://bücher.example/preis",
        ] {
            assert!(validate_asset_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "http://example.com/p",
            "ftp://example.com/p",
            "file:///etc/passwd",
            "example.com/p",
            "https://",
            "https://user:pw@example.com/p",
            "https://exa mple.com/p",
            "javascript:alert(1)",
        ] {
            assert!(validate_asset_url(bad).is_err(), "{bad}");
        }
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        assert!(validate_asset_url(&long).is_err());
    }

    #[test]
    fn old_power_path_is_migrated_only_on_the_power_host() {
        let toml = "[[assets]]\nname = \"Power NL\"\nurl = \"https://dap.xadi.eu/api/nl/today\"\nprice_path = \"data.0.price\"\nunit = \"EUR\"\nunit_hint = \"/kWh\"\nsymbol = \"P\"\n\n[[assets]]\nname = \"Other\"\nurl = \"https://example.com/x\"\nprice_path = \"data.0.price\"\nunit = \"EUR\"\nunit_hint = \"\"\nsymbol = \"O\"\n";
        let mut config = parse_config(toml).unwrap();
        migrate_power_path(&mut config);
        assert_eq!(config.assets[0].price_path, "data.@now.price");
        assert_eq!(config.assets[1].price_path, "data.0.price");
        assert!(config.assets[0].allows_negative());
        assert_eq!(config.assets[0].max_jump(), POWER_MAX_JUMP_PCT);
        assert!(!config.assets[1].allows_negative());
        assert_eq!(config.assets[1].max_jump(), DEFAULT_MAX_JUMP_PCT);
    }

    #[test]
    fn migration_keeps_explicit_power_settings() {
        let toml = "[[assets]]\nname = \"Power NL\"\nurl = \"https://dap.xadi.eu/api/nl/today\"\nprice_path = \"data.@now.price\"\nunit = \"EUR\"\nunit_hint = \"/kWh\"\nsymbol = \"P\"\nallow_negative = false\nmax_jump_pct = 50.0\n";
        let mut config = parse_config(toml).unwrap();
        migrate_power_path(&mut config);
        assert!(!config.assets[0].allows_negative());
        assert_eq!(config.assets[0].max_jump(), 50.0);
    }

    #[test]
    fn plausibility_settings_default_and_roundtrip() {
        let config = parse_config(sample_toml()).unwrap();
        assert!(!config.assets[0].allows_negative());
        assert_eq!(config.assets[0].max_jump(), DEFAULT_MAX_JUMP_PCT);
        let body = toml::to_string_pretty(&config).unwrap();
        assert!(
            !body.contains("allow_negative"),
            "unset fields stay out of the file"
        );
        let mut bad = gecko_btc();
        for pct in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            bad.max_jump_pct = Some(pct);
            assert_eq!(bad.max_jump(), DEFAULT_MAX_JUMP_PCT);
        }
    }

    #[test]
    fn bundled_power_asset_allows_negative_prices() {
        let config = parse_config(include_str!("../config.toml")).unwrap();
        for asset in &config.assets {
            let power = asset.name == "Power NL";
            assert_eq!(asset.allows_negative(), power, "{}", asset.name);
            let jump = if power {
                POWER_MAX_JUMP_PCT
            } else {
                DEFAULT_MAX_JUMP_PCT
            };
            assert_eq!(asset.max_jump(), jump, "{}", asset.name);
        }
    }

    #[test]
    fn invalid_assets_are_skipped_not_fatal() {
        let toml = sample_toml()
            .replace("https://example.com/gold", "http://example.com/gold")
            .replace("bitcoin.eur", " ");
        let config = parse_config(&toml).expect("still a valid config");
        assert_eq!(config.assets.len(), 2, "kept for Edit asset");
        assert!(config.fetchable_assets().is_empty());
        let skipped = config.skipped_assets();
        assert_eq!(
            skipped,
            vec![
                SkippedAsset {
                    name: "Bitcoin".into(),
                    reason: "Price path cannot be empty".into()
                },
                SkippedAsset {
                    name: "Gold".into(),
                    reason: "URL must start with https://".into()
                },
            ]
        );
        let good = parse_config(sample_toml()).unwrap();
        assert_eq!(good.fetchable_assets().len(), 2);
        assert!(good.skipped_assets().is_empty());
    }

    #[test]
    fn bundled_config_urls_pass_validation() {
        let config = parse_config(include_str!("../config.toml")).unwrap();
        for asset in &config.assets {
            assert!(validate_asset_url(&asset.url).is_ok(), "{}", asset.url);
        }
        assert!(config.skipped_assets().is_empty());
    }

    #[test]
    fn user_config_roundtrip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ticker-user-config-{stamp}.toml"));
        unsafe {
            std::env::set_var("TICKER_USER_CONFIG_PATH", &path);
        }
        let mut config = parse_config(sample_toml()).unwrap();
        apply_asset_edit(
            &mut config.assets[0],
            "https://example.com/btc-usd",
            "USD",
            "bitcoin.usd",
        )
        .unwrap();
        save_user_config(&config).unwrap();
        let loaded = load_config_from(&path).unwrap();
        assert_eq!(loaded.assets[0].unit, "USD");
        assert_eq!(loaded.assets[0].url, "https://example.com/btc-usd");
        reset_user_config().unwrap();
        assert!(!path.exists());
        unsafe {
            std::env::remove_var("TICKER_USER_CONFIG_PATH");
        }
    }
}
