//! Price history for the 24h change and the sparkline: per asset a ring buffer of up to
//! [`CAPACITY`] points (7 days at one per 5-minute poll), kept in memory and saved to
//! `~/.ticker_price_series.json` with [`write_atomic`] after every poll.

use crate::atomic_file::{move_aside, write_atomic};
use crate::prices::Change;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Seconds between points; a poll sooner than half of this after the last point (Poll now)
/// replaces that point instead of adding one.
pub const STEP_SECS: i64 = 5 * 60;
/// Points kept per asset: 7 days of 5-minute steps.
pub const CAPACITY: usize = 7 * 24 * 12;
const MAX_AGE_SECS: i64 = 7 * 24 * 3600;
const DAY_SECS: i64 = 24 * 3600;
/// The 24h change needs a point at least this old (23 h); a younger history shows none.
const MIN_BASE_AGE_SECS: i64 = 23 * 3600;
/// Bars in the sparkline: one per hour of the last 24.
pub const SPARK_BARS: usize = 24;
const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// `(unix seconds, price)`.
pub type Point = (i64, f64);

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceSeries {
    #[serde(default)]
    assets: HashMap<String, VecDeque<Point>>,
}

impl PriceSeries {
    /// Add a fetched price at `now` (unix seconds). Non-finite prices are ignored.
    pub fn record(&mut self, name: &str, now: i64, price: f64) {
        if !price.is_finite() {
            return;
        }
        let points = self.assets.entry(name.to_string()).or_default();
        if let Some(last) = points.back_mut()
            && now - last.0 < STEP_SECS / 2
        {
            *last = (now, price);
        } else {
            points.push_back((now, price));
        }
        while points.len() > CAPACITY || points.front().is_some_and(|p| now - p.0 > MAX_AGE_SECS) {
            points.pop_front();
        }
    }

    /// Drop assets that are no longer configured.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.assets.retain(|name, _| keep(name));
    }

    pub fn points(&self, name: &str) -> Option<&VecDeque<Point>> {
        self.assets.get(name)
    }

    /// Change from the price 24 hours before `now` (the first point at or after that moment,
    /// if it is at least 23 hours old) to `price`.
    pub fn change_24h(&self, name: &str, now: i64, price: f64) -> Option<Change> {
        let base = self
            .points(name)?
            .iter()
            .find(|p| p.0 >= now - DAY_SECS)
            .filter(|p| now - p.0 >= MIN_BASE_AGE_SECS)?;
        Change::between(base.1, price)
    }

    /// The last 24 hours as [`SPARK_BARS`] bars (the last price of each hour, scaled between
    /// that day's low and high; a flat day is a row of middle bars). Hours without a point are
    /// left out. `None` with fewer than two bars.
    pub fn sparkline(&self, name: &str, now: i64) -> Option<String> {
        let points = self.points(name)?;
        let start = now - DAY_SECS;
        let bucket_len = DAY_SECS / SPARK_BARS as i64;
        let mut buckets: Vec<Option<f64>> = vec![None; SPARK_BARS];
        for &(t, price) in points.iter().filter(|p| p.0 > start && p.0 <= now) {
            let i = (((t - start - 1) / bucket_len) as usize).min(SPARK_BARS - 1);
            buckets[i] = Some(price);
        }
        let values: Vec<f64> = buckets.into_iter().flatten().collect();
        if values.len() < 2 {
            return None;
        }
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let top = (LEVELS.len() - 1) as f64;
        Some(
            values
                .iter()
                .map(|v| {
                    if high - low <= f64::EPSILON * high.abs().max(1.0) {
                        LEVELS[LEVELS.len() / 2 - 1]
                    } else {
                        LEVELS[(((v - low) / (high - low)) * top).round() as usize]
                    }
                })
                .collect(),
        )
    }
}

/// `None` without a home directory: the series then lives in memory only.
fn series_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ticker_price_series.json"))
}

/// The saved series; empty when there is none. A file that does not parse is moved to `.bak`
/// with a log line.
pub fn load() -> PriceSeries {
    series_path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn load_from(path: &Path) -> PriceSeries {
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PriceSeries::default(),
        Err(e) => {
            crate::log_message(&format!(
                "price series: cannot read {}: {e}",
                path.display()
            ));
            return PriceSeries::default();
        }
    };
    match serde_json::from_str(&data) {
        Ok(series) => series,
        Err(e) => {
            let moved = move_aside(path)
                .map(|b| format!("moved to {}", b.display()))
                .unwrap_or_else(|m| format!("could not be moved aside ({m})"));
            crate::log_message(&format!(
                "price series: {} is corrupt ({e}); {moved}; starting over",
                path.display()
            ));
            PriceSeries::default()
        }
    }
}

pub fn save(series: &PriceSeries) -> Result<(), Box<dyn std::error::Error>> {
    match series_path() {
        Some(path) => save_to(&path, series),
        None => Ok(()),
    }
}

pub fn save_to(path: &Path, series: &PriceSeries) -> Result<(), Box<dyn std::error::Error>> {
    // Compact: ~2000 points per asset.
    let data = serde_json::to_string(series)?;
    write_atomic(path, data.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_790_000_000;

    #[test]
    fn ring_buffer_keeps_seven_days_at_most() {
        let mut s = PriceSeries::default();
        for i in 0..(CAPACITY as i64 + 500) {
            s.record("BTC", T0 + i * STEP_SECS, i as f64 + 1.0);
        }
        let points = s.points("BTC").unwrap();
        assert!(points.len() <= CAPACITY);
        let span = points.back().unwrap().0 - points.front().unwrap().0;
        assert!(span <= MAX_AGE_SECS);
    }

    #[test]
    fn a_quick_second_poll_replaces_the_last_point() {
        let mut s = PriceSeries::default();
        s.record("BTC", T0, 1.0);
        s.record("BTC", T0 + 60, 2.0);
        s.record("BTC", T0 + STEP_SECS, 3.0);
        s.record("BTC", T0 + STEP_SECS + 1, f64::NAN);
        let points: Vec<Point> = s.points("BTC").unwrap().iter().copied().collect();
        assert_eq!(points, vec![(T0 + 60, 2.0), (T0 + STEP_SECS, 3.0)]);
    }

    #[test]
    fn change_24h_needs_a_day_of_history() {
        let mut s = PriceSeries::default();
        for i in 0..=288 {
            s.record("BTC", T0 + i * STEP_SECS, 100.0 + i as f64);
        }
        let now = T0 + 288 * STEP_SECS;
        let c = s.change_24h("BTC", now, 388.0).unwrap();
        assert_eq!(c.amount, 288.0);
        assert!((c.pct - 288.0).abs() < 1e-9);
        let mut young = PriceSeries::default();
        young.record("BTC", T0, 100.0);
        young.record("BTC", T0 + 3600, 110.0);
        assert_eq!(young.change_24h("BTC", T0 + 3600, 110.0), None);
        assert_eq!(young.change_24h("ETH", T0, 1.0), None);
    }

    #[test]
    fn sparkline_scales_hourly_bars_between_low_and_high() {
        let mut s = PriceSeries::default();
        for h in 0..24 {
            s.record("BTC", T0 + h * 3600, h as f64);
        }
        let now = T0 + 23 * 3600;
        let line = s.sparkline("BTC", now).unwrap();
        assert_eq!(line.chars().count(), 24);
        assert_eq!(line.chars().next(), Some('▁'));
        assert_eq!(line.chars().last(), Some('█'));

        let mut flat = PriceSeries::default();
        flat.record("G", T0, 5.0);
        flat.record("G", T0 + 3600, 5.0);
        assert_eq!(flat.sparkline("G", T0 + 3600).as_deref(), Some("▄▄"));
        let mut one = PriceSeries::default();
        one.record("G", T0, 5.0);
        assert_eq!(one.sparkline("G", T0), None);
    }

    #[test]
    fn negative_prices_work_too() {
        let mut s = PriceSeries::default();
        s.record("P", T0, -0.02);
        s.record("P", T0 + 3600, 0.10);
        assert_eq!(s.sparkline("P", T0 + 3600).as_deref(), Some("▁█"));
    }

    #[test]
    fn save_and_load_roundtrip_and_corrupt_file_goes_to_bak() {
        let dir = std::env::temp_dir().join(format!("ticker-series-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("series.json");
        let mut s = PriceSeries::default();
        s.record("BTC", T0, 1.5);
        save_to(&path, &s).unwrap();
        assert_eq!(load_from(&path), s);
        fs::write(&path, "{ nope").unwrap();
        assert_eq!(load_from(&path), PriceSeries::default());
        assert!(dir.join("series.json.bak").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
