//! Which URLs copycraft may fetch on its own (link previews, after the card is revealed) or hand
//! to the browser (Visit). Pure string checks, no DNS: a public name that resolves to a LAN
//! address is not caught here.

use std::net::Ipv4Addr;

/// Host names that only resolve inside a LAN, a container or the machine itself.
const LOCAL_SUFFIXES: &[&str] = &[
    "localhost",
    "local",
    "lan",
    "home",
    "home.arpa",
    "internal",
    "intranet",
    "corp",
    "localdomain",
    "test",
    "invalid",
];

/// Query/fragment keys (lowercase, `-` as `_`) that carry a credential when they match exactly.
const SECRET_KEYS: &[&str] = &["key", "sig", "sid", "code", "otp", "ticket", "hmac", "jwt"];
/// ... or when they contain one of these.
const SECRET_KEY_PARTS: &[&str] = &[
    "token",
    "secret",
    "passw",
    "signature",
    "apikey",
    "api_key",
    "auth",
    "session",
    "credential",
    "x_amz_",
    "x_goog_",
];
/// Opaque values at least this long look like keys or tokens.
const OPAQUE_VALUE_LEN: usize = 32;

/// May copycraft fetch `url` in the background for a preview? Only `http(s)` (or no scheme) to a
/// public host name or public IPv4 address, without userinfo and without token-like query or
/// fragment parameters (magic links, signed URLs, OAuth redirects).
pub fn may_prefetch(url: &str) -> bool {
    let url = url.trim();
    let Some(authority) = http_authority(url) else {
        return false;
    };
    if authority.contains('@') {
        return false;
    }
    let Some(host) = host_of(authority) else {
        return false;
    };
    !is_local_host(&host) && !has_secret_params(url)
}

/// May Visit open `url` in the browser? Only `http://` and `https://` URLs with a host.
pub fn may_visit(url: &str) -> bool {
    let url = url.trim();
    let Some((scheme, _)) = url.split_once("://") else {
        return false;
    };
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return false;
    }
    http_authority(url).and_then(host_of).is_some()
}

/// Authority of an `http(s)://` URL, or of a scheme-less one (`example.com/x`).
fn http_authority(url: &str) -> Option<&str> {
    let rest = match url.split_once("://") {
        Some((scheme, rest))
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") =>
        {
            rest
        }
        Some(_) => return None,
        None if url.contains(':') && !url.contains('/') && !has_port(url) => return None,
        None => url,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    (!authority.is_empty()).then_some(authority)
}

fn has_port(url: &str) -> bool {
    url.rsplit_once(':')
        .is_some_and(|(_, port)| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

/// Lowercased host without port and trailing dot. IPv6 literals keep their brackets.
fn host_of(authority: &str) -> Option<String> {
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    let host = if hostport.starts_with('[') {
        let end = hostport.find(']')?;
        &hostport[..=end]
    } else {
        match hostport.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
            Some(_) => return None,
            None => hostport,
        }
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn is_local_host(host: &str) -> bool {
    // IPv6 literals (loopback, link-local, unique local, mapped v4 ...): never prefetched.
    if host.starts_with('[') {
        return true;
    }
    // Single-label names ("router", "nas") are LAN names.
    let Some((_, last)) = host.rsplit_once('.') else {
        return true;
    };
    if LOCAL_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return true;
    }
    // TLDs are never numeric: a numeric last label is an address in some notation. Only a
    // plain dotted quad of a public address passes ("127.1", "0x7f.0.0.1" do not).
    if last.chars().all(|c| c.is_ascii_digit()) {
        return host
            .parse::<Ipv4Addr>()
            .map_or(true, |ip| !is_public_v4(ip));
    }
    false
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // carrier-grade NAT
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || a >= 240)
}

fn has_secret_params(url: &str) -> bool {
    let (before_fragment, fragment) = url.split_once('#').unwrap_or((url, ""));
    let query = before_fragment.split_once('?').map_or("", |(_, q)| q);
    [query, fragment]
        .into_iter()
        .flat_map(|part| part.split(['&', ';']))
        .filter(|pair| !pair.is_empty())
        .any(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            secret_key(key) || opaque_value(value)
        })
}

fn secret_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase().replace('-', "_");
    SECRET_KEYS.contains(&key.as_str()) || SECRET_KEY_PARTS.iter().any(|part| key.contains(part))
}

/// JWT-shaped (`eyJ…`), or a long run of key-like characters mixing letters and digits.
fn opaque_value(value: &str) -> bool {
    if value.starts_with("eyJ") && value.matches('.').count() >= 2 {
        return true;
    }
    value.len() >= OPAQUE_VALUE_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '%'))
        && value.chars().any(|c| c.is_ascii_digit())
        && value.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_pages_may_be_prefetched() {
        for url in [
            "https://github.com/acme/app",
            "http://example.com/news/story?id=42&lang=en",
            "example.com/a",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://cdn.example.com:8443/img.jpg",
            "https://8.8.8.8/x",
            "https://news.example.co.uk/search?q=keyboard#top",
        ] {
            assert!(may_prefetch(url), "{url}");
        }
    }

    #[test]
    fn local_and_private_hosts_are_skipped() {
        for url in [
            "http://localhost:3000/",
            "http://LOCALHOST./x",
            "http://app.localhost/",
            "http://printer.local/status",
            "http://nas.lan/",
            "https://wiki.corp/page",
            "http://router/",
            "http://127.0.0.1:8080/admin",
            "http://127.1/",
            "http://0x7f.0.0.1/",
            "http://10.0.0.5/",
            "http://172.16.3.4/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://100.64.0.1/",
            "http://0.0.0.0/",
            "http://[::1]/",
            "http://[fe80::1]/",
            "http://2130706433/",
            "http://my.home.arpa/",
            "http://svc.internal:8080/",
        ] {
            assert!(!may_prefetch(url), "{url}");
        }
    }

    #[test]
    fn token_like_urls_are_skipped() {
        for url in [
            "https://example.com/login?token=abc",
            "https://example.com/reset?Reset-Token=1",
            "https://example.com/cb#access_token=x&state=y",
            "https://example.com/cb?code=4/0AY0e",
            "https://bucket.s3.amazonaws.com/f?X-Amz-Signature=abc&X-Amz-Credential=d",
            "https://maps.example.com/api?key=AIzaSy",
            "https://example.com/a?sig=1",
            "https://example.com/a?session_id=1",
            "https://example.com/v?t=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
            "https://example.com/s/x?l=8f14e45fceea167a5a36dedd4bea2543xyz",
            "https://example.com/a?password=hunter2",
        ] {
            assert!(!may_prefetch(url), "{url}");
        }
    }

    #[test]
    fn non_http_and_credential_urls_are_skipped() {
        for url in [
            "ftp://example.com/f",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "mailto:me@example.com",
            "https://user:pw@example.com/",
            "https://",
            "",
        ] {
            assert!(!may_prefetch(url), "{url}");
        }
    }

    #[test]
    fn visit_allows_only_http_and_https() {
        for ok in [
            "https://example.com",
            "HTTP://example.com/a",
            "http://localhost:3000/",
            "https://example.com/login?token=abc",
        ] {
            assert!(may_visit(ok), "{ok}");
        }
        for bad in [
            "example.com",
            "file:///Applications/Calculator.app",
            "ftp://example.com/",
            "javascript:alert(1)",
            "x-apple-systempreferences:com.apple.preference",
            "smb://server/share",
            "https://",
            "",
        ] {
            assert!(!may_visit(bad), "{bad}");
        }
    }
}
