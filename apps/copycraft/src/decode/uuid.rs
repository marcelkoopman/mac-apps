//! UUID: canonical `8-4-4-4-12` hex, optionally in braces or after `urn:uuid:`. Shows the
//! version and variant; for version 1 (60-bit count of 100 ns since 1582-10-15) and version 7
//! (48-bit Unix milliseconds) also the time in local time and UTC. 32 hex digits without dashes
//! are not a UUID here (see the hash decoder).

use super::clock::{self, Env};

/// Seconds from 1582-10-15 (the Gregorian reform, UUID v1 epoch) to 1970-01-01.
const GREGORIAN_TO_UNIX: i64 = 12_219_292_800;

/// The decoded view, or `None` when `text` is not a canonical UUID.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let hex = canonical(text)?;
    let digit = |at: usize| hex.as_bytes()[at] as char;
    let version = digit(14).to_digit(16)?;
    let variant_bits = digit(19).to_digit(16)?;
    let nil = hex.bytes().all(|b| b == b'0' || b == b'-');
    let max = hex.bytes().all(|b| b == b'f' || b == b'-');
    let mut lines = vec![format!("UUID: {hex}")];
    if nil || max {
        lines.push(if nil {
            env.pick("Nil-UUID (alleen nullen)", "Nil UUID (all zeros)")
                .to_string()
        } else {
            env.pick("Max-UUID (alleen f)", "Max UUID (all f)")
                .to_string()
        });
        return Some(lines.join("\n"));
    }
    let rfc = variant_bits & 0b1100 == 0b1000;
    let variant = match variant_bits {
        0..=7 => env.pick("NCS (verouderd)", "NCS (legacy)"),
        8..=11 => "RFC 9562 (RFC 4122)",
        12 | 13 => "Microsoft (GUID)",
        _ => env.pick("gereserveerd", "reserved"),
    };
    if rfc {
        lines.push(format!(
            "{} {version} ({})",
            env.pick("Versie", "Version"),
            version_name(version, env)
        ));
    } else {
        lines.push(format!(
            "{}: {}",
            env.pick("Versie", "Version"),
            env.pick("geen (niet-RFC-variant)", "none (not the RFC variant)")
        ));
    }
    lines.push(format!("Variant: {variant}"));
    let moment = match (rfc, version) {
        (true, 1) => Some(v1_time(&hex)?),
        (true, 7) => Some(v7_time(&hex)?),
        _ => None,
    };
    if let Some((secs, millis)) = moment {
        let [local, utc] = clock::moment_lines(secs, Some(millis), env);
        lines.push(String::new());
        lines.push(format!("{}:", env.pick("Tijdstempel", "Timestamp")));
        lines.push(local);
        lines.push(utc);
        lines.push(clock::relative(secs, env));
    }
    Some(lines.join("\n"))
}

/// The 36-character lowercase form, from `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, `{…}` or
/// `urn:uuid:…`.
fn canonical(text: &str) -> Option<String> {
    let inner = if let Some(rest) = text.strip_prefix('{') {
        rest.strip_suffix('}')?
    } else if let Some(rest) = text
        .get(..9)
        .filter(|head| head.eq_ignore_ascii_case("urn:uuid:"))
        .and_then(|_| text.get(9..))
    {
        rest
    } else {
        text
    };
    let bytes = inner.as_bytes();
    if bytes.len() != 36 {
        return None;
    }
    let ok = bytes.iter().enumerate().all(|(at, b)| match at {
        8 | 13 | 18 | 23 => *b == b'-',
        _ => b.is_ascii_hexdigit(),
    });
    ok.then(|| inner.to_ascii_lowercase())
}

fn version_name(version: u32, env: &Env) -> &'static str {
    match version {
        1 => env.pick("tijd en node", "time and node"),
        2 => env.pick("DCE-beveiliging", "DCE security"),
        3 => env.pick("naam, MD5", "name-based, MD5"),
        4 => env.pick("willekeurig", "random"),
        5 => env.pick("naam, SHA-1", "name-based, SHA-1"),
        6 => env.pick("tijd, herschikt", "reordered time"),
        7 => env.pick("Unix-tijd", "Unix time"),
        8 => env.pick("eigen indeling", "custom"),
        _ => env.pick("onbekend", "unknown"),
    }
}

fn field(hex: &str, range: std::ops::Range<usize>) -> Option<u64> {
    u64::from_str_radix(hex.get(range)?, 16).ok()
}

/// Version 1: `time_hi` (12 bits after the version digit), `time_mid`, `time_low`.
fn v1_time(hex: &str) -> Option<(i64, u32)> {
    let ticks = field(hex, 15..18)? << 48 | field(hex, 9..13)? << 32 | field(hex, 0..8)?;
    let secs = i64::try_from(ticks / 10_000_000).ok()? - GREGORIAN_TO_UNIX;
    let millis = (ticks % 10_000_000 / 10_000) as u32;
    Some((secs, millis))
}

/// Version 7: the first 48 bits are Unix milliseconds.
fn v7_time(hex: &str) -> Option<(i64, u32)> {
    let ms = field(hex, 0..8)? << 16 | field(hex, 9..13)?;
    Some((i64::try_from(ms / 1000).ok()?, (ms % 1000) as u32))
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    #[test]
    fn random_guid_in_braces() {
        let out = decode(&testdata("uuid_v4_braces.txt"), &env(Lang::En)).expect("v4");
        assert_eq!(
            out,
            "UUID: 6f9619ff-8b86-4011-b42d-00c04fc964ff\nVersion 4 (random)\nVariant: RFC 9562 (RFC 4122)"
        );
        let nl = decode(&testdata("uuid_v4_braces.txt"), &env(Lang::Nl)).expect("v4");
        assert!(nl.contains("Versie 4 (willekeurig)"));
    }

    #[test]
    fn version_1_and_7_times() {
        let v1 = decode(&testdata("uuid_v1.txt"), &env(Lang::En)).expect("v1");
        assert!(v1.contains("Version 1 (time and node)"), "{v1}");
        assert!(v1.contains("Timestamp:\n06/10/2026 10:00:00.123  local time (UTC+02:00)\n06/10/2026 08:00:00.123  UTC\nless than a minute ago"), "{v1}");
        let v7 = decode(&testdata("uuid_v7_urn.txt"), &env(Lang::Nl)).expect("v7");
        assert!(
            v7.starts_with("UUID: 01a1103a-047b-7123-8567-89abcdef0123\nVersie 7 (Unix-tijd)\n"),
            "{v7}"
        );
        assert!(v7.contains("06/10/2026 08:00:00.123  UTC"), "{v7}");
    }

    #[test]
    fn nil_and_other_variants() {
        let nil = decode("00000000-0000-0000-0000-000000000000", &env(Lang::En)).expect("nil");
        assert!(nil.ends_with("Nil UUID (all zeros)"));
        let ms = decode("00000000-0000-0000-c000-000000000046", &env(Lang::En)).expect("ms");
        assert!(ms.contains("Variant: Microsoft (GUID)"), "{ms}");
    }

    #[test]
    fn not_uuids() {
        for line in testdata("uuid_negative.txt").lines() {
            assert_eq!(decode(line, &env(Lang::En)), None, "{line}");
        }
    }
}
