use crate::config::Asset;
use crate::prices::{PriceRow, attach_changes};
use chrono::{DateTime, TimeDelta, Utc};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::io::Read;
use std::thread;
use std::time::Duration;

const MAX_FETCH_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(500);
/// Whole request (connect + headers + body). Bounds one attempt, so fetching one URL takes at
/// most 3 × 10 s + 2 × 0.5 s = 31 s, and a poll (URLs fetched one after another, on the fetch
/// thread) at most the number of distinct URLs times that.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Price APIs answer with a few KB; anything above this is refused instead of buffered.
const MAX_BODY_BYTES: u64 = 1024 * 1024;
/// Redirects followed per request; each must stay on https.
const MAX_REDIRECTS: usize = 3;
/// Path part that picks, from an array of `{"time": <RFC 3339>, …}` entries, the one whose period
/// contains the current time (see [`select_now`]).
const NOW_SELECTOR: &str = "@now";
/// Period of the last entry when an `@now` array has a single entry (no step to derive it from).
const DEFAULT_PERIOD: TimeDelta = TimeDelta::hours(1);

/// Cheap to clone (the reqwest client is reference-counted), so a clone can move into the fetch
/// thread.
#[derive(Clone)]
pub struct PriceFetcher {
    client: Client,
}

/// Follows at most [`MAX_REDIRECTS`] redirects, and only to https URLs: config URLs are
/// https-only (checked at load), and a redirect must not downgrade that.
fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            attempt.error(format!("more than {MAX_REDIRECTS} redirects"))
        } else if attempt.url().scheme() != "https" {
            let msg = format!("redirect to a non-https URL ({})", attempt.url());
            attempt.error(msg)
        } else {
            attempt.follow()
        }
    })
}

impl PriceFetcher {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .user_agent("rust-price-fetcher/1.0")
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(redirect_policy())
            .build()?;
        Ok(PriceFetcher { client })
    }

    /// The JSON at `url`, with up to [`MAX_FETCH_ATTEMPTS`] tries. `label` (the asset names that
    /// use this URL) is only for the debug log, where every failure goes.
    pub fn fetch_json(&self, url: &str, label: &str) -> Result<Value, String> {
        let mut last_error = String::new();
        for attempt in 1..=MAX_FETCH_ATTEMPTS {
            eprintln!("🔍 Fetching {label} from {url} (attempt {attempt}/{MAX_FETCH_ATTEMPTS})");
            match self.fetch_json_once(url) {
                Ok(json) => return Ok(json),
                Err(e) => {
                    crate::log_message(&format!(
                        "fetch: {label} attempt {attempt}/{MAX_FETCH_ATTEMPTS} failed: {e}"
                    ));
                    last_error = e;
                }
            }
            if attempt < MAX_FETCH_ATTEMPTS {
                thread::sleep(RETRY_DELAY);
            }
        }
        crate::log_message(&format!(
            "fetch: giving up on {label} after {MAX_FETCH_ATTEMPTS} attempts ({url})"
        ));
        Err(last_error)
    }

    fn fetch_json_once(&self, url: &str) -> Result<Value, String> {
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|e| format!("network error: {e}"))?;
        let response = response
            .error_for_status()
            .map_err(|e| format!("HTTP error: {e}"))?;
        read_json_capped(response, MAX_BODY_BYTES).map_err(|e| format!("bad response: {e}"))
    }

    /// One row per asset, in config order; NaN for an asset whose fetch failed. Each distinct URL
    /// is fetched once per call (petrol and diesel share one), and a bad path or value in the
    /// answer is not retried: it would not change on a second request.
    pub fn fetch_all(&self, assets: &[Asset]) -> Vec<PriceRow> {
        let mut answers: HashMap<&str, Result<Value, String>> = HashMap::new();
        assets
            .iter()
            .map(|asset| {
                let json = answers.entry(asset.url.as_str()).or_insert_with(|| {
                    self.fetch_json(&asset.url, &names_using(assets, &asset.url))
                });
                let price = match json {
                    Ok(json) => price_in(json, &asset.price_path, Utc::now()).unwrap_or_else(|e| {
                        crate::log_message(&format!("fetch: {}: {e}", asset.name));
                        f64::NAN
                    }),
                    Err(_) => f64::NAN,
                };
                PriceRow::new(asset, price)
            })
            .collect()
    }

    /// A poll: fetch every asset and add the change since `previous` (the last poll's rows, if
    /// any) and since the day's first price (`day_opens`).
    pub fn poll(
        &self,
        assets: &[Asset],
        previous: Option<&[PriceRow]>,
        day_opens: &HashMap<String, f64>,
    ) -> Vec<PriceRow> {
        let mut rows = self.fetch_all(assets);
        let prev = previous.map(crate::prices::prices_by_name);
        attach_changes(&mut rows, prev.as_ref(), day_opens);
        rows
    }
}

/// Names of the assets fetched from `url`, for log lines ("Petrol Euro95 + Diesel").
fn names_using(assets: &[Asset], url: &str) -> String {
    assets
        .iter()
        .filter(|a| a.url == url)
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" + ")
}

/// The finite number at `path` in `json` (a JSON number or a numeric string).
fn price_in(json: &Value, path: &str, now: DateTime<Utc>) -> Result<f64, String> {
    let value = value_at_path(json, path, now)
        .ok_or_else(|| format!("path {path:?} not found in the JSON"))?;
    let price = match &value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    price
        .filter(|p: &f64| p.is_finite())
        .ok_or_else(|| format!("value at {path:?} is not a number: {value}"))
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
    fn empty_assets_give_no_rows() {
        let f = fetcher();
        assert!(f.fetch_all(&[]).is_empty());
        assert!(f.poll(&[], None, &HashMap::new()).is_empty());
    }

    #[test]
    fn simple_object_path() {
        let data = json!({"price": 42000.5});
        let v = value_at_path(&data, "price", Utc::now()).unwrap();
        assert_eq!(v.as_f64(), Some(42000.5));
    }

    #[test]
    fn nested_object_path() {
        let data = json!({
            "data": {
                "quote": {
                    "EUR": {
                        "price": 91.23
                    }
                }
            }
        });
        let v = value_at_path(&data, "data.quote.EUR.price", Utc::now()).unwrap();
        assert_eq!(v.as_f64(), Some(91.23));
    }

    #[test]
    fn array_index_path() {
        let data = json!([{"price": 10.0}, {"price": 20.0}]);
        let v = value_at_path(&data, "1.price", Utc::now()).unwrap();
        assert_eq!(v.as_f64(), Some(20.0));
    }

    #[test]
    fn array_filter_by_field() {
        let data = json!({
            "items": [
                {"symbol": "BTC", "price": 50000.0},
                {"symbol": "ETH", "price": 3000.0}
            ]
        });
        let v = value_at_path(&data, "items.symbol=ETH.price", Utc::now()).unwrap();
        assert_eq!(v.as_f64(), Some(3000.0));
    }

    #[test]
    fn missing_key_returns_none() {
        let data = json!({"price": 1.0});
        assert!(value_at_path(&data, "missing", Utc::now()).is_none());
    }

    #[test]
    fn get_value_by_path_string_number() {
        let data = json!({"price": "42.5"});
        let v = value_at_path(&data, "price", Utc::now()).unwrap();
        assert_eq!(v.as_str(), Some("42.5"));
    }

    #[test]
    fn price_in_accepts_numbers_and_numeric_strings_only() {
        let now = Utc::now();
        let data = json!({"a": 1.5, "b": " -2.25 ", "c": "n/a", "d": null, "e": [1]});
        assert_eq!(price_in(&data, "a", now), Ok(1.5));
        assert_eq!(price_in(&data, "b", now), Ok(-2.25));
        for path in ["c", "d", "e"] {
            assert!(
                price_in(&data, path, now)
                    .unwrap_err()
                    .contains("not a number")
            );
        }
        assert!(
            price_in(&data, "zz", now)
                .unwrap_err()
                .contains("not found")
        );
        let inf = json!({"p": "inf"});
        assert!(price_in(&inf, "p", now).is_err());
    }

    #[test]
    fn names_using_joins_assets_sharing_a_url() {
        let config = crate::config::parse_config(include_str!("../config.toml")).unwrap();
        let fuel = config
            .assets
            .iter()
            .find(|a| a.name.to_lowercase().contains("diesel"))
            .expect("default config has diesel");
        let names = names_using(&config.assets, &fuel.url);
        assert!(
            names.contains(" + "),
            "petrol and diesel share a URL: {names}"
        );
        assert_eq!(names_using(&config.assets, "https://nowhere.invalid"), "");
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
}
