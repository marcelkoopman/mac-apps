//! Checksums of a dropped or chosen file: SHA-256 and MD5, read in chunks (never the whole
//! file in memory), on a worker thread ([`crate::menubar`]). *Compare with Clipboard* checks a
//! hex hash on the clipboard against them. Nothing here is logged: no name, path or hash.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

use md5::Digest as _;

/// Bytes read at a time.
const CHUNK: usize = 1 << 20;
/// Files larger than this show their progress while they are read.
pub const SHOW_PROGRESS_ABOVE: u64 = 1 << 30;

/// SHA-256 and MD5 of one file, lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digests {
    pub sha256: String,
    pub md5: String,
}

/// Why no digests came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    /// A newer drop took over.
    Cancelled,
    /// The file could not be read.
    Unreadable,
}

/// Read `reader` to its end in chunks and hash it. `progress` gets the bytes read so far after
/// every chunk; `cancel` is checked before each one.
pub fn digest(
    mut reader: impl Read,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<Digests, Failed> {
    let mut sha256 = sha2::Sha256::new();
    let mut md5 = md5::Md5::new();
    let mut buffer = zeroize::Zeroizing::new(vec![0u8; CHUNK]);
    let mut read = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failed::Cancelled);
        }
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Failed::Unreadable),
        };
        sha256.update(&buffer[..count]);
        md5.update(&buffer[..count]);
        read += count as u64;
        progress(read);
    }
    Ok(Digests {
        sha256: crate::text_ops::encoding::hex_encode(&sha256.finalize()),
        md5: crate::text_ops::encoding::hex_encode(&md5.finalize()),
    })
}

/// The file at `path`, hashed by [`digest`]. Only a regular file.
pub fn digest_file(
    path: &std::path::Path,
    cancel: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<Digests, Failed> {
    let file = std::fs::File::open(path).map_err(|_| Failed::Unreadable)?;
    let is_file = file.metadata().is_ok_and(|meta| meta.is_file());
    if !is_file {
        return Err(Failed::Unreadable);
    }
    digest(file, cancel, progress)
}

/// `text` as a hex hash: lowercase, without spaces (also those between groups, line breaks and
/// tabs), 32 to 128 hex digits of an even count. `None` for anything else.
pub fn clipboard_hash(text: &str) -> Option<String> {
    let hex: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    let hex = hex
        .strip_prefix("sha256:")
        .or_else(|| hex.strip_prefix("md5:"))
        .unwrap_or(&hex)
        .to_string();
    (hex.len() >= 32
        && hex.len() <= 128
        && hex.len().is_multiple_of(2)
        && hex.chars().all(|ch| ch.is_ascii_hexdigit()))
    .then_some(hex)
}

/// What the clipboard's text says about `digests`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The clipboard holds this file's SHA-256.
    Sha256,
    /// The clipboard holds this file's MD5.
    Md5,
    /// A hex hash, but not this file's.
    Differs,
    /// No hex hash on the clipboard.
    NoHash,
}

/// Compare the clipboard's text (`clipboard`) with `digests`: case and spaces do not count.
pub fn compare(clipboard: Option<&str>, digests: &Digests) -> Verdict {
    let Some(hash) = clipboard.and_then(clipboard_hash) else {
        return Verdict::NoHash;
    };
    if hash == digests.sha256 {
        Verdict::Sha256
    } else if hash == digests.md5 {
        Verdict::Md5
    } else {
        Verdict::Differs
    }
}

/// Percent of `total` read (`done`), 0 to 100.
pub fn percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 100;
    }
    (done.min(total).saturating_mul(100) / total) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digests_of(bytes: &[u8]) -> Digests {
        digest(bytes, &AtomicBool::new(false), |_| {}).unwrap()
    }

    #[test]
    fn known_digests() {
        let empty = digests_of(b"");
        assert_eq!(
            empty.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(empty.md5, "d41d8cd98f00b204e9800998ecf8427e");
        let abc = digests_of(b"abc");
        assert_eq!(
            abc.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(abc.md5, "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn chunks_give_the_same_digest_and_report_progress() {
        // Three chunks and a bit, of made-up bytes.
        let bytes: Vec<u8> = (0..CHUNK * 3 + 17).map(|at| (at % 251) as u8).collect();
        let mut seen = Vec::new();
        let chunked = digest(&bytes[..], &AtomicBool::new(false), |done| seen.push(done)).unwrap();
        let whole = Digests {
            sha256: crate::text_ops::encoding::hex_encode(&sha2::Sha256::digest(&bytes)),
            md5: crate::text_ops::encoding::hex_encode(&md5::Md5::digest(&bytes)),
        };
        assert_eq!(chunked, whole);
        assert_eq!(seen.last().copied(), Some(bytes.len() as u64));
        assert!(seen.len() >= 4);
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_cancelled_read_gives_nothing() {
        assert_eq!(
            digest(&b"abc"[..], &AtomicBool::new(true), |_| {}),
            Err(Failed::Cancelled)
        );
    }

    #[test]
    fn a_file_is_read_and_a_folder_is_not() {
        let dir = std::env::temp_dir().join(format!("copycraft-checksum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("made-up.bin");
        std::fs::write(&file, b"abc").unwrap();
        let never = AtomicBool::new(false);
        assert_eq!(
            digest_file(&file, &never, |_| {}).unwrap().md5,
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(digest_file(&dir, &never, |_| {}), Err(Failed::Unreadable));
        assert_eq!(
            digest_file(&dir.join("missing"), &never, |_| {}),
            Err(Failed::Unreadable)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_clipboard_hash_ignores_case_and_spaces() {
        let abc = digests_of(b"abc");
        let upper = abc.sha256.to_uppercase();
        assert_eq!(compare(Some(&upper), &abc), Verdict::Sha256);
        let spaced: String = abc
            .sha256
            .as_bytes()
            .chunks(8)
            .map(|group| std::str::from_utf8(group).unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            compare(Some(&format!("  {spaced}\n")), &abc),
            Verdict::Sha256
        );
        assert_eq!(compare(Some(&abc.md5), &abc), Verdict::Md5);
        assert_eq!(
            compare(Some(&format!("sha256:{}", abc.sha256)), &abc),
            Verdict::Sha256
        );
        let other = digests_of(b"abd");
        assert_eq!(compare(Some(&other.sha256), &abc), Verdict::Differs);
        assert_eq!(compare(Some(&other.md5), &abc), Verdict::Differs);
    }

    #[test]
    fn only_a_hex_hash_counts() {
        for text in [
            "",
            "hello",
            "abc",
            "0123456789abcdef",
            "zz".repeat(16).as_str(),
        ] {
            assert_eq!(clipboard_hash(text), None, "{text}");
        }
        // An odd count of hex digits, or too long.
        assert_eq!(clipboard_hash(&"a".repeat(33)), None);
        assert_eq!(clipboard_hash(&"a".repeat(130)), None);
        assert_eq!(compare(None, &digests_of(b"abc")), Verdict::NoHash);
        assert_eq!(
            compare(Some("not a hash"), &digests_of(b"abc")),
            Verdict::NoHash
        );
    }

    #[test]
    fn percent_read() {
        assert_eq!(percent(0, 200), 0);
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(300, 200), 100);
        assert_eq!(percent(0, 0), 100);
        const { assert!(SHOW_PROGRESS_ABOVE == 1024 * 1024 * 1024) };
    }
}
