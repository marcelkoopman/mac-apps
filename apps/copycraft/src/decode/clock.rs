//! Dates and spans for the decoders, without a date crate: Unix seconds to a civil date and
//! time (proleptic Gregorian), the app's date order, local time from an offset the caller
//! supplies, and "3 days ago" / "3 dagen geleden". Pure, so it is tested on Linux.

use crate::locale::Lang;

/// Everything a decoded view depends on besides the copied text.
#[derive(Clone, Copy)]
pub struct Env {
    /// Now, in Unix milliseconds.
    pub now_ms: i64,
    pub lang: Lang,
    /// The Settings date order (`mm/dd/yyyy` when on), as the table reads ambiguous dates.
    pub month_first: bool,
    /// Seconds east of UTC in the local time zone at a Unix time (DST included).
    pub offset: fn(i64) -> i64,
}

impl Env {
    /// The real clock, the system language, the stored date order and the local time zone.
    pub fn current() -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self {
            now_ms,
            lang: crate::locale::lang(),
            month_first: crate::settings::load().date_month_first,
            offset: local_offset,
        }
    }

    /// Picks NL or EN.
    pub fn pick<'a>(&self, nl: &'a str, en: &'a str) -> &'a str {
        match self.lang {
            Lang::Nl => nl,
            Lang::En => en,
        }
    }
}

/// The local time zone's offset from UTC at `unix` seconds.
#[cfg(target_os = "macos")]
fn local_offset(unix: i64) -> i64 {
    use mac_ui::objc2_foundation::{NSDate, NSTimeZone};
    let date = NSDate::dateWithTimeIntervalSince1970(unix as f64);
    NSTimeZone::localTimeZone().secondsFromGMTForDate(&date) as i64
}

/// Off macOS (the Linux tests) local time is UTC.
#[cfg(not(target_os = "macos"))]
fn local_offset(_unix: i64) -> i64 {
    0
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's `civil_from_days`.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `secs` (Unix) as a date and time in the date order: `dd/mm/yyyy HH:MM:SS`, or
/// `mm/dd/yyyy …` with `month_first`. `millis` adds `.mmm`.
pub fn date_time(secs: i64, millis: Option<u32>, month_first: bool) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (first, second) = if month_first {
        (month, day)
    } else {
        (day, month)
    };
    let mut out = format!(
        "{first:02}/{second:02}/{year:04} {:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    );
    if let Some(ms) = millis {
        out.push_str(&format!(".{ms:03}"));
    }
    out
}

/// `UTC+02:00`, `UTC−05:30`, `UTC`.
pub fn offset_label(offset: i64) -> String {
    if offset == 0 {
        return "UTC".to_string();
    }
    let sign = if offset < 0 { '-' } else { '+' };
    let abs = offset.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60)
}

/// Two lines: local time with its offset, then UTC.
pub fn moment_lines(secs: i64, millis: Option<u32>, env: &Env) -> [String; 2] {
    let offset = (env.offset)(secs);
    let local = format!(
        "{}  {} ({})",
        date_time(secs.saturating_add(offset), millis, env.month_first),
        env.pick("lokale tijd", "local time"),
        offset_label(offset)
    );
    let utc = format!("{}  UTC", date_time(secs, millis, env.month_first));
    [local, utc]
}

/// A length of time, rounded down to its largest sensible unit: minutes up to two hours, hours
/// up to two days, days up to a year, then years.
pub fn span(secs: u64, lang: Lang) -> String {
    let nl = lang == Lang::Nl;
    let (n, one, many) = if secs < 60 {
        return if nl {
            "minder dan een minuut"
        } else {
            "less than a minute"
        }
        .to_string();
    } else if secs < 2 * 3600 {
        let n = secs / 60;
        if nl {
            (n, "minuut", "minuten")
        } else {
            (n, "minute", "minutes")
        }
    } else if secs < 2 * 86_400 {
        let n = secs / 3600;
        if nl {
            (n, "uur", "uur")
        } else {
            (n, "hour", "hours")
        }
    } else if secs < 365 * 86_400 {
        let n = secs / 86_400;
        if nl {
            (n, "dag", "dagen")
        } else {
            (n, "day", "days")
        }
    } else {
        let n = secs / (365 * 86_400);
        if nl {
            (n, "jaar", "jaar")
        } else {
            (n, "year", "years")
        }
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `then` relative to now: "3 days ago" / "3 dagen geleden", "in 5 minutes" / "over 5 minuten".
pub fn relative(then_secs: i64, env: &Env) -> String {
    let now = env.now_ms.div_euclid(1000);
    let diff = now.saturating_sub(then_secs);
    let amount = span(diff.unsigned_abs(), env.lang);
    match (diff >= 0, env.lang) {
        (true, Lang::Nl) => format!("{amount} geleden"),
        (true, Lang::En) => format!("{amount} ago"),
        (false, Lang::Nl) => format!("over {amount}"),
        (false, Lang::En) => format!("in {amount}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Env, civil_from_days, date_time, offset_label, relative, span};
    use crate::locale::Lang;

    /// A fixed clock for the decoder tests: 2026-10-06 08:00:00 UTC.
    pub const NOW_MS: i64 = 1_791_273_600_000;

    fn two_hours(_: i64) -> i64 {
        7200
    }

    pub fn env(lang: Lang) -> Env {
        Env {
            now_ms: NOW_MS,
            lang,
            month_first: false,
            offset: two_hours,
        }
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(date_time(NOW_MS / 1000, None, false), "06/10/2026 08:00:00");
        assert_eq!(date_time(NOW_MS / 1000, None, true), "10/06/2026 08:00:00");
        assert_eq!(
            date_time(1_000_000_000, Some(7), false),
            "09/09/2001 01:46:40.007"
        );
    }

    #[test]
    fn offsets() {
        assert_eq!(offset_label(0), "UTC");
        assert_eq!(offset_label(7200), "UTC+02:00");
        assert_eq!(offset_label(-19_800), "UTC-05:30");
    }

    #[test]
    fn spans_pick_a_unit() {
        assert_eq!(span(30, Lang::En), "less than a minute");
        assert_eq!(span(60, Lang::Nl), "1 minuut");
        assert_eq!(span(119 * 60, Lang::En), "119 minutes");
        assert_eq!(span(3 * 3600, Lang::Nl), "3 uur");
        assert_eq!(span(3 * 86_400, Lang::Nl), "3 dagen");
        assert_eq!(span(400 * 86_400, Lang::En), "1 year");
        assert_eq!(span(9 * 365 * 86_400, Lang::Nl), "9 jaar");
    }

    #[test]
    fn relative_to_now() {
        let now = NOW_MS / 1000;
        assert_eq!(
            relative(now - 3 * 86_400, &env(Lang::Nl)),
            "3 dagen geleden"
        );
        assert_eq!(relative(now + 300, &env(Lang::En)), "in 5 minutes");
        assert_eq!(relative(now + 300, &env(Lang::Nl)), "over 5 minuten");
    }
}
