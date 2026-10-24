//! Hashes in Text ▾ › Convert › Encoding: SHA-256, SHA-1 and MD5 of the whole text as lowercase
//! hex, worked out in Rust (RustCrypto `sha2`, `sha1`, `md-5`; no network).
//!
//! The hash is of the text's UTF-8 bytes exactly as copied, a final newline included, like
//! encoding. MD5 and SHA-1 are broken: their items say "not for security". A hash of a
//! sensitive card stays out of history (`App::finish_text`). Neither the text nor its hash
//! is ever logged.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use sha2::Digest as _;

/// One hash of the Encoding submenu, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    Sha256,
    Sha1,
    Md5,
}

impl HashKind {
    pub const ALL: [HashKind; 3] = [HashKind::Sha256, HashKind::Sha1, HashKind::Md5];

    /// Its menu item; MD5 and SHA-1 say they are not for security.
    pub fn title(self) -> &'static str {
        match self {
            HashKind::Sha256 => crate::locale::t("hash_sha256"),
            HashKind::Sha1 => crate::locale::t("hash_sha1"),
            HashKind::Md5 => crate::locale::t("hash_md5"),
        }
    }

    /// The menu item's detail line.
    pub fn detail(self) -> &'static str {
        if self.weak() {
            crate::locale::t("detail_hash_weak")
        } else {
            crate::locale::t("detail_hash")
        }
    }

    /// MD5 and SHA-1: collisions can be made, so not for security.
    pub fn weak(self) -> bool {
        matches!(self, HashKind::Sha1 | HashKind::Md5)
    }

    /// The hash of `text`'s UTF-8 bytes as lowercase hex.
    pub fn hex(self, text: &str) -> String {
        let bytes = text.as_bytes();
        match self {
            HashKind::Sha256 => super::encoding::hex_encode(&sha2::Sha256::digest(bytes)),
            HashKind::Sha1 => super::encoding::hex_encode(&sha1::Sha1::digest(bytes)),
            HashKind::Md5 => super::encoding::hex_encode(&md5::Md5::digest(bytes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HashKind;

    /// FIPS 180-2 / RFC 3174 / RFC 1321 test vectors: "abc" and the empty string.
    #[test]
    fn known_vectors() {
        let cases = [
            (
                HashKind::Sha256,
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                HashKind::Sha256,
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                HashKind::Sha1,
                "",
                "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            ),
            (
                HashKind::Sha1,
                "abc",
                "a9993e364706816aba3e25717850c26c9cd0d89d",
            ),
            (HashKind::Md5, "", "d41d8cd98f00b204e9800998ecf8427e"),
            (HashKind::Md5, "abc", "900150983cd24fb0d6963f7d28e17f72"),
        ];
        for (kind, text, hex) in cases {
            assert_eq!(kind.hex(text), hex, "{kind:?} of {text:?}");
        }
    }

    #[test]
    fn hashes_the_utf8_bytes_as_copied() {
        // A final newline is part of the text.
        assert_ne!(HashKind::Sha256.hex("abc\n"), HashKind::Sha256.hex("abc"));
        assert_eq!(
            HashKind::Md5.hex("é"),
            "66ddcd97cfdeabb2f6fb8a999b4bc76f",
            "UTF-8, not Latin-1"
        );
        for kind in HashKind::ALL {
            let hex = kind.hex("abc");
            assert!(
                hex.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            );
        }
    }

    #[test]
    fn only_sha256_is_for_security() {
        assert!(!HashKind::Sha256.weak());
        assert!(HashKind::Sha1.weak());
        assert!(HashKind::Md5.weak());
        assert_ne!(HashKind::Sha256.detail(), HashKind::Md5.detail());
        assert_eq!(HashKind::Sha1.detail(), HashKind::Md5.detail());
    }
}
