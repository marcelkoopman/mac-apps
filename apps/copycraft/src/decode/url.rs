//! URL encoding: `%xx` escapes (and `+` as a space in a query) decoded to readable text. Offered
//! only when the copy has percent-escapes or is a full URL with a query. A URL is shown in parts:
//! scheme, host, path, then the query parameters one `key = value` per line, each decoded.
//! Userinfo (`user:password@`) is left out of the view.

use super::clock::Env;

/// The decoded view, or `None` when `text` has nothing to decode.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let escaped = has_escape(text);
    if let Some(url) = UrlParts::parse(text, false)
        && (escaped || url.query.is_some())
    {
        return Some(url.describe(env));
    }
    if !escaped {
        return None;
    }
    // Encoded as a whole (`https%3A%2F%2F…`): the decoded text, then its parts if it is a URL.
    let decoded = percent_decode(text, false)?;
    if decoded == text || !super::is_mostly_printable(&decoded) {
        return None;
    }
    match UrlParts::parse(&decoded, true) {
        Some(url) => Some(format!("{decoded}\n\n{}", url.describe(env))),
        None => Some(decoded),
    }
}

/// A `%` with two hex digits.
fn has_escape(text: &str) -> bool {
    text.as_bytes()
        .windows(3)
        .any(|w| w[0] == b'%' && w[1].is_ascii_hexdigit() && w[2].is_ascii_hexdigit())
}

/// `%xx` to bytes (a `%` without two hex digits stays as it is), `+` to a space with `plus`.
/// `None` when the bytes are not UTF-8.
fn percent_decode(text: &str, plus: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        match bytes[i] {
            b'%' if hex(i + 1).is_some() && hex(i + 2).is_some() => {
                let value = hex(i + 1).unwrap_or(0) * 16 + hex(i + 2).unwrap_or(0);
                out.push(value as u8);
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
    String::from_utf8(out).ok()
}

/// A component decoded, or as written when its bytes are not UTF-8.
fn component(text: &str, plus: bool) -> String {
    percent_decode(text, plus).unwrap_or_else(|| text.to_string())
}

/// A URL split into its parts, as written.
struct UrlParts<'a> {
    scheme: Option<&'a str>,
    host: &'a str,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
    /// Already decoded as a whole: the parts are shown as they are (no second decoding).
    decoded: bool,
}

impl<'a> UrlParts<'a> {
    /// `scheme://host/path?query#fragment`, or a link without a scheme (`www.example.com/…`)
    /// as the Link card reads it. No white space, unless the text is `decoded` already (an
    /// encoded space is a space now), and then no line break.
    fn parse(text: &'a str, decoded: bool) -> Option<Self> {
        let blank = |c: char| {
            if decoded {
                c == '\n' || c == '\r'
            } else {
                c.is_whitespace()
            }
        };
        if text.contains(blank) {
            return None;
        }
        let (scheme, rest) = match text.split_once("://") {
            Some((scheme, rest)) if is_scheme(scheme) => (Some(scheme), rest),
            Some(_) => return None,
            None if !decoded && crate::format::looks_like_url(text) => (None, text),
            None => return None,
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
        // Without `user:password@`.
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        if host.is_empty() {
            return None;
        }
        Some(Self {
            scheme,
            host,
            path,
            query,
            fragment,
            decoded,
        })
    }

    /// A part as shown: decoded (`+` a space with `plus`), unless the whole URL already was.
    fn part(&self, text: &str, plus: bool) -> String {
        if self.decoded {
            text.to_string()
        } else {
            component(text, plus)
        }
    }

    fn describe(&self, env: &Env) -> String {
        let mut lines = Vec::new();
        if let Some(scheme) = self.scheme {
            lines.push(format!("{}: {scheme}", env.pick("Schema", "Scheme")));
        }
        lines.push(format!("Host: {}", self.part(self.host, false)));
        if !self.path.is_empty() {
            lines.push(format!(
                "{}: {}",
                env.pick("Pad", "Path"),
                self.part(self.path, false)
            ));
        }
        if let Some(query) = self.query.filter(|query| !query.is_empty()) {
            lines.push(String::new());
            lines.push(env.pick("Queryparameters", "Query parameters").to_string());
            for pair in query.split('&').filter(|pair| !pair.is_empty()) {
                lines.push(match pair.split_once('=') {
                    Some((key, value)) => {
                        format!("{} = {}", self.part(key, true), self.part(value, true))
                    }
                    None => self.part(pair, true),
                });
            }
        }
        if let Some(fragment) = self.fragment.filter(|fragment| !fragment.is_empty()) {
            lines.push(String::new());
            lines.push(format!("Fragment: {}", self.part(fragment, false)));
        }
        lines.join("\n")
    }
}

/// RFC 3986: a letter, then letters, digits, `+`, `-`, `.`.
fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::decode::clock::tests::env;
    use crate::decode::testdata;
    use crate::locale::Lang;

    fn en(text: &str) -> Option<String> {
        decode(text, &env(Lang::En))
    }

    #[test]
    fn full_url_shows_parts_and_decoded_parameters() {
        let out = en(&testdata("url_query.txt")).expect("url");
        assert_eq!(
            out,
            "Scheme: https\nHost: shop.example.com:8443\nPath: /zoek/fietsen & zo\n\nQuery parameters\nq = rode fiets\ncity = Den Haag\nnote = 50% off\nempty = \nflag\n\nFragment: top"
        );
        let nl = decode(&testdata("url_query.txt"), &env(Lang::Nl)).expect("url");
        assert!(nl.starts_with("Schema: https\n"));
        assert!(nl.contains("\nPad: /zoek/fietsen & zo\n"));
        assert!(nl.contains("\nQueryparameters\nq = rode fiets\n"));
    }

    #[test]
    fn plain_query_link_without_escapes() {
        let out = en("https://example.com/docs?page=2&sort=name").expect("query");
        assert!(
            out.contains("Query parameters\npage = 2\nsort = name"),
            "{out}"
        );
        let bare = en("www.example.com/search?q=a+b").expect("bare link");
        assert!(bare.starts_with("Host: www.example.com\n"), "{bare}");
        assert!(bare.contains("q = a b"));
    }

    #[test]
    fn userinfo_is_left_out() {
        let out = en("https://user:secret@example.com/report?id=7").expect("url");
        assert!(out.contains("Host: example.com\n"), "{out}");
        assert!(!out.contains("secret"));
    }

    #[test]
    fn encoded_text_and_encoded_urls() {
        let out = en(&testdata("url_encoded_whole.txt")).expect("whole");
        assert!(
            out.starts_with("https://example.com/search?q=copy craft&lang=en\n\n"),
            "{out}"
        );
        assert!(out.contains("q = copy craft\nlang = en"));
        assert_eq!(en("caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(en("hello%20world").as_deref(), Some("hello world"));
        // `+` stays outside a query.
        assert_eq!(en("a+b%20c").as_deref(), Some("a+b c"));
    }

    #[test]
    fn nothing_to_decode() {
        for line in testdata("url_negative.txt").lines() {
            assert_eq!(en(line), None, "{line}");
        }
    }
}
