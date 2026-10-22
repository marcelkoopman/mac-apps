//! ISO 8601 to Unix time: a date (`2026-10-22`) or a date and time (`2026-10-22T14:03:05`,
//! `T` or a space, seconds and a fraction optional) with an optional zone (`Z`, `+02:00`,
//! `+0200`, `-05`), from 1970 up to 2100. Shown as Unix time in seconds and milliseconds, then
//! in local time and UTC in the Settings date order. Without a zone the text does not say
//! which moment it is: both readings are shown, as local time first and as UTC.

use super::clock::{self, Env};

/// 2100-01-01 00:00:00 UTC, not included (as for Unix time).
const END: i64 = 4_102_444_800;

/// What the text says: the wall-clock time in seconds since 1970 as if it were UTC, its
/// milliseconds, and its offset from UTC when it has one.
#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    wall: i64,
    millis: u32,
    offset: Option<i64>,
}

/// The decoded view, or `None` when `text` is not an ISO 8601 date in range.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let stamp = parse(text)?;
    match stamp.offset {
        Some(offset) => {
            let secs = stamp.wall - offset;
            in_range(secs).then(|| zoned(secs, stamp.millis, env))
        }
        None => {
            let local = local_reading(stamp.wall, env);
            (in_range(local) && in_range(stamp.wall))
                .then(|| unzoned(local, stamp.wall, stamp.millis, env))
        }
    }
}

fn in_range(secs: i64) -> bool {
    (0..END).contains(&secs)
}

/// `wall` as local time: the Unix time whose local clock shows it (the offset taken at that
/// moment, so daylight saving time on that date counts).
fn local_reading(wall: i64, env: &Env) -> i64 {
    let guess = wall - (env.offset)(wall);
    wall - (env.offset)(guess)
}

fn unix_lines(secs: i64, millis: u32) -> String {
    format!("{secs}\n{}", secs * 1000 + i64::from(millis))
}

fn fraction(millis: u32) -> Option<u32> {
    (millis > 0).then_some(millis)
}

fn zoned(secs: i64, millis: u32, env: &Env) -> String {
    let [local, utc] = clock::moment_lines(secs, fraction(millis), env);
    format!(
        "{}\n\n{local}\n{utc}\n{}\n\n{}",
        unix_lines(secs, millis),
        clock::relative(secs, env),
        env.pick(
            "Unix-tijd in seconden en milliseconden",
            "Unix time in seconds and milliseconds"
        )
    )
}

fn unzoned(local: i64, utc: i64, millis: u32, env: &Env) -> String {
    let offset = clock::offset_label((env.offset)(local));
    format!(
        "{} ({offset}):\n{}\n\n{}:\n{}\n\n{}",
        env.pick("Als lokale tijd", "As local time"),
        unix_lines(local, millis),
        env.pick("Als UTC", "As UTC"),
        unix_lines(utc, millis),
        env.pick(
            "Zonder tijdzone in de tekst; Unix-tijd in seconden en milliseconden",
            "No time zone in the text; Unix time in seconds and milliseconds"
        )
    )
}

/// Days since 1970-01-01 for a civil date. Howard Hinnant's `days_from_civil`, the inverse of
/// [`clock::civil_from_days`].
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `count` ASCII digits at the start of `text` as a number, and the rest.
fn digits(text: &str, count: usize) -> Option<(u32, &str)> {
    let head = text.get(..count)?;
    if !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head.parse().ok()?, &text[count..]))
}

fn parse(text: &str) -> Option<Stamp> {
    let (year, rest) = digits(text, 4)?;
    let rest = rest.strip_prefix('-')?;
    let (month, rest) = digits(rest, 2)?;
    let rest = rest.strip_prefix('-')?;
    let (day, rest) = digits(rest, 2)?;
    if !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let days = days_from_civil(i64::from(year), month, day);
    // 31 February rolls over into March: not a date.
    if clock::civil_from_days(days) != (i64::from(year), month, day) {
        return None;
    }
    let mut stamp = Stamp {
        wall: days * 86_400,
        millis: 0,
        offset: None,
    };
    if rest.is_empty() {
        return Some(stamp);
    }
    let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let (hour, rest) = digits(rest, 2)?;
    let rest = rest.strip_prefix(':')?;
    let (minute, mut rest) = digits(rest, 2)?;
    let mut second = 0;
    if let Some(after) = rest.strip_prefix(':') {
        (second, rest) = digits(after, 2)?;
        if let Some(after) = rest.strip_prefix(['.', ',']) {
            let len = after.bytes().take_while(u8::is_ascii_digit).count();
            if len == 0 {
                return None;
            }
            let (ms, _) = digits(&format!("{:0<3}", &after[..len.min(3)]), 3)?;
            stamp.millis = ms;
            rest = &after[len..];
        }
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    stamp.wall += i64::from(hour * 3600 + minute * 60 + second);
    stamp.offset = zone(rest)?;
    Some(stamp)
}

/// The zone at the end: none, `Z`, or `±HH`, `±HHMM`, `±HH:MM`. `None` when it is anything
/// else; `Some(None)` when there is no zone.
fn zone(text: &str) -> Option<Option<i64>> {
    if text.is_empty() {
        return Some(None);
    }
    if text == "Z" || text == "z" {
        return Some(Some(0));
    }
    let sign = match text.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, rest) = digits(&text[1..], 2)?;
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    let minutes = if rest.is_empty() {
        0
    } else {
        let (minutes, rest) = digits(rest, 2)?;
        if !rest.is_empty() {
            return None;
        }
        minutes
    };
    if hours > 14 || minutes > 59 {
        return None;
    }
    Some(Some(sign * i64::from(hours * 3600 + minutes * 60)))
}

#[cfg(test)]
mod tests {
    use super::{Stamp, days_from_civil, decode, parse};
    use crate::decode::clock::{civil_from_days, tests::env};
    use crate::locale::Lang;

    #[test]
    fn days_from_civil_undoes_civil_from_days() {
        for days in [-1, 0, 11_016, 20_748, 47_481] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
    }

    #[test]
    fn a_zoned_time_is_one_moment() {
        let out = decode("2026-10-22T14:03:05+02:00", &env(Lang::En)).expect("iso");
        assert!(out.starts_with("1792670585\n1792670585000\n\n"), "{out}");
        assert!(out.contains("22/10/2026 12:03:05  UTC"), "{out}");
        for same in [
            "2026-10-22T12:03:05Z",
            "2026-10-22 12:03:05z",
            "2026-10-22T14:03:05+0200",
            "2026-10-22T14:03:05+02",
            "2026-10-22T06:33:05-05:30",
        ] {
            let other = decode(same, &env(Lang::En)).expect(same);
            assert!(other.starts_with("1792670585\n"), "{same}: {other}");
        }
        let ms = decode("2026-10-22T12:03:05.678Z", &env(Lang::Nl)).expect("ms");
        assert!(ms.starts_with("1792670585\n1792670585678\n"), "{ms}");
        assert!(ms.contains("22/10/2026 12:03:05.678  UTC"), "{ms}");
        assert!(
            ms.ends_with("Unix-tijd in seconden en milliseconden"),
            "{ms}"
        );
    }

    #[test]
    fn without_a_zone_both_readings_show() {
        // The test clock is two hours east of UTC.
        let out = decode("2026-10-22T14:03:05", &env(Lang::En)).expect("iso");
        assert_eq!(
            out,
            "As local time (UTC+02:00):\n1792670585\n1792670585000\n\nAs UTC:\n1792677785\n1792677785000\n\nNo time zone in the text; Unix time in seconds and milliseconds"
        );
        let date = decode("2026-10-22", &env(Lang::Nl)).expect("date");
        assert!(
            date.starts_with("Als lokale tijd (UTC+02:00):\n1792620000\n"),
            "{date}"
        );
        assert!(date.contains("Als UTC:\n1792627200\n"), "{date}");
    }

    #[test]
    fn not_a_date_or_out_of_range() {
        for text in [
            "2026-02-30",
            "2026-13-01",
            "2026-10-22T24:00:00Z",
            "2026-10-22T12:60",
            "2026-10-22T12:00:00+15:00",
            "2026-10-22T12:00:00 UTC",
            "2026-10-22T12",
            "2026-10-22T12:00:00.Z",
            "26-10-22",
            "2026/10/22",
            "1969-12-31T23:59:59Z",
            "2100-01-01T00:00:00Z",
            "on 2026-10-22",
        ] {
            assert_eq!(decode(text, &env(Lang::En)), None, "{text}");
        }
        assert_eq!(
            parse("2024-02-29T00:00"),
            Some(Stamp {
                wall: 1_709_164_800,
                millis: 0,
                offset: None
            })
        );
    }
}
