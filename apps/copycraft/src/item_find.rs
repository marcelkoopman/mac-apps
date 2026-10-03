//! ⌘F in the card well, without copying the item's text for each keystroke. The item is folded
//! for case-insensitive search once ([`fold`]), kept with the item and zeroized with it; each
//! query is a `memmem` scan of that buffer. Matches are byte offsets, which [`painted`] turns
//! into UTF-16 ranges for AppKit, at most [`MAX_PAINTED`] of them around the current match.

use std::time::Duration;

use zeroize::{Zeroize, Zeroizing};

/// Matches painted in the well at once, around the current one. The counter still counts all.
pub const MAX_PAINTED: usize = 1000;
/// Items at least this long search after a pause in typing ([`DEBOUNCE`]), not per keystroke.
pub const DEBOUNCE_FROM: usize = 200 * 1024;
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// `text` folded for case-insensitive search. A character is lowercased when its lowercase is
/// one character of the same UTF-8 length (`É` → `é`), so every byte offset is the same in the
/// folded buffer and in `text`; the few others (`İ`) only match as typed.
pub fn fold(text: &str) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::with_capacity(text.len()));
    let mut buf = [0u8; 4];
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch.to_ascii_lowercase() as u8);
            continue;
        }
        let mut lower = ch.to_lowercase();
        let folded = match (lower.next(), lower.next()) {
            (Some(one), None) if one.len_utf8() == ch.len_utf8() => one,
            _ => ch,
        };
        out.extend_from_slice(folded.encode_utf8(&mut buf).as_bytes());
    }
    buf.zeroize();
    out
}

/// Non-overlapping matches of a query in a folded item.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Matches {
    /// Byte offsets, ascending.
    pub starts: Vec<usize>,
    /// Byte length of every match (the folded query's).
    pub len: usize,
}

impl Matches {
    pub fn total(&self) -> usize {
        self.starts.len()
    }
}

/// Where the folded `query` occurs in `folded`. An empty query finds nothing.
pub fn find(folded: &[u8], query: &str) -> Matches {
    let needle = fold(query);
    if needle.is_empty() {
        return Matches::default();
    }
    Matches {
        starts: memchr::memmem::find_iter(folded, needle.as_slice()).collect(),
        len: needle.len(),
    }
}

/// UTF-16 length of `bytes`, which is valid UTF-8: one unit per character, two for one outside
/// the Basic Multilingual Plane (a four-byte sequence).
fn utf16_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|&byte| usize::from(byte & 0xC0 != 0x80) + usize::from(byte >= 0xF0))
        .sum()
}

/// The matches to paint, as UTF-16 `(location, length)`: at most [`MAX_PAINTED`], in a window
/// around `current`, and where `current` is among them. `folded` has the item's byte layout,
/// and its UTF-16 lengths are the item's too.
pub fn painted(folded: &[u8], matches: &Matches, current: usize) -> (Vec<(usize, usize)>, usize) {
    let total = matches.total();
    if total == 0 {
        return (Vec::new(), 0);
    }
    let current = current.min(total - 1);
    let first = current
        .saturating_sub(MAX_PAINTED / 2)
        .min(total.saturating_sub(MAX_PAINTED));
    let last = (first + MAX_PAINTED).min(total);
    let mut ranges = Vec::with_capacity(last - first);
    let (mut byte, mut unit) = (0, 0);
    for &start in &matches.starts[first..last] {
        unit += utf16_len(&folded[byte..start]);
        byte = start;
        let end = (start + matches.len).min(folded.len());
        ranges.push((unit, utf16_len(&folded[start..end])));
    }
    (ranges, current - first)
}

#[cfg(test)]
mod tests {
    use super::{MAX_PAINTED, find, fold, painted};

    #[test]
    fn folding_keeps_byte_offsets() {
        for text in ["Hello WORLD", "Héllo ÉCOLE", "İstanbul ẞ", "emoji 😀 OK"] {
            assert_eq!(fold(text).len(), text.len(), "{text}");
        }
        assert_eq!(fold("ÉCOLE Ab").as_slice(), "école ab".as_bytes());
    }

    #[test]
    fn finds_case_insensitive_non_overlapping_matches() {
        let text = "Alpha beta ALPHA gamma alpha";
        let found = find(&fold(text), "alpha");
        assert_eq!(found.starts, vec![0, 11, 23]);
        assert_eq!(found.len, 5);
        assert_eq!(find(&fold("aaaa"), "aa").starts, vec![0, 2]);
        assert_eq!(find(&fold("Héllo HÉLLO"), "héllo").total(), 2);
        assert_eq!(find(&fold(text), "").total(), 0);
        assert_eq!(find(&fold(""), "a").total(), 0);
    }

    #[test]
    fn painted_ranges_are_utf16() {
        let text = "😀 ab é ab";
        let folded = fold(text);
        let found = find(&folded, "AB");
        let (ranges, current) = painted(&folded, &found, 1);
        // 😀 is two UTF-16 units, the space one: "ab" starts at 3; then " é " to 8.
        assert_eq!(ranges, vec![(3, 2), (8, 2)]);
        assert_eq!(current, 1);
        let expected: Vec<(usize, usize)> = found
            .starts
            .iter()
            .map(|&start| {
                (
                    text[..start].encode_utf16().count(),
                    text[start..start + found.len].encode_utf16().count(),
                )
            })
            .collect();
        assert_eq!(ranges, expected);
    }

    #[test]
    fn paints_at_most_a_window_around_the_current_match() {
        let text = "x ".repeat(5000);
        let folded = fold(&text);
        let found = find(&folded, "x");
        assert_eq!(found.total(), 5000);
        let (ranges, current) = painted(&folded, &found, 4000);
        assert_eq!(ranges.len(), MAX_PAINTED);
        assert_eq!(ranges[current], (8000, 1));
        let (ranges, current) = painted(&folded, &found, 4999);
        assert_eq!(ranges.len(), MAX_PAINTED);
        assert_eq!(ranges[current], (9998, 1));
        let (_, current) = painted(&folded, &found, 3);
        assert_eq!(current, 3);
    }
}
