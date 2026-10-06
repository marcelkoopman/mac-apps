//! Unix time: a bare integer of 10 digits (seconds) or 13 digits (milliseconds) between
//! 2001-09-09 (1 000 000 000 s) and 2100-01-01. Shown in local time and UTC in the Settings date
//! order, relative to now ("3 dagen geleden"), and as ISO 8601 UTC.

use super::clock::{self, Env};

/// 2001-09-09 01:46:40 UTC: the first 10-digit second.
const FIRST: i64 = 1_000_000_000;
/// 2100-01-01 00:00:00 UTC, not included.
const END: i64 = 4_102_444_800;

/// The decoded view, or `None` when `text` is not a Unix time in range.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let (secs, millis) = parse(text)?;
    let [local, utc] = clock::moment_lines(secs, millis, env);
    let unit = match (millis.is_some(), env.lang) {
        (true, crate::locale::Lang::Nl) => "Unix-tijd in milliseconden",
        (true, crate::locale::Lang::En) => "Unix time in milliseconds",
        (false, crate::locale::Lang::Nl) => "Unix-tijd in seconden",
        (false, crate::locale::Lang::En) => "Unix time in seconds",
    };
    Some(format!(
        "{local}\n{utc}\n{}\n\nISO 8601: {}\n{unit}",
        clock::relative(secs, env),
        iso(secs, millis)
    ))
}

/// Seconds, and milliseconds for a 13-digit value.
fn parse(text: &str) -> Option<(i64, Option<u32>)> {
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    let (secs, millis) = match text.len() {
        10 => (value, None),
        13 => (value / 1000, Some((value % 1000) as u32)),
        _ => return None,
    };
    (FIRST..END).contains(&secs).then_some((secs, millis))
}

/// `2026-10-06T08:00:00Z` (`.123Z` with milliseconds).
fn iso(secs: i64, millis: Option<u32>) -> String {
    let (year, month, day) = clock::civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    let fraction = millis.map(|ms| format!(".{ms:03}")).unwrap_or_default();
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{fraction}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    #[test]
    fn seconds_in_local_time_utc_and_relative() {
        let out = decode(&testdata("unix_seconds.txt"), &env(Lang::Nl)).expect("seconds");
        assert_eq!(
            out,
            "15/11/2023 00:13:20  lokale tijd (UTC+02:00)\n14/11/2023 22:13:20  UTC\n2 jaar geleden\n\nISO 8601: 2023-11-14T22:13:20Z\nUnix-tijd in seconden"
        );
        let mut month_first = env(Lang::En);
        month_first.month_first = true;
        let out = decode("1700000000", &month_first).expect("seconds");
        assert!(out.starts_with("11/15/2023 00:13:20  local time (UTC+02:00)\n11/14/2023 22:13:20  UTC\n2 years ago\n"), "{out}");
    }

    #[test]
    fn milliseconds_keep_their_fraction() {
        let mut clock = env(Lang::En);
        clock.now_ms -= 3 * 86_400 * 1000;
        let out = decode(&testdata("unix_milliseconds.txt"), &clock).expect("millis");
        assert!(
            out.contains("06/10/2026 08:00:00.123  UTC\nin 3 days\n"),
            "{out}"
        );
        assert!(out.contains("ISO 8601: 2026-10-06T08:00:00.123Z\nUnix time in milliseconds"));
    }

    #[test]
    fn out_of_range_and_other_numbers() {
        for line in testdata("unix_negative_numbers.txt").lines() {
            assert_eq!(decode(line, &env(Lang::En)), None, "{line}");
        }
        assert!(decode("1000000000", &env(Lang::En)).is_some());
        assert!(decode("4102444799", &env(Lang::En)).is_some());
        assert!(decode("4102444800", &env(Lang::En)).is_none());
    }
}
