//! How fresh each asset's price is: when it was last fetched and how many polls in a row failed.
//! A price kept from an earlier poll (`fill_nan_from_prev`) is marked stale after
//! [`STALE_AFTER_FAILED_POLLS`] failed polls or when it is older than [`STALE_AFTER`].

use chrono::{DateTime, Local, TimeDelta};

pub const STALE_AFTER_FAILED_POLLS: u32 = 2;
pub const STALE_AFTER: TimeDelta = TimeDelta::minutes(15);

/// Prefix of a stale row and title. U+26A0 with the text presentation selector (U+FE0E), so it
/// renders as a plain glyph, not an emoji.
pub const STALE_MARK: &str = "\u{26A0}\u{FE0E}";

/// Age after which a price is stale with polls every `interval`: [`STALE_AFTER`] (three polls at
/// the default 5 minutes), or three intervals when that is longer.
pub fn stale_after(interval: std::time::Duration) -> TimeDelta {
    let three = TimeDelta::from_std(interval * 3).unwrap_or(TimeDelta::MAX);
    three.max(STALE_AFTER)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AssetStatus {
    /// Last poll that fetched a price for the asset.
    pub last_ok: Option<DateTime<Local>>,
    /// Polls in a row without a price since then.
    pub failed_polls: u32,
}

impl AssetStatus {
    /// One finished poll: `fetched` when it returned a price for the asset.
    pub fn record(&mut self, fetched: bool, now: DateTime<Local>) {
        if fetched {
            self.last_ok = Some(now);
            self.failed_polls = 0;
        } else {
            self.failed_polls = self.failed_polls.saturating_add(1);
        }
    }

    /// Stale with the default age limit [`STALE_AFTER`] (5-minute polls).
    #[cfg(test)]
    pub fn is_stale(&self, now: DateTime<Local>) -> bool {
        self.is_stale_after(now, STALE_AFTER)
    }

    /// [`AssetStatus::is_stale`] with another age limit (see [`stale_after`]).
    pub fn is_stale_after(&self, now: DateTime<Local>, max_age: TimeDelta) -> bool {
        self.failed_polls >= STALE_AFTER_FAILED_POLLS
            || self.last_ok.is_some_and(|t| now - t > max_age)
    }

    /// Second menu line text: `updated 14:05` (with the date when not today), or `no data yet`.
    pub fn updated_label(&self, now: DateTime<Local>) -> String {
        match self.last_ok {
            Some(t) if t.date_naive() == now.date_naive() => {
                format!("updated {}", t.format("%H:%M"))
            }
            Some(t) => format!("updated {}", t.format("%d-%m %H:%M")),
            None => "no data yet".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 3, h, m, 0).unwrap()
    }

    #[test]
    fn stale_age_follows_a_longer_interval() {
        use std::time::Duration;
        assert_eq!(stale_after(Duration::from_secs(60)), STALE_AFTER);
        assert_eq!(stale_after(Duration::from_secs(300)), STALE_AFTER);
        assert_eq!(
            stale_after(Duration::from_secs(1800)),
            TimeDelta::minutes(90)
        );
        let mut s = AssetStatus::default();
        s.record(true, at(14, 0));
        assert!(!s.is_stale_after(at(15, 0), TimeDelta::minutes(90)));
        assert!(s.is_stale_after(at(15, 31), TimeDelta::minutes(90)));
    }

    #[test]
    fn fresh_until_two_failed_polls() {
        let mut s = AssetStatus::default();
        s.record(true, at(14, 0));
        assert!(!s.is_stale(at(14, 5)));
        s.record(false, at(14, 5));
        assert!(!s.is_stale(at(14, 5)));
        s.record(false, at(14, 10));
        assert!(s.is_stale(at(14, 10)));
        s.record(true, at(14, 15));
        assert!(!s.is_stale(at(14, 15)));
        assert_eq!(s.failed_polls, 0);
    }

    #[test]
    fn stale_when_older_than_fifteen_minutes() {
        let mut s = AssetStatus::default();
        s.record(true, at(14, 0));
        assert!(!s.is_stale(at(14, 15)));
        assert!(s.is_stale(at(14, 16)));
    }

    #[test]
    fn never_fetched_is_stale_after_two_failures() {
        let mut s = AssetStatus::default();
        assert!(!s.is_stale(at(9, 0)));
        s.record(false, at(9, 0));
        s.record(false, at(9, 5));
        assert!(s.is_stale(at(9, 5)));
        assert_eq!(s.updated_label(at(9, 5)), "no data yet");
    }

    #[test]
    fn label_shows_time_and_date_when_not_today() {
        let mut s = AssetStatus::default();
        s.record(true, at(14, 5));
        assert_eq!(s.updated_label(at(15, 0)), "updated 14:05");
        let tomorrow = Local.with_ymd_and_hms(2026, 10, 4, 9, 0, 0).unwrap();
        assert_eq!(s.updated_label(tomorrow), "updated 03-10 14:05");
    }

    #[test]
    fn stale_mark_is_text_presentation() {
        assert_eq!(STALE_MARK.chars().count(), 2);
        assert!(STALE_MARK.ends_with('\u{FE0E}'));
    }
}
