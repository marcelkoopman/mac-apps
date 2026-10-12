pub(super) fn strip_http_scheme(text: &str) -> Option<&str> {
    let (scheme, rest) = text.split_once("://")?;
    if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") {
        Some(rest)
    } else {
        None
    }
}

pub(crate) fn authority_host(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.contains(' ') {
        return None;
    }
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    let host = match hostport.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty()
                && !port.is_empty()
                && port.chars().all(|ch| ch.is_ascii_digit()) =>
        {
            host
        }
        Some(_) => return None,
        None => hostport,
    };
    is_url_host(host).then_some(host)
}

pub(super) fn is_url_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let tld = labels[labels.len() - 1];
    labels.iter().all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    }) && tld.len() >= 2
        && tld.chars().all(|ch| ch.is_ascii_alphabetic())
}

pub(super) fn is_filename(name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "pdf"
            | "zip"
            | "txt"
            | "json"
            | "xml"
            | "csv"
            | "mp4"
            | "mp3"
    )
}

/// Adds `https://` when the scheme is missing and percent-encodes characters
/// that are not allowed in a URI path or query parameter.
pub(super) fn format_url(text: &str) -> String {
    let text = text.trim();
    let (scheme, rest) = match strip_http_scheme(text) {
        Some(rest) => {
            let scheme = if text.to_ascii_lowercase().starts_with("http://") {
                "http"
            } else {
                "https"
            };
            (scheme, rest)
        }
        None => ("https", text),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let after = &rest[authority_end..];
    let (before_fragment, fragment) = split_marker(after, '#');
    let (path, query) = split_marker(before_fragment, '?');
    let mut out = String::new();
    out.push_str(scheme);
    out.push_str("://");
    out.push_str(authority);
    out.push_str(&encode_path(path));
    if let Some(query) = query {
        out.push('?');
        out.push_str(&encode_query(query));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&encode_component(fragment, fragment_byte));
    }
    out
}

pub(super) fn split_marker(text: &str, marker: char) -> (&str, Option<&str>) {
    match text.split_once(marker) {
        Some((head, tail)) => (head, Some(tail)),
        None => (text, None),
    }
}

pub(super) fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| encode_component(segment, query_byte))
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn encode_query(query: &str) -> String {
    query
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, value)) => format!(
                "{}={}",
                encode_component(key, query_byte),
                encode_component(value, query_byte)
            ),
            None => encode_component(part, query_byte),
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub(super) fn encode_component(text: &str, allow: fn(u8) -> bool) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            out.push('%');
            out.push(bytes[index + 1] as char);
            out.push(bytes[index + 2] as char);
            index += 3;
            continue;
        }
        let byte = bytes[index];
        if allow(byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        index += 1;
    }
    out
}

pub(super) fn query_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b':'
                | b'@'
        )
}

pub(super) fn fragment_byte(byte: u8) -> bool {
    query_byte(byte) || byte == b'/' || byte == b'?'
}
