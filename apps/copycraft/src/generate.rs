//! ⋯ › New: values made on the spot, each a new card and on the clipboard. A UUID (version 4,
//! random; version 7, time-ordered, RFC 9562), the Unix time in seconds or milliseconds, and
//! the local date and time now as ISO 8601. No crate: the random bits come from `NSUUID`
//! (the system's random UUID), the dates from the decoders' civil-date arithmetic.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::decode::clock::civil_from_days;

/// One item of the New submenu, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    UuidV4,
    UuidV7,
    UnixSeconds,
    UnixMillis,
    IsoNow,
}

/// The moment a value is made for: Unix milliseconds and the local offset from UTC then.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    pub unix_ms: i64,
    /// Seconds east of UTC, daylight saving time included.
    pub offset: i64,
}

impl Moment {
    /// The clock and the local time zone, now.
    pub fn now() -> Self {
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self {
            unix_ms,
            offset: crate::decode::clock::local_offset(unix_ms.div_euclid(1000)),
        }
    }
}

impl Generator {
    pub const ALL: [Generator; 5] = [
        Generator::UuidV4,
        Generator::UuidV7,
        Generator::UnixSeconds,
        Generator::UnixMillis,
        Generator::IsoNow,
    ];

    /// Its menu item, also the new card's title.
    pub fn title(self) -> &'static str {
        match self {
            Generator::UuidV4 => crate::locale::t("generate_uuid_v4"),
            Generator::UuidV7 => crate::locale::t("generate_uuid_v7"),
            Generator::UnixSeconds => crate::locale::t("generate_unix_seconds"),
            Generator::UnixMillis => crate::locale::t("generate_unix_millis"),
            Generator::IsoNow => crate::locale::t("generate_iso_now"),
        }
    }

    /// The value for `moment`, with `random` as the UUIDs' random bits.
    pub fn value(self, moment: Moment, random: [u8; 16]) -> String {
        match self {
            Generator::UuidV4 => uuid_v4(random),
            Generator::UuidV7 => uuid_v7(moment.unix_ms, random),
            Generator::UnixSeconds => moment.unix_ms.div_euclid(1000).to_string(),
            Generator::UnixMillis => moment.unix_ms.to_string(),
            Generator::IsoNow => iso_local(moment.unix_ms.div_euclid(1000), moment.offset),
        }
    }

    /// The value now, from the system clock and random source.
    pub fn make(self) -> String {
        self.value(Moment::now(), random_bytes())
    }
}

/// The 8-4-4-4-12 lowercase form of 16 bytes.
fn uuid_text(bytes: [u8; 16]) -> String {
    let hex = crate::text_ops::encoding::hex_encode(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Version 4: random, with the version (4) and the RFC variant (10) set.
pub fn uuid_v4(mut random: [u8; 16]) -> String {
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    uuid_text(random)
}

/// Version 7: the 48-bit Unix time in milliseconds first, then version 7, 12 random bits, the
/// variant and 62 random bits (RFC 9562 §5.7).
pub fn uuid_v7(unix_ms: i64, mut random: [u8; 16]) -> String {
    let ms = u64::try_from(unix_ms).unwrap_or(0) & 0xffff_ffff_ffff;
    random[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
    random[6] = (random[6] & 0x0f) | 0x70;
    random[8] = (random[8] & 0x3f) | 0x80;
    uuid_text(random)
}

/// `2026-10-22T14:03:05+02:00`: `secs` (Unix) in local time with its offset; `Z` at UTC.
pub fn iso_local(secs: i64, offset: i64) -> String {
    let local = secs.saturating_add(offset);
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let rest = local.rem_euclid(86_400);
    let zone = if offset == 0 {
        "Z".to_string()
    } else {
        let sign = if offset < 0 { '-' } else { '+' };
        let abs = offset.unsigned_abs();
        format!("{sign}{:02}:{:02}", abs / 3600, abs % 3600 / 60)
    };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}{zone}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// 16 random bytes: those of a fresh `NSUUID` (the system's random UUID; its six fixed
/// version and variant bits are set again by [`uuid_v4`] and [`uuid_v7`]).
#[cfg(target_os = "macos")]
fn random_bytes() -> [u8; 16] {
    let text = mac_ui::objc2_foundation::NSUUID::UUID()
        .UUIDString()
        .to_string();
    let digits: Vec<u8> = text
        .bytes()
        .filter_map(|b| (b as char).to_digit(16).map(|d| d as u8))
        .collect();
    let mut out = [0u8; 16];
    for (at, pair) in digits.chunks(2).take(16).enumerate() {
        out[at] = pair[0] << 4 | pair.get(1).copied().unwrap_or(0);
    }
    out
}

/// Off macOS (the Linux tests): the standard library's random hash keys.
#[cfg(not(target_os = "macos"))]
fn random_bytes() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for half in out.chunks_mut(8) {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u8(0);
        half.copy_from_slice(&hasher.finish().to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Generator, Moment, iso_local, uuid_v4, uuid_v7};

    /// 2026-10-22 12:03:05.678 UTC.
    const MS: i64 = 1_792_670_585_678;

    fn shape(uuid: &str, version: char) {
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert_eq!(uuid.chars().nth(14), Some(version), "{uuid}");
        assert!(
            matches!(uuid.chars().nth(19), Some('8' | '9' | 'a' | 'b')),
            "{uuid}"
        );
    }

    #[test]
    fn uuids_carry_their_version_and_variant() {
        for random in [[0u8; 16], [0xff; 16], [0x5a; 16]] {
            shape(&uuid_v4(random), '4');
            shape(&uuid_v7(MS, random), '7');
        }
        assert_eq!(uuid_v4([0; 16]), "00000000-0000-4000-8000-000000000000");
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn a_v7_uuid_starts_with_the_time_in_milliseconds() {
        let uuid = uuid_v7(MS, [0; 16]);
        let hex: String = uuid.split('-').take(2).collect();
        assert_eq!(i64::from_str_radix(&hex, 16).expect("hex"), MS);
        // RFC 9562 appendix B.2: 2022-02-22 19:22:22 UTC is 017F22E279B0.
        assert!(uuid_v7(0x017F_22E2_79B0, [0; 16]).starts_with("017f22e2-79b0-7"));
        // Later is larger, as text too.
        assert!(uuid_v7(MS + 1, [0; 16]) > uuid_v7(MS, [0xff; 16]));
    }

    #[test]
    fn two_made_uuids_differ() {
        assert_ne!(Generator::UuidV4.make(), Generator::UuidV4.make());
        shape(&Generator::UuidV7.make(), '7');
    }

    #[test]
    fn unix_time_and_iso_8601_now() {
        let moment = Moment {
            unix_ms: MS,
            offset: 7200,
        };
        let value = |generator: Generator| generator.value(moment, [0; 16]);
        assert_eq!(value(Generator::UnixSeconds), "1792670585");
        assert_eq!(value(Generator::UnixMillis), "1792670585678");
        assert_eq!(value(Generator::IsoNow), "2026-10-22T14:03:05+02:00");
        assert_eq!(iso_local(1_792_670_585, 0), "2026-10-22T12:03:05Z");
        assert_eq!(
            iso_local(1_792_670_585, -19_800),
            "2026-10-22T06:33:05-05:30"
        );
        assert_eq!(iso_local(0, 0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn every_generator_has_a_title() {
        for generator in Generator::ALL {
            assert_ne!(generator.title(), "?", "{generator:?}");
        }
    }
}
