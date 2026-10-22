//! Clean URL (Text ▾ › Clean URL…, and the chip on a link card): tracking parameters out of every
//! `http(s)://` URL in a text.
//!
//! Pure string work: nothing is fetched, no redirect is followed and no short link is expanded.
//! Only the query of a URL that holds a tracking parameter changes; the other parameters, their
//! order and spelling, the fragment (`#…`) and all text around the URLs stay exactly as copied.
//! A query left empty loses its `?`. Nothing here logs the text or a URL.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::ops::Range;

/// Tracking parameters removed by name (ASCII case ignored, after `%xx` decoding). Any name
/// that starts with `utm_` is one too.
pub const TRACKING: [&str; 11] = [
    "fbclid", "gclid", "dclid", "msclkid", "mc_cid", "mc_eid", "igshid", "si", "ref_src", "_hsenc",
    "_hsmi",
];

/// Texts longer than this are not scanned to decide whether the item shows (the card asks on
/// the main thread); the step itself works on any length.
pub const MAX_SCAN: usize = 4 << 20;

/// What cleaning a text did.
#[derive(Clone, PartialEq, Eq)]
pub struct Cleaned {
    /// The text with the tracking parameters removed (the same text when there were none).
    pub text: String,
    /// Tracking parameters removed, over all URLs.
    pub removed: usize,
    /// The URLs that changed, cleaned, in order (for the preview).
    pub changed: Vec<String>,
}

/// URLs are never printed.
impl std::fmt::Debug for Cleaned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cleaned(removed: {}, changed: {})",
            self.removed,
            self.changed.len()
        )
    }
}

/// Is the query pair's name (raw, as in the URL) a tracking parameter?
pub fn is_tracking(raw_name: &str) -> bool {
    let name = crate::link_view::percent_decode(raw_name, true).to_ascii_lowercase();
    name.starts_with("utm_") || TRACKING.contains(&name.as_str())
}

/// Characters that end a URL in running text.
fn ends_url(c: char) -> bool {
    c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'' | '`')
}

/// Punctuation that closes a sentence or a bracket rather than the URL, when it comes last.
fn trailing(c: char) -> bool {
    matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}')
}

/// Byte ranges of the `http://` and `https://` URLs in `text` (scheme in any case) that
/// [`crate::url_policy::may_visit`] accepts: the same rule as Visit.
pub fn url_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("://") {
        let colon = from + found;
        from = colon + 3;
        // Scheme: the letters right before `://`.
        let start = bytes[..colon]
            .iter()
            .rposition(|b| !b.is_ascii_alphabetic())
            .map_or(0, |at| at + 1);
        let scheme = &text[start..colon];
        if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
            continue;
        }
        // Not glued to a word before it (`xhttps://`).
        if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            continue;
        }
        let mut end = text[colon..]
            .find(ends_url)
            .map_or(text.len(), |at| colon + at);
        while let Some(last) = text[..end].chars().next_back() {
            if end <= colon + 3 || !trailing(last) {
                break;
            }
            end -= last.len_utf8();
        }
        if crate::url_policy::may_visit(&text[start..end]) {
            out.push(start..end);
            from = end.max(from);
        }
    }
    out
}

/// The name of a query pair: up to its `=`, or all of it.
fn pair_name(pair: &str) -> &str {
    pair.split_once('=').map_or(pair, |(name, _)| name)
}

/// `url` without its tracking parameters, and how many it had. Unchanged (and 0) without any.
pub fn clean_url(url: &str) -> (String, usize) {
    let (before, fragment) = match url.find('#') {
        Some(at) => url.split_at(at),
        None => (url, ""),
    };
    let Some(question) = before.find('?') else {
        return (url.to_string(), 0);
    };
    let (base, query) = (&before[..question], &before[question + 1..]);
    let removed = query
        .split('&')
        .filter(|pair| !pair.is_empty() && is_tracking(pair_name(pair)))
        .count();
    if removed == 0 {
        return (url.to_string(), 0);
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty() && !is_tracking(pair_name(pair)))
        .collect();
    let mut out = String::with_capacity(url.len());
    out.push_str(base);
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    out.push_str(fragment);
    (out, removed)
}

/// Every URL in `text` without its tracking parameters.
pub fn clean(text: &str) -> Cleaned {
    let mut out = String::with_capacity(text.len());
    let mut removed = 0;
    let mut changed = Vec::new();
    let mut at = 0;
    for range in url_ranges(text) {
        let (cleaned, count) = clean_url(&text[range.clone()]);
        if count == 0 {
            continue;
        }
        out.push_str(&text[at..range.start]);
        out.push_str(&cleaned);
        at = range.end;
        removed += count;
        changed.push(cleaned);
    }
    out.push_str(&text[at..]);
    Cleaned {
        text: out,
        removed,
        changed,
    }
}

/// How many tracking parameters the URLs in `text` hold, without building the cleaned text.
pub fn removable(text: &str) -> usize {
    url_ranges(text)
        .into_iter()
        .map(|range| clean_url(&text[range]).1)
        .sum()
}

/// Whether Clean URL has something to do on `text`: the item and the chip show only then. A
/// text over [`MAX_SCAN`] is not scanned.
pub fn has_tracking(text: &str) -> bool {
    text.len() <= MAX_SCAN && text.contains("://") && removable(text) > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleaned(text: &str) -> String {
        clean(text).text
    }

    #[test]
    fn the_example_keeps_the_other_parameter_and_the_fragment() {
        assert_eq!(
            cleaned("https://x.nl/p?utm_source=x&id=5#top"),
            "https://x.nl/p?id=5#top"
        );
        assert_eq!(clean("https://x.nl/p?utm_source=x&id=5#top").removed, 1);
    }

    #[test]
    fn each_tracking_parameter_is_removed() {
        for name in [
            "utm_source",
            "utm_medium",
            "utm_campaign",
            "utm_term",
            "utm_content",
            "utm_id",
            "utm_anything_else",
            "UTM_Source",
            "utm%5Fsource",
            "fbclid",
            "gclid",
            "dclid",
            "msclkid",
            "mc_cid",
            "mc_eid",
            "igshid",
            "si",
            "ref_src",
            "_hsenc",
            "_hsmi",
            "FBCLID",
        ] {
            let url = format!("https://example.com/a?keep=1&{name}=abc");
            assert_eq!(cleaned(&url), "https://example.com/a?keep=1", "{name}");
            let alone = format!("https://example.com/a?{name}=abc");
            assert_eq!(cleaned(&alone), "https://example.com/a", "{name}");
            let bare = format!("https://example.com/a?{name}&keep=1");
            assert_eq!(cleaned(&bare), "https://example.com/a?keep=1", "{name}");
        }
    }

    #[test]
    fn other_parameters_stay_in_their_order_and_spelling() {
        assert_eq!(
            cleaned("https://e.com/s?q=a+b&utm_medium=mail&page=2&x=%20y&utm_source=z"),
            "https://e.com/s?q=a+b&page=2&x=%20y"
        );
        // Look-alikes are not tracking parameters.
        let keep = "https://e.com/?utm=1&sid=2&fbclid_x=3&mysi=4&ref=5&source=6";
        assert_eq!(cleaned(keep), keep);
        assert!(!has_tracking(keep));
    }

    #[test]
    fn with_a_fragment() {
        assert_eq!(
            cleaned("https://e.com/doc?gclid=1#section-2"),
            "https://e.com/doc#section-2"
        );
        assert_eq!(
            cleaned("https://e.com/doc?a=1&fbclid=1#x?utm_source=kept"),
            "https://e.com/doc?a=1#x?utm_source=kept"
        );
        // A `?` only in the fragment is not a query.
        let fragment_only = "https://e.com/doc#x?utm_source=y";
        assert_eq!(cleaned(fragment_only), fragment_only);
    }

    #[test]
    fn several_urls_in_one_text() {
        let text = "Zie https://a.nl/x?utm_source=nb&id=1 en\n\
                    (http://B.example/y?si=abc&t=30), of https://c.org/z?q=1.\n\
                    HTTPS://d.io/?fbclid=1&gclid=2#top!";
        let done = clean(text);
        assert_eq!(
            done.text,
            "Zie https://a.nl/x?id=1 en\n\
             (http://B.example/y?t=30), of https://c.org/z?q=1.\n\
             HTTPS://d.io/#top!"
        );
        assert_eq!(done.removed, 4);
        assert_eq!(removable(text), 4);
        assert_eq!(
            done.changed,
            [
                "https://a.nl/x?id=1",
                "http://B.example/y?t=30",
                "HTTPS://d.io/#top"
            ]
        );
    }

    #[test]
    fn nothing_to_remove_leaves_the_text_identical() {
        for text in [
            "",
            "no links here",
            "https://example.com/a?b=1#c",
            "https://example.com/?",
            "ftp://e.com/?utm_source=x",
            "xhttps://e.com/?utm_source=x",
            "utm_source=x&fbclid=y",
            "https://?utm_source=x",
        ] {
            let done = clean(text);
            assert_eq!(done.text, text, "{text}");
            assert_eq!(done.removed, 0, "{text}");
            assert!(!has_tracking(text), "{text}");
        }
    }

    #[test]
    fn empty_pairs_go_only_when_the_query_is_rebuilt() {
        assert_eq!(
            cleaned("https://e.com/?a=1&&utm_id=2&"),
            "https://e.com/?a=1"
        );
        assert_eq!(cleaned("https://e.com/?&utm_id=2"), "https://e.com/");
    }

    #[test]
    fn the_debug_form_shows_no_url() {
        let done = clean("https://secret.example/?utm_source=x&token=abc");
        let shown = format!("{done:?}");
        assert!(
            !shown.contains("secret") && !shown.contains("abc"),
            "{shown}"
        );
    }
}
