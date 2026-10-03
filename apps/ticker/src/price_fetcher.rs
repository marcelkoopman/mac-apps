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
/// Wait before the second attempt; doubled before each further one (exponential backoff).
const RETRY_DELAY: Duration = Duration::from_millis(500);
/// A `Retry-After` (on 429 or 5xx) up to this long is waited for; a longer one ends the retries
/// for this poll (the next poll tries again), so one slow API cannot hold up the others.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);
/// Whole request (connect + headers + body). Bounds one attempt, so fetching one URL takes at
/// most 3 × 10 s + 0.5 s + 1 s = 31.5 s without `Retry-After` (up to 2 × 10 s more with it),
/// and a poll (URLs fetched one after another, on the fetch thread) at most the number of
/// distinct URLs times that.
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
    /// [`RETRY_DELAY`] and [`MAX_RETRY_AFTER`]; shorter in the HTTP tests.
    retry_delay: Duration,
    max_retry_after: Duration,
}

/// Why one attempt failed, and whether another attempt makes sense.
#[derive(Debug)]
enum AttemptError {
    /// Network error, timeout, 429 or 5xx (with the server's `Retry-After`, if any), or a body
    /// that is not JSON.
    Retry {
        message: String,
        retry_after: Option<Duration>,
    },
    /// Another 4xx, a body over the size limit or a refused redirect: the same request would
    /// fail the same way.
    Final(String),
}

impl AttemptError {
    fn message(&self) -> &str {
        match self {
            AttemptError::Retry { message, .. } | AttemptError::Final(message) => message,
        }
    }
}

/// Wait before attempt `attempt + 1`: [`RETRY_DELAY`] doubled per earlier retry, or the
/// server's `Retry-After` when that is longer. `None` when `Retry-After` exceeds
/// `max_retry_after` (give up for this poll).
fn retry_wait(
    attempt: u32,
    base: Duration,
    retry_after: Option<Duration>,
    max_retry_after: Duration,
) -> Option<Duration> {
    let backoff = base * 2u32.saturating_pow(attempt.saturating_sub(1));
    match retry_after {
        Some(after) if after > max_retry_after => None,
        Some(after) => Some(after.max(backoff)),
        None => Some(backoff),
    }
}

/// `Retry-After` as seconds or as an HTTP date (time left until then; 0 when past).
fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let value = value.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    Some((at - now).to_std().unwrap_or(Duration::ZERO))
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
        Ok(PriceFetcher {
            client,
            retry_delay: RETRY_DELAY,
            max_retry_after: MAX_RETRY_AFTER,
        })
    }

    /// The JSON at `url`, with up to [`MAX_FETCH_ATTEMPTS`] tries. `label` (the asset names that
    /// use this URL) is only for the debug log, where every failure goes.
    pub fn fetch_json(&self, url: &str, label: &str) -> Result<Value, String> {
        for attempt in 1..=MAX_FETCH_ATTEMPTS {
            eprintln!("🔍 Fetching {label} from {url} (attempt {attempt}/{MAX_FETCH_ATTEMPTS})");
            let error = match self.fetch_json_once(url) {
                Ok(json) => return Ok(json),
                Err(e) => e,
            };
            crate::log_message(&format!(
                "fetch: {label} attempt {attempt}/{MAX_FETCH_ATTEMPTS} failed: {}",
                error.message()
            ));
            let wait = match &error {
                AttemptError::Final(_) => None,
                AttemptError::Retry { .. } if attempt == MAX_FETCH_ATTEMPTS => None,
                AttemptError::Retry { retry_after, .. } => {
                    let wait = retry_wait(
                        attempt,
                        self.retry_delay,
                        *retry_after,
                        self.max_retry_after,
                    );
                    if wait.is_none() {
                        crate::log_message(&format!(
                            "fetch: {label}: Retry-After {retry_after:?} is over {:?}; trying \
                             again at the next poll",
                            self.max_retry_after
                        ));
                    }
                    wait
                }
            };
            match wait {
                Some(wait) => thread::sleep(wait),
                None => {
                    crate::log_message(&format!(
                        "fetch: giving up on {label} after {attempt} attempt(s) ({url})"
                    ));
                    return Err(error.message().to_string());
                }
            }
        }
        unreachable!("the last attempt returns")
    }

    fn fetch_json_once(&self, url: &str) -> Result<Value, AttemptError> {
        let response = self.client.get(url).send().map_err(|e| {
            let message = format!("network error: {e}");
            if e.is_redirect() {
                AttemptError::Final(message)
            } else {
                AttemptError::Retry {
                    message,
                    retry_after: None,
                }
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            let message = format!("HTTP error: {status} for {url}");
            if status.as_u16() == 429 || status.is_server_error() {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| parse_retry_after(v, Utc::now()));
                return Err(AttemptError::Retry {
                    message,
                    retry_after,
                });
            }
            return Err(AttemptError::Final(message));
        }
        read_json_capped(response, MAX_BODY_BYTES).map_err(|e| {
            let message = format!("bad response: {e}");
            if e.starts_with("response too large") {
                AttemptError::Final(message)
            } else {
                AttemptError::Retry {
                    message,
                    retry_after: None,
                }
            }
        })
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
        let backoff = (1..MAX_FETCH_ATTEMPTS)
            .map(|a| retry_wait(a, RETRY_DELAY, None, MAX_RETRY_AFTER).unwrap())
            .sum::<Duration>();
        assert_eq!(
            REQUEST_TIMEOUT * MAX_FETCH_ATTEMPTS + backoff,
            Duration::from_millis(31_500)
        );
        // With the longest Retry-After waited for before both retries.
        let per_url =
            REQUEST_TIMEOUT * MAX_FETCH_ATTEMPTS + MAX_RETRY_AFTER * (MAX_FETCH_ATTEMPTS - 1);
        assert_eq!(per_url, Duration::from_secs(50));
        // The default config (5 distinct URLs) finishes within one default poll interval (5 min)
        // even if every request times out.
        let config = crate::config::parse_config(include_str!("../config.toml")).unwrap();
        let mut urls: Vec<&str> = config.assets.iter().map(|a| a.url.as_str()).collect();
        urls.sort();
        urls.dedup();
        assert!(per_url * urls.len() as u32 <= Duration::from_secs(5 * 60));
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

    #[test]
    fn retry_wait_backs_off_exponentially_and_honours_retry_after() {
        let base = Duration::from_millis(500);
        let max = Duration::from_secs(10);
        assert_eq!(retry_wait(1, base, None, max), Some(base));
        assert_eq!(retry_wait(2, base, None, max), Some(base * 2));
        assert_eq!(retry_wait(3, base, None, max), Some(base * 4));
        let three = Duration::from_secs(3);
        assert_eq!(retry_wait(1, base, Some(three), max), Some(three));
        assert_eq!(
            retry_wait(1, base, Some(Duration::ZERO), max),
            Some(base),
            "never sooner than the backoff"
        );
        assert_eq!(
            retry_wait(1, base, Some(Duration::from_secs(11)), max),
            None
        );
    }

    #[test]
    fn retry_after_as_seconds_or_http_date() {
        let now = DateTime::parse_from_rfc3339("2026-10-03T17:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(parse_retry_after(" 7 ", now), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after("Sat, 03 Oct 2026 17:00:30 GMT", now),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_retry_after("Sat, 03 Oct 2026 16:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("soon", now), None);
        assert_eq!(parse_retry_after("-1", now), None);
    }

    /// HTTP tests against a local server on `std::net::TcpListener` (tests only; ticker may use
    /// the network, copycraft's guards are not involved).
    mod http {
        use super::super::*;
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        /// What the server does with one request.
        enum Reply {
            /// Raw status line + headers (without the blank line) and a body.
            Respond(&'static str, String),
            /// Read the request and answer nothing for this long.
            Stall(Duration),
        }

        struct Server {
            url: String,
            requests: Arc<AtomicUsize>,
        }

        /// Serves `replies` in order, one per connection, then stops.
        fn serve(replies: Vec<Reply>) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/price", listener.local_addr().unwrap());
            let requests = Arc::new(AtomicUsize::new(0));
            let count = Arc::clone(&requests);
            thread::spawn(move || {
                for reply in replies {
                    let Ok((stream, _)) = listener.accept() else {
                        return;
                    };
                    count.fetch_add(1, Ordering::SeqCst);
                    answer(stream, reply);
                }
            });
            Server { url, requests }
        }

        fn answer(mut stream: TcpStream, reply: Reply) {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            // Request line and headers, up to the blank line (GET has no body).
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                if line == "\r\n" {
                    break;
                }
                line.clear();
            }
            match reply {
                Reply::Respond(head, body) => {
                    let _ = write!(
                        stream,
                        "{head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
                Reply::Stall(d) => thread::sleep(d),
            }
        }

        fn ok(body: &str) -> Reply {
            Reply::Respond(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json",
                body.to_string(),
            )
        }

        fn status(head: &'static str) -> Reply {
            Reply::Respond(head, String::new())
        }

        /// Short timeouts and delays; no proxy, so the requests stay on 127.0.0.1.
        fn fetcher(timeout: Duration) -> PriceFetcher {
            let client = Client::builder()
                .timeout(timeout)
                .connect_timeout(timeout)
                .redirect(redirect_policy())
                .no_proxy()
                .build()
                .unwrap();
            PriceFetcher {
                client,
                retry_delay: Duration::from_millis(20),
                max_retry_after: Duration::from_secs(2),
            }
        }

        fn quick() -> PriceFetcher {
            fetcher(Duration::from_secs(5))
        }

        #[test]
        fn retries_a_server_error_then_succeeds() {
            let server = serve(vec![
                status("HTTP/1.1 503 Service Unavailable"),
                status("HTTP/1.1 500 Internal Server Error"),
                ok(r#"{"p": 1.5}"#),
            ]);
            let json = quick().fetch_json(&server.url, "test").unwrap();
            assert_eq!(json["p"], 1.5);
            assert_eq!(server.requests.load(Ordering::SeqCst), 3);
        }

        #[test]
        fn gives_up_after_three_attempts() {
            let server = serve((0..3).map(|_| status("HTTP/1.1 502 Bad Gateway")).collect());
            let err = quick().fetch_json(&server.url, "test").unwrap_err();
            assert!(err.contains("502"), "{err}");
            assert_eq!(server.requests.load(Ordering::SeqCst), 3);
        }

        #[test]
        fn a_client_error_is_not_retried() {
            let server = serve(vec![status("HTTP/1.1 404 Not Found"), ok("{}")]);
            let err = quick().fetch_json(&server.url, "test").unwrap_err();
            assert!(err.contains("404"), "{err}");
            assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn waits_for_retry_after_on_429() {
            let server = serve(vec![
                status("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1"),
                ok(r#"{"p": 2}"#),
            ]);
            let start = Instant::now();
            let json = quick().fetch_json(&server.url, "test").unwrap();
            assert_eq!(json["p"], 2);
            assert!(
                start.elapsed() >= Duration::from_secs(1),
                "{:?}",
                start.elapsed()
            );
            assert_eq!(server.requests.load(Ordering::SeqCst), 2);
        }

        #[test]
        fn a_long_retry_after_ends_the_retries_for_this_poll() {
            let server = serve(vec![
                status("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 3600"),
                ok("{}"),
            ]);
            let start = Instant::now();
            assert!(quick().fetch_json(&server.url, "test").is_err());
            assert!(start.elapsed() < Duration::from_secs(2));
            assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn a_stalled_server_times_out() {
            let stall = Duration::from_millis(1500);
            let server = serve((0..3).map(|_| Reply::Stall(stall)).collect());
            let start = Instant::now();
            let err = fetcher(Duration::from_millis(300))
                .fetch_json(&server.url, "test")
                .unwrap_err();
            assert!(err.contains("network error"), "{err}");
            // Three timed-out attempts, not three full stalls.
            assert!(start.elapsed() < stall * 3, "{:?}", start.elapsed());
        }

        #[test]
        fn a_body_over_one_megabyte_is_refused_without_retry() {
            let big = format!(r#"{{"p": "{}"}}"#, "x".repeat(MAX_BODY_BYTES as usize));
            let server = serve(vec![ok(&big), ok("{}")]);
            let err = quick().fetch_json(&server.url, "test").unwrap_err();
            assert!(err.contains("too large"), "{err}");
            assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn a_redirect_to_plain_http_is_refused() {
            let server = serve(vec![
                status("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/elsewhere"),
                ok("{}"),
            ]);
            let err = quick().fetch_json(&server.url, "test").unwrap_err();
            assert!(err.contains("network error"), "{err}");
            assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn fetch_all_requests_a_shared_url_once() {
            let server = serve(vec![ok(r#"{"a": 1.0, "b": 2.0}"#), ok("{}")]);
            let asset = |name: &str, path: &str| Asset {
                name: name.into(),
                url: server.url.clone(),
                price_path: path.into(),
                unit: "EUR".into(),
                unit_hint: String::new(),
                symbol: String::new(),
                allow_negative: None,
                max_jump_pct: None,
            };
            let rows = quick().fetch_all(&[asset("A", "a"), asset("B", "b"), asset("C", "zz")]);
            assert_eq!(rows[0].price, 1.0);
            assert_eq!(rows[1].price, 2.0);
            assert!(rows[2].price.is_nan(), "missing path is not retried");
            assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        }
    }
}
