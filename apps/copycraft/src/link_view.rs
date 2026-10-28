//! A URL split into decoded parts, and the spans the well styles (scheme dim, host bold, path
//! dim). The Link card itself is [`crate::link_card`]. String work only: nothing is fetched, and
//! nothing here logs the text.

/// A URL split into parts, each decoded for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkParts {
    pub scheme: Option<String>,
    pub host: String,
    pub path: String,
    /// `(name, value)` in order; repeated names stay repeated. A pair without `=` has an empty
    /// value.
    pub query: Vec<(String, String)>,
    pub fragment: Option<String>,
}

/// `%xx` to bytes (a `%` not followed by two hex digits stays as written), `+` to a space with
/// `plus`. Bytes that are not UTF-8 show as U+FFFD.
pub(crate) fn percent_decode(text: &str, plus: bool) -> String {
    let bytes = text.as_bytes();
    let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if hex(i + 1).is_some() && hex(i + 2).is_some() => {
                out.push((hex(i + 1).unwrap_or(0) * 16 + hex(i + 2).unwrap_or(0)) as u8);
                i += 3;
            }
            b'+' if plus => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Split `text` (trimmed, no white space inside). `None` when it is not a URL this can read:
/// a scheme that is not one, no host, or white space.
pub(crate) fn split_url(text: &str) -> Option<LinkParts> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) if is_scheme(scheme) => (Some(scheme), rest),
        Some(_) => return None,
        None => (None, text),
    };
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query)),
        None => (rest, None),
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    // Never the `user:password@` part.
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if host.is_empty() || host.starts_with(':') {
        return None;
    }
    let query = query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((name, value)) => (percent_decode(name, true), percent_decode(value, true)),
            None => (percent_decode(pair, true), String::new()),
        })
        .collect();
    Some(LinkParts {
        scheme: scheme.map(str::to_string),
        host: host.to_string(),
        path: percent_decode(path, false),
        query,
        fragment: fragment.map(|f| percent_decode(f, false)),
    })
}

/// RFC 3986: a letter, then letters, digits, `+`, `-`, `.`.
fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Where the parts of a URL sit in the text shown for it, as byte ranges: `lead` is everything
/// before the host (scheme, `://`, and any `user@` that is still there), `host` the host and
/// port, `rest` the path, query and fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UrlSpans {
    pub lead: std::ops::Range<usize>,
    pub host: std::ops::Range<usize>,
    pub rest: std::ops::Range<usize>,
}

/// The spans of `url` (one line, as shown) for the card's styling. `None` when [`split_url`]
/// cannot read it: invalid URLs are not styled. String work only; nothing is logged.
pub(crate) fn url_spans(url: &str) -> Option<UrlSpans> {
    split_url(url)?;
    // `split_url` trims; spans are of the text as given, so skip the same white space.
    let start = url.len() - url.trim_start().len();
    let end = url.trim_end().len();
    let body = &url[start..end];
    let after_scheme = body.find("://").map_or(0, |at| at + 3);
    let authority_end = body[after_scheme..]
        .find(['/', '?', '#'])
        .map_or(body.len(), |at| after_scheme + at);
    let authority = &body[after_scheme..authority_end];
    let host_start = after_scheme + authority.rfind('@').map_or(0, |at| at + 1);
    Some(UrlSpans {
        lead: start..start + host_start,
        host: start + host_start..start + authority_end,
        rest: start + authority_end..end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans_of(url: &str) -> (&str, &str, &str) {
        let spans = url_spans(url).unwrap();
        (&url[spans.lead], &url[spans.host], &url[spans.rest])
    }

    #[test]
    fn spans_split_scheme_host_and_rest() {
        assert_eq!(
            spans_of("https://example.com:8080/a/b?x=1#top"),
            ("https://", "example.com:8080", "/a/b?x=1#top")
        );
        assert_eq!(
            spans_of("https://example.com"),
            ("https://", "example.com", "")
        );
        assert_eq!(spans_of("https://e.com?q=1"), ("https://", "e.com", "?q=1"));
        assert_eq!(spans_of("www.example.com/x"), ("", "www.example.com", "/x"));
    }

    #[test]
    fn spans_keep_userinfo_out_of_the_host() {
        assert_eq!(
            spans_of("https://user:secret@example.com/x"),
            ("https://user:secret@", "example.com", "/x")
        );
        assert_eq!(
            spans_of("https://a@b@example.com/x"),
            ("https://a@b@", "example.com", "/x")
        );
        // An `@` in the path is not userinfo.
        assert_eq!(
            spans_of("https://example.com/@me"),
            ("https://", "example.com", "/@me")
        );
    }

    #[test]
    fn spans_use_byte_ranges_for_non_ascii() {
        let url = "https://bücher.example/café?naïve=ü…";
        let (lead, host, rest) = spans_of(url);
        assert_eq!(
            (lead, host, rest),
            ("https://", "bücher.example", "/café?naïve=ü…")
        );
        assert_eq!(url_spans(url).unwrap().rest.end, url.len());
    }

    #[test]
    fn spans_are_none_for_invalid_urls() {
        for text in [
            "",
            "not a url",
            "ht tp://x.com",
            "1x://e.com",
            "https://",
            "https://:80/x",
        ] {
            assert_eq!(url_spans(text), None, "{text}");
        }
    }

    fn pairs(parts: &LinkParts) -> Vec<(&str, &str)> {
        parts
            .query
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect()
    }

    #[test]
    fn splits_all_parts() {
        let p = split_url("https://example.com:8443/a%20b/c?q=1&r=2#top").unwrap();
        assert_eq!(p.scheme.as_deref(), Some("https"));
        assert_eq!(p.host, "example.com:8443");
        assert_eq!(p.path, "/a b/c");
        assert_eq!(pairs(&p), vec![("q", "1"), ("r", "2")]);
        assert_eq!(p.fragment.as_deref(), Some("top"));
    }

    #[test]
    fn percent_encoding_and_plus() {
        let p = split_url("https://e.com/?q=rode+fiets&c=Den%20Haag&u=caf%C3%A9&k%20ey=%E2%82%AC")
            .unwrap();
        assert_eq!(
            pairs(&p),
            vec![
                ("q", "rode fiets"),
                ("c", "Den Haag"),
                ("u", "café"),
                ("k ey", "€")
            ]
        );
        // `+` is only a space in the query.
        assert_eq!(split_url("https://e.com/a+b").unwrap().path, "/a+b");
    }

    #[test]
    fn invalid_escapes_stay_as_written() {
        assert_eq!(percent_decode("50%", false), "50%");
        assert_eq!(percent_decode("%zz%4", false), "%zz%4");
        assert_eq!(percent_decode("100%25", false), "100%");
        // Not UTF-8: replacement character, no panic.
        assert_eq!(percent_decode("%FF%FE", false), "\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn no_query() {
        let p = split_url("https://example.com/path").unwrap();
        assert!(p.query.is_empty());
        assert_eq!(p.fragment, None);
        let bare = split_url("example.com").unwrap();
        assert_eq!(
            (bare.scheme, bare.host.as_str(), bare.path.as_str()),
            (None, "example.com", "")
        );
    }

    #[test]
    fn fragment_and_query_order() {
        let p = split_url("https://e.com/p#frag?x=1").unwrap();
        assert!(p.query.is_empty());
        assert_eq!(p.fragment.as_deref(), Some("frag?x=1"));
        let p = split_url("https://e.com/p?a=1#f%41").unwrap();
        assert_eq!(p.fragment.as_deref(), Some("fA"));
    }

    #[test]
    fn repeated_keys_empty_values_and_flags() {
        let p = split_url("https://e.com/?t=a&t=b&empty=&flag&&=x&eq=a=b").unwrap();
        assert_eq!(
            pairs(&p),
            vec![
                ("t", "a"),
                ("t", "b"),
                ("empty", ""),
                ("flag", ""),
                ("", "x"),
                ("eq", "a=b")
            ]
        );
    }

    #[test]
    fn invalid_urls() {
        for text in [
            "",
            "   ",
            "has space.com/x",
            "1ttp://x",
            "://x",
            "https://",
            "https:///p?q=1",
            "a b://c.d",
            "https://:80/",
        ] {
            assert_eq!(split_url(text), None, "{text}");
        }
    }
}
