/// A clipboard that is only an HTML page URL. YouTube videos are handled apart.
pub fn page_url(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    if crate::youtube::video_id(text).is_some() || !crate::format::looks_like_url(text) {
        return None;
    }
    let rest = without_scheme(text);
    if rest.is_empty() || rest.starts_with('/') || is_file_url(rest) {
        return None;
    }
    Some(text)
}

fn without_scheme(text: &str) -> &str {
    let Some((scheme, rest)) = text.split_once("://") else {
        return text;
    };
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        rest
    } else {
        text
    }
}

/// Username or password in the authority (`user:secret@host`).
/// An `@` later in the path, query, or fragment is not userinfo.
pub fn url_has_userinfo(url: &str) -> bool {
    let url = url.trim();
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority.contains('@')
}

/// Authority without `user:password@`. Scheme, host, port, path, query, and
/// fragment stay as written. A URL without userinfo is returned unchanged.
pub fn strip_userinfo(url: &str) -> String {
    if !url_has_userinfo(url) {
        return url.to_string();
    }
    let (head, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (&url[..scheme.len() + 3], rest),
        None => ("", url),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let tail = &rest[authority_end..];
    let hostport = if let Some(host) = crate::format::authority_host(authority)
        && let Some(start) = authority.rfind(host)
    {
        &authority[start..]
    } else {
        authority
            .rsplit_once('@')
            .map(|(_, hostport)| hostport)
            .unwrap_or(authority)
    };
    let mut opened = String::with_capacity(head.len() + hostport.len() + tail.len());
    opened.push_str(head);
    opened.push_str(hostport);
    opened.push_str(tail);
    opened
}

pub fn host(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = crate::format::authority_host(authority)?;
    let host = host.strip_prefix("www.").unwrap_or(host);
    (!host.is_empty()).then_some(host)
}

const FILE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "pdf", "zip", "gz", "tgz", "mp4", "mp3",
    "mov", "webm", "wav", "css", "js", "mjs", "json", "xml", "txt", "csv", "tsv", "ico", "woff",
    "woff2", "ttf", "otf", "wasm", "heic", "bmp", "tif", "tiff",
];

fn is_file_url(rest: &str) -> bool {
    let path = match rest.find('/') {
        Some(index) => &rest[index..],
        None => return false,
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let file = path.rsplit('/').next().unwrap_or(path);
    let Some((_, ext)) = file.rsplit_once('.') else {
        return false;
    };
    FILE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::{host, page_url, strip_userinfo, url_has_userinfo};

    #[test]
    fn accepts_html_pages_and_skips_files() {
        assert_eq!(
            page_url("https://example.com/news/story"),
            Some("https://example.com/news/story")
        );
        assert_eq!(
            page_url("  https://www.example.com/index.html?x=1  "),
            Some("https://www.example.com/index.html?x=1")
        );
        assert_eq!(page_url("https://example.com"), Some("https://example.com"));
        assert_eq!(page_url("grok.com"), Some("grok.com"));
        assert_eq!(page_url("www.grok.com/news"), Some("www.grok.com/news"));
        assert_eq!(page_url("https://example.com/photo.jpg"), None);
        assert_eq!(page_url("https://example.com/file.pdf"), None);
        assert_eq!(
            page_url("https://www.youtube.com/watch?v=bEN9Dyg48b0"),
            None
        );
        assert_eq!(page_url("see https://example.com"), None);
    }

    #[test]
    fn credential_url_host_drops_the_user() {
        let url = "https://deploy:s3cr3t@github.com/acme/app.git";
        assert_eq!(page_url(url), Some(url));
        assert!(url_has_userinfo(url));
        assert_eq!(host(url), Some("github.com"));
        let shown = host(url).unwrap();
        assert!(!shown.contains("deploy"));
        assert!(!shown.contains("s3cr3t"));
        assert_eq!(
            host("https://deploy:s3cr3t@www.github.com:443/acme"),
            Some("github.com")
        );
        assert!(url_has_userinfo("https://deploy@github.com/acme"));
        assert!(url_has_userinfo("deploy:s3cr3t@github.com/acme"));
        assert!(!url_has_userinfo("https://github.com/acme/app"));
        assert!(!url_has_userinfo("https://example.com/a@b"));
        assert!(!url_has_userinfo("https://example.com/search?q=a@b"));
        assert!(!url_has_userinfo("https://example.com/news#user@host"));
        let canon = crate::format::format_text(url);
        assert!(url_has_userinfo(&canon));
        assert_eq!(host(&canon), Some("github.com"));
    }

    #[test]
    fn strip_userinfo_drops_credentials_and_keeps_the_rest() {
        assert_eq!(
            strip_userinfo("https://deploy:s3cr3t@github.com/acme/app.git"),
            "https://github.com/acme/app.git"
        );
        assert_eq!(
            strip_userinfo("https://u:p@host:8443/x"),
            "https://host:8443/x"
        );
        assert_eq!(
            strip_userinfo("https://u:p@github.com:8443/x"),
            "https://github.com:8443/x"
        );
        assert_eq!(
            strip_userinfo("https://deploy:s3cr3t@github.com/a@b?x=c@d#f@g"),
            "https://github.com/a@b?x=c@d#f@g"
        );
        assert_eq!(
            strip_userinfo("https://host/a@b?x=c@d"),
            "https://host/a@b?x=c@d"
        );
        let plain = "https://github.com/acme/app";
        assert_eq!(strip_userinfo(plain), plain);
    }
}
