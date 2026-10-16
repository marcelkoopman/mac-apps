//! The Link card's text: the whole URL (the well wraps it) and its query parameters as a small
//! `name │ value` table, decoded for display. String work only: nothing is fetched, and nothing
//! here logs the text.

/// Rows of the parameter table before "… and N more".
const MAX_ROWS: usize = 40;
/// Characters of a parameter name or value shown; the rest is cut with `…`.
const MAX_NAME: usize = 24;
const MAX_VALUE: usize = 200;
/// Characters of the URL itself shown.
const MAX_URL: usize = 4000;

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

/// At most `max` characters of `text` on one line (control characters and line breaks become
/// spaces), cut with `…`.
fn one_line(text: &str, max: usize) -> String {
    let mut out: String = text
        .chars()
        .take(max)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if text.chars().nth(max).is_some() {
        out.push('…');
    }
    out
}

/// The card text for `text`: the full URL, then (with a query) a blank line, a heading and one
/// `name │ value` row per parameter. `None` when `text` is not a URL: the card then shows it as
/// before.
pub(crate) fn card_text(text: &str) -> Option<String> {
    let url = text.trim();
    let parts = split_url(url)?;
    let shown = crate::page_preview::strip_userinfo(url);
    let mut out = one_line(&shown, MAX_URL);
    if parts.query.is_empty() {
        return Some(out);
    }
    let name_width = parts
        .query
        .iter()
        .take(MAX_ROWS)
        .map(|(name, _)| one_line(name, MAX_NAME).chars().count())
        .max()
        .unwrap_or(0);
    out.push_str("\n\n");
    out.push_str(crate::locale::t("link_query_heading"));
    for (name, value) in parts.query.iter().take(MAX_ROWS) {
        let name = one_line(name, MAX_NAME);
        let pad = name_width.saturating_sub(name.chars().count());
        out.push('\n');
        out.push_str(&name);
        out.push_str(&" ".repeat(pad));
        out.push_str(" │ ");
        out.push_str(&one_line(value, MAX_VALUE));
    }
    if parts.query.len() > MAX_ROWS {
        out.push('\n');
        out.push_str(&crate::locale::tf(
            "link_query_more",
            &[&(parts.query.len() - MAX_ROWS)],
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            card_text("https://example.com/path").unwrap(),
            "https://example.com/path"
        );
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
            assert_eq!(card_text(text), None, "{text}");
        }
    }

    #[test]
    fn userinfo_is_not_shown() {
        let text = card_text("https://user:secret@example.com/x?a=1").unwrap();
        assert!(!text.contains("secret"), "{text}");
        assert!(text.starts_with("https://example.com/x?a=1"));
    }

    #[test]
    fn long_parameters_are_clipped_to_one_line_each() {
        let long = "x".repeat(5000);
        let url = format!("https://e.com/?{long}=b&v={long}&nl=a%0Ab%0D");
        let text = card_text(&url).unwrap();
        for line in text.lines().skip(2) {
            assert!(
                line.chars().count() <= MAX_NAME + 1 + 3 + MAX_VALUE + 1,
                "{}",
                line.len()
            );
        }
        assert!(text.contains('…'));
        // The row with `%0A` stays one row.
        assert!(
            text.lines()
                .any(|l| l.starts_with("nl") && l.contains("│ a b"))
        );
        // The full URL is shown on top (up to its own cap).
        assert!(text.starts_with("https://e.com/?xxxx"));
    }

    #[test]
    fn many_parameters_are_summed_up() {
        let query: Vec<String> = (0..50).map(|i| format!("k{i}=v")).collect();
        let text = card_text(&format!("https://e.com/?{}", query.join("&"))).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains(" │ ")).count(), MAX_ROWS);
        assert!(text.lines().last().unwrap().contains("10"));
    }

    #[test]
    fn table_aligns_names() {
        let text = card_text("https://e.com/?a=1&longer=2").unwrap();
        assert!(text.contains("\na      │ 1\nlonger │ 2"), "{text}");
    }
}
