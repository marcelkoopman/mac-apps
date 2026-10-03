//! Which URLs Visit may hand to the browser. Pure string checks: copycraft itself never opens a
//! connection, it only asks macOS (`NSWorkspace`) to open the link in the default browser.

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

#[cfg(test)]
mod tests {
    use super::*;

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
