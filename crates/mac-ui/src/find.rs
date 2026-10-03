//! Find in text: case-insensitive, non-overlapping matches (byte ranges), a "3/12" counter and
//! wrapping steps between matches. Plain Rust; `widgets::mark_ranges` (feature `widgets`) paints
//! them in a text view.

use std::ops::Range;

/// Case-insensitive, non-overlapping spans in `text`. Indexes are bytes.
pub fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.len() < needle.len() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut index = 0;
    while index + needle.len() <= chars.len() {
        let hit = needle
            .iter()
            .enumerate()
            .all(|(offset, expected)| chars_equal_ignore_case(chars[index + offset].1, *expected));
        if hit {
            let start = chars[index].0;
            let end_index = index + needle.len();
            let end = chars
                .get(end_index)
                .map(|(byte, _)| *byte)
                .unwrap_or(text.len());
            matches.push(start..end);
            index = end_index;
        } else {
            index += 1;
        }
    }
    matches
}

fn chars_equal_ignore_case(left: char, right: char) -> bool {
    if left == right || left.eq_ignore_ascii_case(&right) {
        return true;
    }
    let mut left_chars = left.to_lowercase();
    let mut right_chars = right.to_lowercase();
    loop {
        match (left_chars.next(), right_chars.next()) {
            (Some(a), Some(b)) if a == b => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

/// `index` is zero-based. `3/12` is the third match of twelve.
pub fn match_counter(index: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", index + 1, total)
    }
}

/// [`match_counter`], or nothing while there is no query.
pub fn match_label(query: &str, index: usize, total: usize) -> String {
    if query.is_empty() {
        String::new()
    } else {
        match_counter(index, total)
    }
}

/// The match after (`forward`) or before `index`, wrapping around; 0 without matches.
pub fn step_match(index: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (index + 1) % total
    } else {
        (index + total - 1) % total
    }
}

#[cfg(test)]
mod tests {
    use super::{find_matches, match_counter, match_label, step_match};

    #[test]
    fn match_counter_is_one_based() {
        assert_eq!(match_counter(2, 12), "3/12");
        assert_eq!(match_counter(0, 1), "1/1");
        assert_eq!(match_counter(0, 0), "0/0");
        assert_eq!(match_label("", 0, 4), "");
        assert_eq!(match_label("a", 0, 0), "0/0");
    }

    #[test]
    fn step_match_wraps() {
        assert_eq!(step_match(0, 12, true), 1);
        assert_eq!(step_match(11, 12, true), 0);
        assert_eq!(step_match(0, 12, false), 11);
        assert_eq!(step_match(3, 12, false), 2);
        assert_eq!(step_match(0, 1, true), 0);
        assert_eq!(step_match(0, 0, true), 0);
        assert_eq!(step_match(4, 0, false), 0);
    }

    #[test]
    fn find_matches_are_case_insensitive_and_nonoverlapping() {
        let text = "Alpha alpha ALPHA";
        let hits = find_matches(text, "alpha");
        assert_eq!(hits.len(), 3);
        assert_eq!(&text[hits[0].start..hits[0].end], "Alpha");
        assert_eq!(&text[hits[1].start..hits[1].end], "alpha");
        assert_eq!(match_counter(0, hits.len()), "1/3");
        assert_eq!(step_match(0, hits.len(), true), 1);
        assert_eq!(step_match(0, hits.len(), false), 2);
        assert!(find_matches(text, "").is_empty());
        assert_eq!(find_matches("aaaa", "aa"), vec![0..2, 2..4]);
        let accented = "héllo héllo";
        let marks = find_matches(accented, "Héllo");
        assert_eq!(marks.len(), 2);
        assert_eq!(&accented[marks[0].start..marks[0].end], "héllo");
    }
}
