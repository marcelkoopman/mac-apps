//! Hashes, recognised only by hex digits and length: 32 MD5, 40 SHA-1, 56 SHA-224, 64 SHA-256,
//! 96 SHA-384, 128 SHA-512. Never reversed or looked up: the view only names the likely
//! algorithm. 32 hex digits may also be a UUID without dashes, and the view says so.

use super::clock::Env;

/// The decoded view, or `None` when `text` is not a hex digest of a known length.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let bytes = text.as_bytes();
    if !bytes.iter().all(u8::is_ascii_hexdigit)
        // A digest has letters and digits; one case only.
        || !bytes.iter().any(u8::is_ascii_digit)
        || !bytes.iter().any(u8::is_ascii_alphabetic)
        || (bytes.iter().any(u8::is_ascii_lowercase) && bytes.iter().any(u8::is_ascii_uppercase))
    {
        return None;
    }
    let algorithm = match bytes.len() {
        32 => "MD5",
        40 => "SHA-1",
        56 => "SHA-224",
        64 => "SHA-256",
        96 => "SHA-384",
        128 => "SHA-512",
        _ => return None,
    };
    let bits = bytes.len() * 4;
    let mut lines = vec![
        env.pick(
            &format!("Waarschijnlijk {algorithm}"),
            &format!("Probably {algorithm}"),
        )
        .to_string(),
        env.pick(
            &format!("{bits} bits, {} hex-tekens", bytes.len()),
            &format!("{bits} bits, {} hex characters", bytes.len()),
        )
        .to_string(),
    ];
    if bytes.len() == 32 {
        lines.push(
            env.pick("Of een UUID zonder streepjes", "Or a UUID without dashes")
                .to_string(),
        );
    }
    lines.push(String::new());
    lines.push(
        env.pick(
            "Alleen herkend aan lengte en tekens. Een hash is niet terug te rekenen.",
            "Recognised by length and characters only. A hash cannot be reversed.",
        )
        .to_string(),
    );
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    #[test]
    fn lengths_name_the_algorithm() {
        for (file, nl, en) in [
            ("hash_md5.txt", "Waarschijnlijk MD5", "Probably MD5"),
            ("hash_sha1.txt", "Waarschijnlijk SHA-1", "Probably SHA-1"),
            (
                "hash_sha224.txt",
                "Waarschijnlijk SHA-224",
                "Probably SHA-224",
            ),
            (
                "hash_sha256.txt",
                "Waarschijnlijk SHA-256",
                "Probably SHA-256",
            ),
            (
                "hash_sha384.txt",
                "Waarschijnlijk SHA-384",
                "Probably SHA-384",
            ),
            (
                "hash_sha512.txt",
                "Waarschijnlijk SHA-512",
                "Probably SHA-512",
            ),
        ] {
            let text = testdata(file);
            let out = decode(&text, &env(Lang::En)).expect(file);
            assert!(out.starts_with(&format!("{en}\n")), "{file}: {out}");
            assert!(out.ends_with("A hash cannot be reversed."));
            let out = decode(&text, &env(Lang::Nl)).expect(file);
            assert!(out.starts_with(&format!("{nl}\n")), "{file}: {out}");
        }
        let md5 = decode(&testdata("hash_md5.txt"), &env(Lang::En)).expect("md5");
        assert!(md5.contains("128 bits, 32 hex characters\nOr a UUID without dashes"));
    }

    #[test]
    fn other_lengths_and_characters() {
        for line in testdata("hash_negative.txt").lines() {
            assert_eq!(decode(line, &env(Lang::En)), None, "{line}");
        }
        // Mixed case is not a digest.
        assert_eq!(
            decode("32110a6be834984e423beab864afdeE9", &env(Lang::En)),
            None
        );
    }
}
