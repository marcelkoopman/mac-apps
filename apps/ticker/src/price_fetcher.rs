use crate::config::Asset;
use chrono::{DateTime, TimeDelta, Utc};
use polars::prelude::*;
use reqwest::blocking::Client;
use serde_json::Value;
use std::error::Error;
use std::io::Read;
use std::thread;
use std::time::Duration;

const MAX_FETCH_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);
/// Whole request (connect + headers + body). Bounds one attempt, so `fetch_price` on one asset
/// takes at most 3 × 10 s + 2 × 0.5 s = 31 s, and a poll (assets fetched one after another, on the
/// fetch thread) at most `assets.len()` times that.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Price APIs answer with a few KB; anything above this is refused instead of buffered.
const MAX_BODY_BYTES: u64 = 1024 * 1024;
/// Path part that picks, from an array of `{"time": <RFC 3339>, …}` entries, the one whose period
/// contains the current time (see [`select_now`]).
const NOW_SELECTOR: &str = "@now";
/// Moves smaller than this (in percent of the earlier price) count as "flat".
const FLAT_PCT: f64 = 0.05;
/// Period of the last entry when an `@now` array has a single entry (no step to derive it from).
const DEFAULT_PERIOD: TimeDelta = TimeDelta::hours(1);

/// Cheap to clone (the reqwest client is reference-counted), so a clone can move into the fetch
/// thread.
#[derive(Clone)]
pub struct PriceFetcher {
    client: Client,
}

impl PriceFetcher {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .user_agent("rust-price-fetcher/1.0")
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()?;
        Ok(PriceFetcher { client })
    }

    /// Up to [`MAX_FETCH_ATTEMPTS`] tries; NaN when all fail. Failures go to the debug log.
    pub fn fetch_price(&self, asset: &Asset) -> f64 {
        for attempt in 1..=MAX_FETCH_ATTEMPTS {
            match self.fetch_price_once(asset, attempt) {
                Ok(price) => return price,
                Err(e) => {
                    crate::log_message(&format!(
                        "fetch: {} attempt {attempt}/{MAX_FETCH_ATTEMPTS} failed: {e}",
                        asset.name
                    ));
                }
            }
            if attempt < MAX_FETCH_ATTEMPTS {
                thread::sleep(RETRY_DELAY);
            }
        }

        crate::log_message(&format!(
            "fetch: giving up on {} after {MAX_FETCH_ATTEMPTS} attempts ({})",
            asset.name, asset.url
        ));
        f64::NAN
    }

    fn fetch_price_once(&self, asset: &Asset, attempt: u32) -> Result<f64, String> {
        eprintln!(
            "🔍 Fetching {} from {} (attempt {}/{})",
            asset.name, asset.url, attempt, MAX_FETCH_ATTEMPTS
        );
        let response = self
            .client
            .get(&asset.url)
            .send()
            .map_err(|e| format!("network error: {e}"))?;
        let response = response
            .error_for_status()
            .map_err(|e| format!("HTTP error: {e}"))?;
        let json =
            read_json_capped(response, MAX_BODY_BYTES).map_err(|e| format!("bad response: {e}"))?;
        let value = self
            .get_value_by_path(&json, &asset.price_path)
            .ok_or_else(|| format!("path {:?} not found in the JSON", asset.price_path))?;
        let price = match &value {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        };
        price
            .filter(|p: &f64| p.is_finite())
            .ok_or_else(|| format!("value at {:?} is not a number: {value}", asset.price_path))
    }

    pub fn fetch_all(&self, assets: &[Asset]) -> Result<DataFrame, Box<dyn Error>> {
        let mut symbols: Vec<String> = Vec::with_capacity(assets.len());
        let mut names: Vec<String> = Vec::with_capacity(assets.len());
        let mut prices: Vec<f64> = Vec::with_capacity(assets.len());
        let mut units: Vec<String> = Vec::with_capacity(assets.len());
        let mut unit_hints: Vec<String> = Vec::with_capacity(assets.len());

        for asset in assets {
            let price = self.fetch_price(asset);
            symbols.push(asset.symbol.clone());
            names.push(asset.name.clone());
            prices.push(price);
            units.push(asset.unit.clone());
            unit_hints.push(asset.unit_hint.clone());
        }

        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), symbols).into(),
            Series::new("name".into(), names).into(),
            Series::new("price".into(), prices).into(),
            Series::new("unit".into(), units).into(),
            Series::new("unit_hint".into(), unit_hints).into(),
        ])
        .map_err(|e| e.into())
    }

    pub fn build_initial_dataframe(
        &self,
        assets: &[Asset],
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let base = self.fetch_all(assets)?;
        Self::attach_change_columns(base, None, day_opens)
    }

    pub fn update_dataframe(
        &self,
        previous: &DataFrame,
        assets: &[Asset],
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let fresh = self.fetch_all(assets)?;

        let prev_names = previous.column("name")?.str()?;
        let prev_prices = previous.column("price")?.f64()?;

        let mut prev_by_name: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for i in 0..previous.height() {
            if let (Some(name), Some(price)) = (prev_names.get(i), prev_prices.get(i)) {
                prev_by_name.insert(name.to_string(), price);
            }
        }

        Self::attach_change_columns(fresh, Some(&prev_by_name), day_opens)
    }

    fn attach_change_columns(
        mut df: DataFrame,
        prev_by_name: Option<&std::collections::HashMap<String, f64>>,
        day_opens: &std::collections::HashMap<String, f64>,
    ) -> Result<DataFrame, Box<dyn Error>> {
        let n = df.height();
        let names = df.column("name")?.str()?;
        let prices = df.column("price")?.f64()?;

        let mut prev_price_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut change_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut pct_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut direction_col: Vec<String> = Vec::with_capacity(n);

        let mut day_open_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut change_day_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut pct_day_col: Vec<Option<f64>> = Vec::with_capacity(n);
        let mut direction_day_col: Vec<String> = Vec::with_capacity(n);

        for i in 0..n {
            let name = names.get(i).unwrap_or("");
            let price = prices.get(i).unwrap_or(f64::NAN);

            let prev = prev_by_name.and_then(|m| m.get(name).copied());
            prev_price_col.push(prev);
            if let Some(p) = prev {
                if price.is_nan() || p.is_nan() || p == 0.0 {
                    change_col.push(None);
                    pct_col.push(None);
                    direction_col.push(String::new());
                } else {
                    let change = price - p;
                    let pct = (change / p.abs()) * 100.0;
                    change_col.push(Some(change));
                    pct_col.push(Some(pct));
                    direction_col.push(Self::direction_label(pct));
                }
            } else {
                change_col.push(None);
                pct_col.push(None);
                direction_col.push(String::new());
            }

            let open = day_opens
                .get(name)
                .copied()
                .filter(|o| !o.is_nan() && *o != 0.0)
                .or_else(|| {
                    if !price.is_nan() && price != 0.0 {
                        Some(price)
                    } else {
                        None
                    }
                });

            day_open_col.push(open);
            if let Some(o) = open {
                if price.is_nan() {
                    change_day_col.push(None);
                    pct_day_col.push(None);
                    direction_day_col.push(String::new());
                } else {
                    let change = price - o;
                    let pct = (change / o.abs()) * 100.0;
                    change_day_col.push(Some(change));
                    pct_day_col.push(Some(pct));
                    direction_day_col.push(Self::direction_label(pct));
                }
            } else {
                change_day_col.push(None);
                pct_day_col.push(None);
                direction_day_col.push(String::new());
            }
        }

        df.with_column(Series::new("prev_price".into(), prev_price_col).into())?;
        df.with_column(Series::new("change".into(), change_col).into())?;
        df.with_column(Series::new("pct_change".into(), pct_col).into())?;
        df.with_column(Series::new("direction".into(), direction_col).into())?;
        df.with_column(Series::new("day_open".into(), day_open_col).into())?;
        df.with_column(Series::new("change_day".into(), change_day_col).into())?;
        df.with_column(Series::new("pct_day".into(), pct_day_col).into())?;
        df.with_column(Series::new("direction_day".into(), direction_day_col).into())?;

        eprintln!("📊 DataFrame:\n{df}");
        Ok(df)
    }

    /// "up" / "down" for a move of at least [`FLAT_PCT`] percent either way, else "flat".
    /// Relative, so a cent on €0,20/kWh power counts and a cent on €66.000 bitcoin does not.
    fn direction_label(pct: f64) -> String {
        if pct >= FLAT_PCT {
            "up".to_string()
        } else if pct <= -FLAT_PCT {
            "down".to_string()
        } else {
            "flat".to_string()
        }
    }

    fn get_value_by_path(&self, value: &Value, path: &str) -> Option<Value> {
        value_at_path(value, path, Utc::now())
    }
}

/// `path` resolved in `value` at time `now` (only `@now` parts depend on it). Parts are split on
/// `.`: an object key, an array index, `field=value` (the array element whose string `field`
/// equals `value`) or `@now` (see [`select_now`]).
fn value_at_path(value: &Value, path: &str, now: DateTime<Utc>) -> Option<Value> {
    let mut current = value.clone();

    for part in path.split('.') {
        if part == NOW_SELECTOR {
            let Value::Array(arr) = &current else {
                return None;
            };
            current = select_now(arr, now)?.clone();
        } else if part.contains('=') {
            let (field_name, filter_value) = part.split_once('=')?;

            if let Value::Array(arr) = &current {
                current = arr
                    .iter()
                    .find(|item| {
                        if let Value::Object(map) = item
                            && let Some(field) = map.get(field_name)
                        {
                            return field.as_str().map(|s| s == filter_value).unwrap_or(false);
                        }
                        false
                    })?
                    .clone();
            } else {
                return None;
            }
        } else {
            current = match &current {
                Value::Object(map) => map.get(part)?.clone(),
                Value::Array(arr) => {
                    let index: usize = part.parse().ok()?;
                    arr.get(index)?.clone()
                }
                _ => return None,
            };
        }
    }

    Some(current)
}

/// The entry of `arr` (objects with an RFC 3339 `time`, e.g. `2026-10-02T22:00:00.000Z`) whose
/// period contains `now`: its `time` ≤ `now` < the next entry's `time`. The last entry lasts as
/// long as the step before it (one hour when it is the only entry), so hourly and quarter-hour
/// data both work. Times are compared as instants, so a DST day (23 or 25 hourly entries) needs
/// nothing special. `None` before the first entry, after the last period, or without usable times.
fn select_now(arr: &[Value], now: DateTime<Utc>) -> Option<&Value> {
    let mut timed: Vec<(DateTime<Utc>, &Value)> = arr
        .iter()
        .filter_map(|item| {
            let time = DateTime::parse_from_rfc3339(item.get("time")?.as_str()?).ok()?;
            Some((time.with_timezone(&Utc), item))
        })
        .collect();
    timed.sort_by_key(|(time, _)| *time);
    let i = timed.iter().rposition(|(time, _)| *time <= now)?;
    let end = match timed.get(i + 1) {
        Some((next, _)) => *next,
        None => {
            let step = match i.checked_sub(1) {
                Some(prev) => timed[i].0 - timed[prev].0,
                None => DEFAULT_PERIOD,
            };
            timed[i].0 + step
        }
    };
    (now < end).then_some(timed[i].1)
}

/// Response body as JSON, refusing bodies over `cap` bytes (by `Content-Length` up front, and
/// while reading for chunked or lying responses).
fn read_json_capped(resp: reqwest::blocking::Response, cap: u64) -> Result<Value, String> {
    if let Some(len) = resp.content_length()
        && len > cap
    {
        return Err(format!("response too large ({len} bytes, limit {cap})"));
    }
    let body = read_capped(resp, cap)?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

fn read_capped(reader: impl Read, cap: u64) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > cap {
        return Err(format!("response too large (over {cap} bytes)"));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn fetcher() -> PriceFetcher {
        PriceFetcher::new().expect("client should build")
    }

    #[test]
    fn max_fetch_attempts_is_three() {
        assert_eq!(MAX_FETCH_ATTEMPTS, 3);
    }

    #[test]
    fn read_capped_accepts_up_to_cap_and_refuses_more() {
        assert_eq!(read_capped(&b"12345"[..], 5).unwrap(), b"12345");
        assert!(read_capped(&b"123456"[..], 5).is_err());
        assert!(read_capped(&b""[..], 5).unwrap().is_empty());
        let big = vec![b' '; (MAX_BODY_BYTES + 1) as usize];
        assert!(read_capped(&big[..], MAX_BODY_BYTES).is_err());
    }

    #[test]
    fn worst_case_fetch_time_is_bounded() {
        assert!(CONNECT_TIMEOUT <= REQUEST_TIMEOUT);
        let per_asset =
            REQUEST_TIMEOUT * MAX_FETCH_ATTEMPTS + RETRY_DELAY * (MAX_FETCH_ATTEMPTS - 1);
        assert_eq!(per_asset, Duration::from_secs(31));
        // Default config (7 assets) finishes within one poll interval (5 min) even if all time out.
        assert!(per_asset * 7 < Duration::from_secs(5 * 60));
    }

    #[test]
    fn empty_assets_dataframe() {
        let f = fetcher();
        let df = f.fetch_all(&[]).expect("empty df");
        assert_eq!(df.height(), 0);
        assert_eq!(df.width(), 5);
    }

    #[test]
    fn simple_object_path() {
        let f = fetcher();
        let data = json!({"price": 42000.5});
        let v = f.get_value_by_path(&data, "price").unwrap();
        assert_eq!(v.as_f64(), Some(42000.5));
    }

    #[test]
    fn nested_object_path() {
        let f = fetcher();
        let data = json!({
            "data": {
                "quote": {
                    "EUR": {
                        "price": 91.23
                    }
                }
            }
        });
        let v = f.get_value_by_path(&data, "data.quote.EUR.price").unwrap();
        assert_eq!(v.as_f64(), Some(91.23));
    }

    #[test]
    fn array_index_path() {
        let f = fetcher();
        let data = json!([{"price": 10.0}, {"price": 20.0}]);
        let v = f.get_value_by_path(&data, "1.price").unwrap();
        assert_eq!(v.as_f64(), Some(20.0));
    }

    #[test]
    fn array_filter_by_field() {
        let f = fetcher();
        let data = json!({
            "items": [
                {"symbol": "BTC", "price": 50000.0},
                {"symbol": "ETH", "price": 3000.0}
            ]
        });
        let v = f
            .get_value_by_path(&data, "items.symbol=ETH.price")
            .unwrap();
        assert_eq!(v.as_f64(), Some(3000.0));
    }

    #[test]
    fn missing_key_returns_none() {
        let f = fetcher();
        let data = json!({"price": 1.0});
        assert!(f.get_value_by_path(&data, "missing").is_none());
    }

    #[test]
    fn direction_label_thresholds() {
        assert_eq!(PriceFetcher::direction_label(0.0), "flat");
        assert_eq!(PriceFetcher::direction_label(0.049), "flat");
        assert_eq!(PriceFetcher::direction_label(-0.049), "flat");
        assert_eq!(PriceFetcher::direction_label(0.05), "up");
        assert_eq!(PriceFetcher::direction_label(-0.05), "down");
    }

    #[test]
    fn small_prices_move_and_big_prices_stay_flat_on_the_same_cent() {
        // One cent on 0,20 €/kWh is 5%: a move.
        let df = base_df("Power NL", 0.21);
        let opens = HashMap::from([("Power NL".to_string(), 0.20)]);
        let df = PriceFetcher::attach_change_columns(df, None, &opens).unwrap();
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("up")
        );
        // Ten euro on 66.000 € bitcoin is 0,015%: flat.
        let df = base_df("Bitcoin", 66010.0);
        let opens = HashMap::from([("Bitcoin".to_string(), 66000.0)]);
        let df = PriceFetcher::attach_change_columns(df, None, &opens).unwrap();
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("flat")
        );
    }

    fn base_df(name: &str, price: f64) -> DataFrame {
        DataFrame::new_infer_height(vec![
            Series::new("symbol".into(), vec!["X".to_string()]).into(),
            Series::new("name".into(), vec![name.to_string()]).into(),
            Series::new("price".into(), vec![price]).into(),
            Series::new("unit".into(), vec!["EUR".to_string()]).into(),
            Series::new("unit_hint".into(), vec!["/u".to_string()]).into(),
        ])
        .expect("df")
    }

    #[test]
    fn attach_change_columns_poll_and_day_up() {
        let df = base_df("Bitcoin", 110.0);
        let prev = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        let opens = HashMap::from([("Bitcoin".to_string(), 100.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        let change = df.column("change").unwrap().f64().unwrap().get(0);
        let pct = df.column("pct_change").unwrap().f64().unwrap().get(0);
        let dir = df.column("direction").unwrap().str().unwrap().get(0);
        let day_dir = df.column("direction_day").unwrap().str().unwrap().get(0);
        assert_eq!(change, Some(10.0));
        assert!((pct.unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(dir, Some("up"));
        assert_eq!(day_dir, Some("up"));
    }

    #[test]
    fn attach_change_columns_day_down_poll_flat() {
        let df = base_df("Gold", 100.005);
        let prev = HashMap::from([("Gold".to_string(), 100.0)]);
        let opens = HashMap::from([("Gold".to_string(), 110.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        assert_eq!(
            df.column("direction").unwrap().str().unwrap().get(0),
            Some("flat")
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("down")
        );
    }

    #[test]
    fn attach_change_columns_nan_price_has_no_change() {
        let df = base_df("Gas", f64::NAN);
        let prev = HashMap::from([("Gas".to_string(), 40.0)]);
        let opens = HashMap::from([("Gas".to_string(), 40.0)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &opens).unwrap();

        assert!(df.column("change").unwrap().f64().unwrap().get(0).is_none());
        assert_eq!(
            df.column("direction").unwrap().str().unwrap().get(0),
            Some("")
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("")
        );
    }

    #[test]
    fn attach_change_columns_missing_open_uses_current_price() {
        let df = base_df("Power", 50.0);
        let df = PriceFetcher::attach_change_columns(df, None, &HashMap::new()).unwrap();
        assert_eq!(
            df.column("day_open").unwrap().f64().unwrap().get(0),
            Some(50.0)
        );
        assert_eq!(
            df.column("direction_day").unwrap().str().unwrap().get(0),
            Some("flat")
        );
        assert!(df.column("change").unwrap().f64().unwrap().get(0).is_none());
    }

    #[test]
    fn get_value_by_path_string_number() {
        let f = fetcher();
        let data = json!({"price": "42.5"});
        let v = f.get_value_by_path(&data, "price").unwrap();
        assert_eq!(v.as_str(), Some("42.5"));
    }

    /// `dap.xadi.eu/api/nl/today` shape: `data` entries with a UTC `time`, `localTime`, `price`.
    fn day_ahead(start: &str, step_minutes: i64, prices: &[f64]) -> Value {
        let start = DateTime::parse_from_rfc3339(start)
            .unwrap()
            .with_timezone(&Utc);
        let data: Vec<Value> = prices
            .iter()
            .enumerate()
            .map(|(i, price)| {
                let time = start + TimeDelta::minutes(step_minutes * i as i64);
                json!({
                    "time": time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                    "localTime": "",
                    "price": price,
                })
            })
            .collect();
        json!({"status": "success", "data": data})
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn price_now(data: &Value, now: &str) -> Option<f64> {
        value_at_path(data, "data.@now.price", at(now))?.as_f64()
    }

    #[test]
    fn now_selector_on_real_response_shape() {
        let data = json!({"status": "success", "data": [
            {"time": "2026-10-02T22:00:00.000Z", "priceMwh": 191.4, "price": 0.19141,
             "localTime": "00:00", "hour": "00:00"},
            {"time": "2026-10-02T23:00:00.000Z", "priceMwh": 177.4, "price": 0.17747,
             "localTime": "01:00", "hour": "01:00"},
            {"time": "2026-10-03T00:00:00.000Z", "priceMwh": 173.5, "price": 0.17351,
             "localTime": "02:00", "hour": "02:00"},
        ]});
        // 01:30 local (UTC+2) is 23:30Z: the 01:00 entry, not data.0 (midnight).
        assert_eq!(price_now(&data, "2026-10-02T23:30:00Z"), Some(0.17747));
        assert_eq!(price_now(&data, "2026-10-02T22:00:00Z"), Some(0.19141));
        assert_eq!(price_now(&data, "2026-10-02T22:59:59Z"), Some(0.19141));
        // The last entry covers one step (1 h) past its time, then nothing.
        assert_eq!(price_now(&data, "2026-10-03T00:59:59Z"), Some(0.17351));
        assert_eq!(price_now(&data, "2026-10-03T01:00:00Z"), None);
        assert_eq!(price_now(&data, "2026-10-02T21:59:59Z"), None);
    }

    #[test]
    fn now_selector_hourly_day_picks_current_hour() {
        let prices: Vec<f64> = (0..24).map(|h| h as f64 / 100.0).collect();
        // 2026-10-03 in Amsterdam (UTC+2) starts at 2026-10-02T22:00Z.
        let data = day_ahead("2026-10-02T22:00:00Z", 60, &prices);
        // 18:05 local = 16:05Z → hour index 18.
        assert_eq!(price_now(&data, "2026-10-03T16:05:00Z"), Some(0.18));
        assert_eq!(price_now(&data, "2026-10-03T21:59:00Z"), Some(0.23));
        assert_eq!(price_now(&data, "2026-10-03T22:00:00Z"), None);
    }

    #[test]
    fn now_selector_dst_fall_back_day_has_25_hours() {
        // 2026-10-25: Amsterdam goes from UTC+2 to UTC+1 at 03:00 local; 02:00–03:00 local
        // happens twice. Day starts 2026-10-24T22:00Z and has 25 hourly entries.
        let prices: Vec<f64> = (0..25).map(|h| h as f64).collect();
        let data = day_ahead("2026-10-24T22:00:00Z", 60, &prices);
        // First 02:30 local (UTC+2) = 00:30Z → index 2; second 02:30 (UTC+1) = 01:30Z → index 3.
        assert_eq!(price_now(&data, "2026-10-25T00:30:00Z"), Some(2.0));
        assert_eq!(price_now(&data, "2026-10-25T01:30:00Z"), Some(3.0));
        // 23:30 local (UTC+1) = 22:30Z → last entry (index 24).
        assert_eq!(price_now(&data, "2026-10-25T22:30:00Z"), Some(24.0));
        assert_eq!(price_now(&data, "2026-10-25T23:00:00Z"), None);
    }

    #[test]
    fn now_selector_dst_spring_forward_day_has_23_hours() {
        // 2026-03-29: UTC+1 → UTC+2 at 02:00 local. Starts 2026-03-28T23:00Z, 23 entries.
        let prices: Vec<f64> = (0..23).map(|h| h as f64).collect();
        let data = day_ahead("2026-03-28T23:00:00Z", 60, &prices);
        // 03:30 local (UTC+2) = 01:30Z → index 2 (00:00, 01:00, then 03:00 local).
        assert_eq!(price_now(&data, "2026-03-29T01:30:00Z"), Some(2.0));
        assert_eq!(price_now(&data, "2026-03-29T21:59:00Z"), Some(22.0));
        assert_eq!(price_now(&data, "2026-03-29T22:00:00Z"), None);
    }

    #[test]
    fn now_selector_quarter_hours_and_negative_prices() {
        let prices: Vec<f64> = (0..96).map(|q| -0.05 + q as f64 / 1000.0).collect();
        let data = day_ahead("2026-10-02T22:00:00Z", 15, &prices);
        // 12:20 local = 10:20Z → quarter 12*4+1 = 49.
        let p = price_now(&data, "2026-10-03T10:20:00Z").unwrap();
        assert!((p - (-0.05 + 0.049)).abs() < 1e-12, "{p}");
        assert_eq!(price_now(&data, "2026-10-02T22:14:59Z"), Some(-0.05));
        // The last quarter covers 15 minutes, not an hour.
        assert!(price_now(&data, "2026-10-03T21:59:00Z").is_some());
        assert_eq!(price_now(&data, "2026-10-03T22:00:00Z"), None);
    }

    #[test]
    fn now_selector_handles_unsorted_single_and_bad_entries() {
        let data = json!({"data": [
            {"time": "2026-10-03T01:00:00Z", "price": 2.0},
            {"time": "not a time", "price": 99.0},
            {"price": 98.0},
            {"time": "2026-10-03T00:00:00Z", "price": 1.0},
        ]});
        assert_eq!(price_now(&data, "2026-10-03T00:30:00Z"), Some(1.0));
        assert_eq!(price_now(&data, "2026-10-03T01:30:00Z"), Some(2.0));
        let single = json!({"data": [{"time": "2026-10-03T00:00:00Z", "price": 1.0}]});
        assert_eq!(price_now(&single, "2026-10-03T00:59:00Z"), Some(1.0));
        assert_eq!(price_now(&single, "2026-10-03T01:00:00Z"), None);
        // `@now` on something that is not an array.
        assert_eq!(
            price_now(&json!({"data": {"price": 1.0}}), "2026-10-03T00:00:00Z"),
            None
        );
        assert_eq!(
            price_now(&json!({"data": []}), "2026-10-03T00:00:00Z"),
            None
        );
    }

    #[test]
    fn bundled_power_asset_uses_now_selector() {
        let config = crate::config::parse_config(include_str!("../config.toml")).unwrap();
        let power = config.assets.iter().find(|a| a.name == "Power NL").unwrap();
        assert_eq!(power.price_path, "data.@now.price");
    }

    #[test]
    fn pct_change_of_negative_price_keeps_the_sign_of_the_move() {
        let df = base_df("Power NL", 0.05);
        let prev = HashMap::from([("Power NL".to_string(), -0.05)]);
        let df = PriceFetcher::attach_change_columns(df, Some(&prev), &HashMap::new()).unwrap();
        let pct = df
            .column("pct_change")
            .unwrap()
            .f64()
            .unwrap()
            .get(0)
            .unwrap();
        assert!((pct - 200.0).abs() < 1e-9, "{pct}");
        assert_eq!(
            df.column("direction").unwrap().str().unwrap().get(0),
            Some("up")
        );
    }
}
