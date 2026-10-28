//! The Link card: a copy that is exactly one URL, read into its parts for a richer card than
//! one line of text. Domain, path, the names of the query parameters, the site it belongs to
//! (from a fixed local list: GitHub, GitLab, Jira, Notion, Google Docs, Figma, Stack Overflow,
//! YouTube, Facebook, Instagram, localhost), readable names for a known site's path and
//! parameters ([`crate::link_meaning`]) and warnings (a token, tracking, a user name and password, an
//! internationalised domain). String work in memory only: nothing is fetched, stored or logged
//! (no URL, domain, parameter or site), and parameter values are never shown.

use zeroize::Zeroize;

/// Characters of the path shown before it is cut in the middle.
const MAX_PATH: usize = 56;
/// Parameter names shown as chips; the rest is summed up as `+N`.
pub(crate) const MAX_PARAMS: usize = 4;
/// Characters of one parameter name shown.
const MAX_NAME: usize = 24;
/// Longer copies are not read as a link (they are not one URL a person copied).
const MAX_LEN: usize = 16 * 1024;

/// Orange of a warning label on a dark well (the darkest is the app's #07112D) and on a light
/// one; both keep body-text contrast (tested).
pub(crate) const WARN_ON_DARK: crate::card_theme::Rgb = crate::card_theme::Rgb(255, 159, 10);
pub(crate) const WARN_ON_LIGHT: crate::card_theme::Rgb = crate::card_theme::Rgb(168, 72, 0);

/// Where a link points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// A host on the web.
    Web,
    /// `localhost` or an IP address.
    Local,
    /// A `file://` URL.
    File,
}

impl Place {
    /// SF Symbol in front of the domain.
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Place::Web => "globe",
            Place::Local => "network",
            Place::File => "doc",
        }
    }
}

/// A short label under the parameters. `warn` ones are orange, the rest grey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Badge {
    pub text: String,
    pub warn: bool,
}

/// One line of the card, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    /// The domain, large, after the [`Place::symbol`].
    Domain(String),
    /// A known site: "GitHub · owner/repo · PR #42".
    Site(String),
    /// The path, small, cut in the middle when long.
    Path(String),
    /// A known site's readable names (*Video-ID*), then the other query parameter names (never
    /// values), at most [`MAX_PARAMS`], then `+N`.
    Params(Vec<String>),
    Badges(Vec<Badge>),
}

/// A copied URL, read for the card. Wiped when dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkCard {
    /// Lower case; `None` for a scheme-less link (`example.com/x`).
    pub scheme: Option<String>,
    /// Lower case, without `www.` and port; an internationalised name in its `xn--` form.
    pub host: String,
    pub port: Option<String>,
    pub place: Place,
    /// Decoded path, without query and fragment.
    pub path: String,
    /// Decoded parameter names, each once, in order (query, then a `#a=b` fragment), without
    /// the ones a [`Self::meanings`] entry stands for.
    pub params: Vec<String>,
    /// Readable names of a known site's parts (*Video-ID* for YouTube's `v`), shown as chips
    /// before the raw names.
    pub meanings: Vec<crate::link_meaning::Meaning>,
    /// The known site explains the whole path (the site line and the meanings): no path line.
    pub path_explained: bool,
    /// The known site and what the link points at there.
    pub site: Option<String>,
    /// A YouTube link's video ID (11 characters of `A-Z a-z 0-9 _ -`), the one value the card
    /// shows: it tells two YouTube links apart. Every other value and ID stays hidden.
    pub video_id: Option<String>,
    /// A parameter that looks like a token, key or signature.
    pub token: bool,
    /// Clean URL would remove tracking parameters.
    pub tracking: bool,
    /// `user:password@` in the URL (the value is never shown).
    pub credentials: bool,
    /// Non-ASCII or `xn--` (punycode) in the domain.
    pub punycode: bool,
    /// Other characters that do not belong in a host name (`%`, `_`, …).
    pub odd_host: bool,
}

impl Drop for LinkCard {
    fn drop(&mut self) {
        if let Some(scheme) = self.scheme.as_mut() {
            scheme.zeroize();
        }
        self.host.zeroize();
        if let Some(port) = self.port.as_mut() {
            port.zeroize();
        }
        self.path.zeroize();
        for name in &mut self.params {
            name.zeroize();
        }
        if let Some(site) = self.site.as_mut() {
            site.zeroize();
        }
        if let Some(id) = self.video_id.as_mut() {
            id.zeroize();
        }
    }
}

/// Is `text` exactly one URL the Link card shows (http, https, file, or localhost)?
pub(crate) fn is_link(text: &str) -> bool {
    parse(text).is_some()
}

/// Read `text` as one URL. `None` for anything else: white space inside, another scheme, no
/// host, or text around the URL.
pub(crate) fn parse(text: &str) -> Option<LinkCard> {
    let url = text.trim();
    if url.is_empty() || url.len() > MAX_LEN || url.contains(char::is_whitespace) {
        return None;
    }
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if !matches!(scheme.as_str(), "http" | "https" | "file") {
                return None;
            }
            (Some(scheme), rest)
        }
        None => {
            // Scheme-less: what the Link card took before (`example.com/x`), a YouTube link
            // and `localhost:3000/x`.
            // `mailto:`, `tel:`, `javascript:`: a scheme, not a host and port.
            if let Some((head, tail)) = url.split_once(':')
                && !head.contains('/')
                && !tail.starts_with(|c: char| c.is_ascii_digit())
            {
                return None;
            }
            let lower = url.to_ascii_lowercase();
            let local = lower == "localhost"
                || lower.starts_with("localhost:")
                || lower.starts_with("localhost/");
            if !local
                && crate::page_preview::page_url(url).is_none()
                && crate::youtube::video_id(url).is_none()
            {
                return None;
            }
            (None, url)
        }
    };
    let (before_hash, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (before_query, query) = before_hash.split_once('?').unwrap_or((before_hash, ""));
    let (authority, raw_path) =
        before_query.split_at(before_query.find('/').unwrap_or(before_query.len()));
    let file = scheme.as_deref() == Some("file");
    let credentials = !file && authority.contains('@');
    let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (raw_host, port) = split_port(hostport)?;
    if file {
        // `file:///path` or `file://localhost/path`; a path is needed.
        if !(raw_host.is_empty() || raw_host.eq_ignore_ascii_case("localhost"))
            || raw_path.len() <= 1
        {
            return None;
        }
    } else if raw_host.is_empty() || !host_chars_ok(raw_host) {
        return None;
    }
    let decoded_host = crate::link_view::percent_decode(raw_host, false).to_lowercase();
    let odd_host = !file && (raw_host.contains('%') || has_odd_host_chars(&decoded_host));
    let decoded_host = decoded_host.trim_end_matches('.').to_string();
    let (host, punycode) = ascii_host(&decoded_host);
    let host = host
        .strip_prefix("www.")
        .map(str::to_string)
        .unwrap_or(host);
    if !file && !host_shape_ok(&host) {
        return None;
    }
    let place = if file {
        Place::File
    } else if is_local_host(&host) {
        Place::Local
    } else {
        Place::Web
    };
    let pairs = pairs_of(query, fragment);
    let mut params: Vec<String> = Vec::new();
    for (name, _) in &pairs {
        if !name.is_empty() && !params.contains(name) {
            params.push(name.clone());
        }
    }
    let path = crate::link_view::percent_decode(raw_path, false);
    let token = pairs.iter().any(|(name, value)| is_token(name, value))
        || raw_path.split('/').any(looks_like_jwt);
    let tracking = !file && crate::text_ops::url_clean::clean_url(url).1 > 0;
    let segments: Vec<String> = raw_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| crate::link_view::percent_decode(segment, false))
        .collect();
    let site = if file {
        None
    } else {
        site_title(url, &host, port.as_deref(), &segments, &pairs)
    };
    let explained = if file {
        crate::link_meaning::Explained::default()
    } else {
        crate::link_meaning::explain(&host, &segments, &params)
    };
    let mut explained = explained;
    let video_id = if file {
        None
    } else {
        crate::youtube::video_id(url).map(str::to_string)
    };
    if video_id.is_some() {
        // The ID itself is on the site line.
        explained
            .meanings
            .retain(|meaning| *meaning != crate::link_meaning::Meaning::VideoId);
    }
    params.retain_mut(|name| {
        let keep = !explained.params.contains(name);
        if !keep {
            name.zeroize();
        }
        keep
    });
    for name in &mut explained.params {
        name.zeroize();
    }
    let mut segments = segments;
    for segment in &mut segments {
        segment.zeroize();
    }
    let mut pairs = pairs;
    for (name, value) in &mut pairs {
        name.zeroize();
        value.zeroize();
    }
    Some(LinkCard {
        scheme,
        host,
        port,
        place,
        path,
        params,
        meanings: explained.meanings.clone(),
        path_explained: explained.path,
        site,
        video_id,
        token,
        tracking,
        credentials,
        punycode,
        odd_host,
    })
}

/// Host and port of `host:port` (`[::1]:8080` keeps its brackets). `None` for a port that is
/// not a number.
fn split_port(hostport: &str) -> Option<(&str, Option<String>)> {
    if hostport.starts_with('[') {
        let end = hostport.find(']')?;
        let (host, after) = hostport.split_at(end + 1);
        return match after.strip_prefix(':') {
            Some(port) if is_port(port) => Some((host, Some(port.to_string()))),
            Some(_) => None,
            None if after.is_empty() => Some((host, None)),
            None => None,
        };
    }
    match hostport.rsplit_once(':') {
        Some((host, port)) if is_port(port) => Some((host, Some(port.to_string()))),
        Some(_) => None,
        None => Some((hostport, None)),
    }
}

fn is_port(port: &str) -> bool {
    !port.is_empty() && port.len() <= 5 && port.chars().all(|c| c.is_ascii_digit())
}

/// Characters that never appear in a host as typed.
fn host_chars_ok(host: &str) -> bool {
    !host.chars().any(|c| {
        c.is_control() || matches!(c, '<' | '>' | '"' | '\\' | '^' | '`' | '{' | '|' | '}')
    })
}

/// After decoding: characters outside letters, digits, `-` and `.` (an IPv6 literal aside).
fn has_odd_host_chars(host: &str) -> bool {
    if host.starts_with('[') {
        return false;
    }
    host.chars()
        .any(|c| !(c.is_alphanumeric() || matches!(c, '-' | '.')))
}

/// A host name, an IP address or `localhost`: dot-separated labels that are not empty.
fn host_shape_ok(host: &str) -> bool {
    if host.starts_with('[') {
        return host.ends_with(']') && host.len() > 2;
    }
    !host.is_empty() && host.split('.').all(|label| !label.is_empty())
}

fn is_local_host(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host.starts_with('[') || is_ipv4(host)
}

fn is_ipv4(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<u16>().is_ok_and(|n| n <= 255)
        })
}

/// `host` with every non-ASCII label in its `xn--` form, and whether the host is
/// internationalised (non-ASCII, or already `xn--`).
fn ascii_host(host: &str) -> (String, bool) {
    let mut idn = false;
    let labels: Vec<String> = host
        .split('.')
        .map(|label| {
            if label.is_ascii() {
                if label.starts_with("xn--") {
                    idn = true;
                }
                label.to_string()
            } else {
                idn = true;
                punycode(label).map_or_else(|| label.to_string(), |code| format!("xn--{code}"))
            }
        })
        .collect();
    (labels.join("."), idn)
}

/// RFC 3492 punycode of one label (without the `xn--`). `None` on overflow.
pub(crate) fn punycode(label: &str) -> Option<String> {
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    let points: Vec<u32> = label.chars().map(u32::from).collect();
    let mut out: String = label.chars().filter(char::is_ascii).collect();
    let basic = out.len() as u32;
    let mut handled = basic;
    if basic > 0 {
        out.push('-');
    }
    let mut n: u32 = 128;
    let mut delta: u32 = 0;
    let mut bias: u32 = 72;
    let total = points.len() as u32;
    while handled < total {
        let m = points.iter().copied().filter(|&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(handled + 1)?)?;
        n = m;
        for &c in &points {
            if c < n {
                delta = delta.checked_add(1)?;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias {
                        TMIN
                    } else if k >= bias + TMAX {
                        TMAX
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    out.push(punycode_digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(punycode_digit(q));
                bias = adapt(delta, handled + 1, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1)?;
        n = n.checked_add(1)?;
    }
    Some(out)
}

fn adapt(delta: u32, points: u32, first: bool) -> u32 {
    let mut delta = if first { delta / 700 } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > ((36 - 1) * 26) / 2 {
        delta /= 36 - 1;
        k += 36;
    }
    k + (36 - 1 + 1) * delta / (delta + 38)
}

fn punycode_digit(d: u32) -> char {
    let d = d as u8;
    if d < 26 {
        (b'a' + d) as char
    } else {
        (b'0' + d - 26) as char
    }
}

/// Decoded `(name, value)` pairs of the query and of a fragment that holds `a=b` pairs (an
/// OAuth `#access_token=…`).
fn pairs_of(query: &str, fragment: &str) -> Vec<(String, String)> {
    let fragment = if fragment.contains('=') { fragment } else { "" };
    query
        .split('&')
        .chain(fragment.split('&'))
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (
                crate::link_view::percent_decode(name, true),
                crate::link_view::percent_decode(value, true),
            )
        })
        .collect()
}

/// Parameter names that carry a secret.
const TOKEN_NAMES: &[&str] = &[
    "token",
    "key",
    "sig",
    "code",
    "access_token",
    "api_key",
    "apikey",
    "auth",
    "client_secret",
    "id_token",
    "password",
    "refresh_token",
    "secret",
    "signature",
    "x-amz-credential",
    "x-amz-signature",
];

/// A parameter that may hold a token: a secret's name ([`TOKEN_NAMES`], `*_token`,
/// `*_secret`), a JWT, a long hex hash or random-looking string, or a value copycraft's secret
/// check calls a credential.
fn is_token(name: &str, value: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if TOKEN_NAMES.contains(&name.as_str())
        || name.ends_with("_token")
        || name.ends_with("_secret")
        || name.ends_with("_signature")
    {
        return true;
    }
    looks_like_jwt(value)
        || looks_like_hash(value)
        || (value.len() >= 12
            && crate::sensitivity::scan_labels(value)
                .contains(&crate::sensitivity::Label::Credential))
}

/// `header.payload.signature`, base64url, the header JSON (`eyJ`).
fn looks_like_jwt(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("eyJ")
        && parts[0].len() >= 10
        && parts[1].len() >= 10
        && parts.iter().all(|part| {
            part.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
        })
}

/// 32 or more hex digits (a hash, an API key), or 24 or more base64url characters that mix
/// upper case, lower case and digits (a random token).
fn looks_like_hash(text: &str) -> bool {
    if text.len() >= 32 && text.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    text.len() >= 24
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
        && text.chars().any(|c| c.is_ascii_uppercase())
        && text.chars().any(|c| c.is_ascii_lowercase())
        && text.chars().any(|c| c.is_ascii_digit())
}

/// A Jira issue key: `ABC-123`.
fn is_jira_key(text: &str) -> bool {
    let Some((project, number)) = text.split_once('-') else {
        return false;
    };
    project.len() >= 2
        && project
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        && project
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// `my-page-title` to `my page title`.
fn slug_words(slug: &str) -> String {
    slug.replace(['-', '_', '+'], " ").trim().to_string()
}

fn short_sha(sha: &str) -> Option<String> {
    (sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("Commit {}", &sha[..7]))
}

/// The known site of a link and what it points at there, from the URL alone.
fn site_title(
    url: &str,
    host: &str,
    port: Option<&str>,
    segments: &[String],
    pairs: &[(String, String)],
) -> Option<String> {
    let seg = |at: usize| segments.get(at).map(String::as_str);
    let mut parts: Vec<String> = Vec::new();
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        return Some(match port {
            Some(port) => crate::locale::tf("link_site_port", &[&port]),
            None => "localhost".to_string(),
        });
    }
    if crate::youtube::video_id(url).is_some() {
        parts.push("YouTube".into());
        parts.push(
            if seg(0) == Some("shorts") {
                "Short"
            } else {
                "Video"
            }
            .into(),
        );
    } else if host == "github.com" {
        parts.push("GitHub".into());
        match (seg(0), seg(1)) {
            (Some(owner), Some(repo)) => {
                let repo = repo.strip_suffix(".git").unwrap_or(repo);
                parts.push(format!("{owner}/{repo}"));
                let detail = match (seg(2), seg(3)) {
                    (Some("pull"), Some(n)) if is_number(n) => Some(format!("PR #{n}")),
                    (Some("issues"), Some(n)) if is_number(n) => Some(format!("Issue #{n}")),
                    (Some("commit"), Some(sha)) => short_sha(sha),
                    (Some("blob" | "tree"), Some(_)) => {
                        segments.last().filter(|_| segments.len() > 4).cloned()
                    }
                    (Some("pulls"), None) => Some("Pull requests".into()),
                    (Some("issues"), None) => Some("Issues".into()),
                    _ => None,
                };
                parts.extend(detail);
            }
            (Some(owner), None) => parts.push(owner.to_string()),
            _ => {}
        }
    } else if host == "gitlab.com" || host.starts_with("gitlab.") {
        parts.push("GitLab".into());
        let dash = segments.iter().position(|s| s == "-");
        let project = &segments[..dash.unwrap_or(segments.len())];
        if !project.is_empty() {
            parts.push(project.join("/"));
        }
        if let Some(dash) = dash {
            let after = |at: usize| segments.get(dash + 1 + at).map(String::as_str);
            let detail = match (after(0), after(1)) {
                (Some("merge_requests"), Some(n)) if is_number(n) => Some(format!("MR !{n}")),
                (Some("issues"), Some(n)) if is_number(n) => Some(format!("Issue #{n}")),
                (Some("commit"), Some(sha)) => short_sha(sha),
                (Some("blob" | "tree"), Some(_)) if segments.len() > dash + 3 => {
                    segments.last().cloned()
                }
                _ => None,
            };
            parts.extend(detail);
        }
    } else if let Some(key) = jira_key(host, segments, pairs) {
        parts.push("Jira".into());
        parts.push(key);
    } else if host.ends_with(".atlassian.net") && seg(0) == Some("jira") {
        parts.push("Jira".into());
    } else if host == "notion.so" || host.ends_with(".notion.so") || host.ends_with(".notion.site")
    {
        parts.push("Notion".into());
        if let Some(last) = segments.last() {
            // `Page-Title-<32 hex>`: the title words.
            let title = match last.rsplit_once('-') {
                Some((title, id))
                    if id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()) =>
                {
                    slug_words(title)
                }
                _ => String::new(),
            };
            if !title.is_empty() {
                parts.push(title);
            }
        }
    } else if host == "docs.google.com" {
        parts.push(
            match seg(0) {
                Some("spreadsheets") => "Google Sheets",
                Some("presentation") => "Google Slides",
                Some("forms") => "Google Forms",
                Some("drawings") => "Google Drawings",
                _ => "Google Docs",
            }
            .into(),
        );
    } else if host == "drive.google.com" {
        parts.push("Google Drive".into());
    } else if host == "figma.com" {
        parts.push("Figma".into());
        if matches!(
            seg(0),
            Some("file" | "design" | "proto" | "board" | "slides")
        ) && let Some(name) = seg(2)
        {
            let name = slug_words(name);
            if !name.is_empty() {
                parts.push(name);
            }
        }
    } else if host == "stackoverflow.com" {
        parts.push("Stack Overflow".into());
        if seg(0) == Some("questions")
            && let Some(n) = seg(1).filter(|n| is_number(n))
        {
            match seg(2).map(slug_words).filter(|words| !words.is_empty()) {
                Some(words) => parts.push(words),
                None => parts.push(format!("#{n}")),
            }
        }
    } else {
        let names: Vec<String> = pairs.iter().map(|(name, _)| name.clone()).collect();
        return crate::link_meaning::social(host, segments, &names);
    }
    Some(parts.join(" · "))
}

/// A Jira issue key on a Jira host (`*.atlassian.net`, a host with `jira` in it) or under
/// `/browse/` anywhere: the path or `?selectedIssue=`.
fn jira_key(host: &str, segments: &[String], pairs: &[(String, String)]) -> Option<String> {
    let jira_host =
        host.ends_with(".atlassian.net") || host.split('.').any(|label| label == "jira");
    if let Some(at) = segments.iter().position(|s| s == "browse")
        && let Some(key) = segments.get(at + 1).filter(|key| is_jira_key(key))
    {
        return Some(key.clone());
    }
    if !jira_host {
        return None;
    }
    pairs
        .iter()
        .find(|(name, value)| name == "selectedIssue" && is_jira_key(value))
        .map(|(_, value)| value.clone())
        .or_else(|| segments.iter().rev().find(|s| is_jira_key(s)).cloned())
}

/// At most `max` characters of `text` on one line, cut in the middle with `…`.
pub(crate) fn middle_cut(text: &str, max: usize) -> String {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if chars.len() <= max {
        return chars.into_iter().collect();
    }
    let head = (max - 1) / 2;
    let tail = max - 1 - head;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

fn one_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .take(MAX_NAME)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if name.chars().nth(MAX_NAME).is_some() {
        out.push('…');
    }
    out
}

impl LinkCard {
    /// The domain as shown: host and port.
    pub(crate) fn domain(&self) -> String {
        if self.place == Place::File {
            return crate::locale::t("link_file_host").to_string();
        }
        match &self.port {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        }
    }

    /// What Copy domain puts on the clipboard: the host, without port; empty for a file.
    pub(crate) fn copy_domain(&self) -> Option<String> {
        (self.place != Place::File).then(|| self.host.clone())
    }

    /// The SF Symbol in front of the domain: a known brand's ([`crate::link_meaning::Brand`],
    /// YouTube, Facebook, TikTok) or the place's (`globe`, `network`, `doc`).
    pub(crate) fn symbol(&self) -> &'static str {
        crate::link_meaning::brand(&self.host)
            .filter(|_| self.place == Place::Web)
            .map_or(self.place.symbol(), crate::link_meaning::Brand::symbol)
    }

    /// The site line: the known site, and a YouTube link's video ID after it
    /// (`YouTube · Video · aBcDeFgHiJk`).
    pub(crate) fn site_line(&self) -> Option<String> {
        let site = self.site.as_ref()?;
        Some(match &self.video_id {
            Some(id) => format!("{site} · {id}"),
            None => site.clone(),
        })
    }

    /// The labels, warnings first.
    pub(crate) fn badges(&self) -> Vec<Badge> {
        let mut out = Vec::new();
        let mut push = |text: &str, warn: bool| {
            out.push(Badge {
                text: text.to_string(),
                warn,
            });
        };
        if self.token {
            push(crate::locale::t("link_badge_token"), true);
        }
        if self.credentials {
            push(crate::locale::t("link_badge_credentials"), true);
        }
        if self.punycode {
            push(crate::locale::t("link_badge_punycode"), true);
        }
        if self.odd_host {
            push(crate::locale::t("link_badge_odd_host"), true);
        }
        if self.tracking {
            push(crate::locale::t("link_badge_tracking"), false);
        }
        if let Some(scheme) = self.scheme.as_deref().filter(|scheme| *scheme != "https") {
            out.push(Badge {
                text: scheme.to_string(),
                warn: false,
            });
        }
        out
    }

    /// The card's lines, top to bottom.
    pub(crate) fn lines(&self) -> Vec<Line> {
        let mut lines = vec![Line::Domain(self.domain())];
        if let Some(site) = self.site_line() {
            lines.push(Line::Site(site));
        }
        if !self.path_explained && !self.path.is_empty() && self.path != "/" {
            lines.push(Line::Path(middle_cut(&self.path, MAX_PATH)));
        }
        let all: Vec<String> = self
            .meanings
            .iter()
            .map(|meaning| meaning.label().to_string())
            .chain(self.params.iter().map(|name| one_name(name)))
            .collect();
        if !all.is_empty() {
            let mut names: Vec<String> = all.iter().take(MAX_PARAMS).cloned().collect();
            if all.len() > MAX_PARAMS {
                names.push(format!("+{}", all.len() - MAX_PARAMS));
            }
            lines.push(Line::Params(names));
        }
        let badges = self.badges();
        if !badges.is_empty() {
            lines.push(Line::Badges(badges));
        }
        lines
    }

    /// The card as plain text (search, selection, VoiceOver's value): one line per [`Line`].
    pub(crate) fn text(&self) -> String {
        self.lines()
            .iter()
            .map(|line| match line {
                Line::Domain(text) | Line::Site(text) | Line::Path(text) => text.clone(),
                Line::Params(names) => names.join("  "),
                Line::Badges(badges) => badges
                    .iter()
                    .map(|badge| badge.text.as_str())
                    .collect::<Vec<_>>()
                    .join("  ·  "),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// VoiceOver label of the well: the domain and what kind of link it is.
    pub(crate) fn ax_label(&self) -> String {
        let kind = match (self.site_line(), self.place) {
            (Some(site), _) => site,
            (None, Place::Web) => crate::locale::t("link_kind_web").to_string(),
            (None, Place::Local) => crate::locale::t("link_kind_local").to_string(),
            (None, Place::File) => crate::locale::t("link_kind_file").to_string(),
        };
        crate::locale::tf("link_ax", &[&self.domain(), &kind])
    }

    /// Open asks first: the link may hand a token to the browser and the site.
    pub(crate) fn asks_before_open(&self) -> bool {
        self.token
    }
}

/// What Open hands to the browser: an `http(s)` link (scheme-less ones get one; `localhost`
/// gets `http://`), without `user:password@`, when [`crate::url_policy::may_visit`] allows it.
/// `None` for a `file://` link and anything that is not a link.
pub(crate) fn open_url(text: &str) -> Option<String> {
    let link = parse(text)?;
    if link.place == Place::File {
        return None;
    }
    let raw = text.trim();
    let url = if link.scheme.is_none() && raw.to_ascii_lowercase().starts_with("localhost") {
        format!("http://{raw}")
    } else {
        crate::format::format_text(raw)
    };
    let url = crate::page_preview::strip_userinfo(&url);
    crate::url_policy::may_visit(&url).then_some(url)
}

/// What Copy cleaned puts on the clipboard: `text` without tracking parameters and without
/// `user:password@`.
pub(crate) fn cleaned(text: &str) -> String {
    let url = crate::page_preview::strip_userinfo(text.trim());
    crate::text_ops::url_clean::clean_url(&url).0
}

/// The one `http(s)` URL inside a longer text (a sentence with a link), for Open link on a
/// text card. `None` without one, with more than one, or when the text is a link itself.
pub(crate) fn link_in_text(text: &str) -> Option<&str> {
    if text.len() > MAX_LEN || is_link(text) {
        return None;
    }
    let ranges = crate::text_ops::url_clean::url_ranges(text);
    match ranges.as_slice() {
        [one] => Some(&text[one.clone()]),
        _ => None,
    }
}

#[cfg(test)]
#[path = "link_card_tests.rs"]
mod tests;
