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

/// Address used to fetch and open a page, with `https://` added when missing.
pub fn canonical_url(text: &str) -> Option<String> {
    let page = page_url(text)?;
    Some(crate::format::format_text(page))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlPreview {
    pub title: Option<String>,
    pub image: Option<String>,
}

pub fn from_html(html: &str, page: &str) -> HtmlPreview {
    let head = html_head(html);
    let title = meta_content(head, "og:title")
        .or_else(|| meta_content(head, "twitter:title"))
        .or_else(|| title_tag(head));
    // Shops such as Magento omit og:image and put the product picture in
    // schema.org JSON-LD after </head>.
    let image = meta_content(head, "og:image")
        .or_else(|| meta_content(head, "og:image:secure_url"))
        .or_else(|| meta_content(head, "twitter:image"))
        .or_else(|| meta_content(head, "twitter:image:src"))
        .or_else(|| jsonld_image(html))
        .and_then(|url| absolute_url(&url, page));
    HtmlPreview { title, image }
}

const FILE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "pdf", "zip", "gz", "tgz", "mp4", "mp3",
    "mov", "webm", "wav", "css", "js", "mjs", "json", "xml", "txt", "csv", "tsv", "ico", "woff",
    "woff2", "ttf", "otf", "wasm", "heic", "bmp", "tif", "tiff",
];

fn html_head(html: &str) -> &str {
    find_ci(html, "</head>")
        .map(|index| &html[..index])
        .unwrap_or(html)
}

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

fn meta_content(html: &str, key: &str) -> Option<String> {
    let mut rest = html;
    while let Some(start) = find_ci(rest, "<meta") {
        let after = &rest[start + 5..];
        let (tag, next) = split_tag(after);
        let matches = attr_is(tag, "property", key) || attr_is(tag, "name", key);
        if matches && let Some(content) = attr(tag, "content") {
            let decoded = decode_basic_entities(content.trim());
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
        if next.is_empty() {
            break;
        }
        rest = next;
    }
    None
}

fn title_tag(html: &str) -> Option<String> {
    let start = find_ci(html, "<title")?;
    let (_, inner) = split_tag(&html[start..]);
    let close = find_ci(inner, "</title")?;
    let text = decode_basic_entities(inner[..close].trim());
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!flat.is_empty()).then_some(flat)
}

/// Highest-ranked schema.org picture in `application/ld+json` blocks.
fn jsonld_image(html: &str) -> Option<String> {
    let mut best = (0_u8, None);
    for body in ldjson_bodies(html) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
            continue;
        };
        consider_jsonld(&value, &mut best);
    }
    best.1
}

fn ldjson_bodies(html: &str) -> Vec<&str> {
    let mut bodies = Vec::new();
    let mut rest = html;
    while let Some(start) = find_ci(rest, "<script") {
        let after = &rest[start + "<script".len()..];
        let (tag, next) = split_tag(after);
        let is_ld = attr(tag, "type").is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .eq_ignore_ascii_case("application/ld+json")
        });
        if !is_ld {
            if next.is_empty() {
                break;
            }
            rest = next;
            continue;
        }
        let Some(close) = find_ci(next, "</script>") else {
            break;
        };
        let body = trim_ldjson(&next[..close]);
        if !body.is_empty() {
            bodies.push(body);
        }
        let after_close = close + "</script>".len();
        if after_close >= next.len() {
            break;
        }
        rest = &next[after_close..];
    }
    bodies
}

fn trim_ldjson(body: &str) -> &str {
    let body = body.trim().trim_start_matches('\u{feff}');
    body.strip_prefix("<!--")
        .and_then(|inner| inner.strip_suffix("-->"))
        .unwrap_or(body)
        .trim()
}

fn consider_jsonld(value: &serde_json::Value, best: &mut (u8, Option<String>)) {
    let serde_json::Value::Object(map) = value else {
        if let serde_json::Value::Array(items) = value {
            for item in items {
                consider_jsonld(item, best);
            }
        }
        return;
    };
    let rank = map.get("@type").map_or(0, type_rank);
    if rank > best.0
        && let Some(url) = image_from_fields(map)
    {
        *best = (rank, Some(url));
    }
    for (key, child) in map {
        if key == "@context" || key == "@type" {
            continue;
        }
        consider_jsonld(child, best);
    }
}

fn type_rank(value: &serde_json::Value) -> u8 {
    match value {
        serde_json::Value::String(text) => rank_type_name(text),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(rank_type_name)
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

fn rank_type_name(text: &str) -> u8 {
    let name = text.rsplit(['/', '#']).next().unwrap_or(text);
    match name {
        "Product" | "ProductGroup" | "IndividualProduct" | "ProductModel" => 3,
        "Article" | "NewsArticle" | "BlogPosting" | "TechArticle" | "Recipe" | "Event"
        | "VideoObject" | "Movie" | "Book" => 2,
        "WebPage" | "ItemPage" => 1,
        _ => 0,
    }
}

fn image_from_fields(map: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    ["image", "primaryImageOfPage", "thumbnailUrl"]
        .iter()
        .find_map(|key| map.get(*key).and_then(image_value))
}

fn image_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => usable_image_url(text),
        serde_json::Value::Array(items) => items.iter().find_map(image_value),
        serde_json::Value::Object(map) => map
            .get("url")
            .or_else(|| map.get("contentUrl"))
            .and_then(image_value),
        _ => None,
    }
}

fn usable_image_url(text: &str) -> Option<String> {
    let text = decode_basic_entities(text.trim());
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("data:")
        || lower.starts_with("javascript:")
        || lower.starts_with("http://schema.org")
        || lower.starts_with("https://schema.org")
    {
        return None;
    }
    Some(text)
}

fn attr_is(tag: &str, name: &str, key: &str) -> bool {
    attr(tag, name).is_some_and(|value| value.eq_ignore_ascii_case(key))
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let name_start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'>'
        {
            i += 1;
        }
        let attr_name = &tag[name_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        if quote != b'"' && quote != b'\'' {
            continue;
        }
        i += 1;
        let value_start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        let value = &tag[value_start..i];
        if attr_name.eq_ignore_ascii_case(name) {
            return Some(value);
        }
        if i < bytes.len() {
            i += 1;
        }
    }
    None
}

fn split_tag(text: &str) -> (&str, &str) {
    let bytes = text.as_bytes();
    let mut quote = None;
    for (index, byte) in bytes.iter().enumerate() {
        match quote {
            Some(open) if *byte == open => quote = None,
            Some(_) => {}
            None if *byte == b'"' || *byte == b'\'' => quote = Some(*byte),
            None if *byte == b'>' => return (&text[..index], &text[index + 1..]),
            None => {}
        }
    }
    (text, "")
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn absolute_url(url: &str, page: &str) -> Option<String> {
    let url = url.trim();
    if url.starts_with("https://") || url.starts_with("http://") {
        return Some(url.to_string());
    }
    if let Some(rest) = url.strip_prefix("//") {
        let scheme = page.split_once("://")?.0;
        return Some(format!("{scheme}://{rest}"));
    }
    let origin = origin(page)?;
    if let Some(path) = url.strip_prefix('/') {
        return Some(format!("{origin}/{path}"));
    }
    let dir = page_dir(page);
    Some(format!("{dir}{url}"))
}

fn origin(page: &str) -> Option<&str> {
    let (scheme, rest) = page.split_once("://")?;
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if host_end == 0 {
        return None;
    }
    Some(&page[..scheme.len() + 3 + host_end])
}

fn page_dir(page: &str) -> String {
    let Some(origin) = origin(page) else {
        return page.to_string();
    };
    let path = &page[origin.len()..];
    let path = path.split(['?', '#']).next().unwrap_or(path);
    match path.rfind('/') {
        Some(index) => format!("{}{}", origin, &path[..=index]),
        None => format!("{origin}/"),
    }
}

fn decode_basic_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..end];
        let decoded = if let Some(hex) = entity
            .strip_prefix("#x")
            .or_else(|| entity.strip_prefix("#X"))
        {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(decimal) = entity.strip_prefix('#') {
            decimal.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "nbsp" => Some(if entity == "nbsp" { '\u{00a0}' } else { '\'' }),
                _ => None,
            }
        };
        if let Some(ch) = decoded {
            out.push(ch);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{from_html, host, page_url, strip_userinfo, url_has_userinfo};

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
        assert_eq!(
            super::canonical_url("grok.com").as_deref(),
            Some("https://grok.com")
        );
        assert_eq!(page_url("https://example.com/photo.jpg"), None);
        assert_eq!(page_url("https://example.com/file.pdf"), None);
        assert_eq!(
            page_url("https://www.youtube.com/watch?v=bEN9Dyg48b0"),
            None
        );
        assert_eq!(page_url("see https://example.com"), None);
    }

    #[test]
    fn reads_open_graph_preview() {
        let html = r#"
            <html><head>
            <title>Fallback</title>
            <meta content="https://cdn.example.com/a.jpg" property="og:image">
            <meta property="og:title" content="Hello &amp; Co">
            </head></html>
        "#;
        let preview = from_html(html, "https://example.com/news/story");
        assert_eq!(preview.title.as_deref(), Some("Hello & Co"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://cdn.example.com/a.jpg")
        );
    }

    #[test]
    fn resolves_relative_images_and_title_tag() {
        let html = r#"<title>  A   page </title><meta name="twitter:image" content="/pic.png">"#;
        let preview = from_html(html, "https://www.example.com/news/story");
        assert_eq!(preview.title.as_deref(), Some("A page"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://www.example.com/pic.png")
        );
        assert_eq!(
            host("https://www.example.com/news/story"),
            Some("example.com")
        );
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
        let canon = super::canonical_url(url).unwrap();
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

    #[test]
    fn protocol_relative_image_keeps_the_page_scheme() {
        let html = r#"<meta property="og:image" content="//cdn.example.com/a.jpg">"#;
        let preview = from_html(html, "http://example.com/a");
        assert_eq!(
            preview.image.as_deref(),
            Some("http://cdn.example.com/a.jpg")
        );
    }

    #[test]
    fn product_jsonld_image_outside_the_head() {
        let html = r#"
            <html><head>
            <title>Owon DGE3032 function generator</title>
            <meta name="description" content="14 bit function generator">
            </head><body>
            <script type="application/ld+json">
            {"@context":"http://schema.org","@type":"BreadcrumbList","itemListElement":[]}
            </script>
            <script type="application/ld+json">
            {
              "@context": "http://schema.org",
              "@type": "Product",
              "name": "Owon DGE3032 function generator",
              "image": "https://static.eleshop.nl/mage/media/catalog/product/d/g/dge3030_1.jpg"
            }
            </script>
            </body></html>
        "#;
        let page = "https://eleshop.eu/test-measure/function-generators/all-function-generators/owon-dge3032-function-generator.html";
        assert_eq!(page_url(page), Some(page));
        let preview = from_html(html, page);
        assert_eq!(
            preview.title.as_deref(),
            Some("Owon DGE3032 function generator")
        );
        assert_eq!(
            preview.image.as_deref(),
            Some("https://static.eleshop.nl/mage/media/catalog/product/d/g/dge3030_1.jpg")
        );
    }

    #[test]
    fn open_graph_image_beats_schema_product() {
        let html = r#"
            <head>
            <meta property="og:image" content="https://cdn.example.com/og.jpg">
            </head>
            <script type="application/ld+json">
            {"@type":"Product","image":"https://cdn.example.com/product.jpg"}
            </script>
        "#;
        let preview = from_html(html, "https://shop.example/item");
        assert_eq!(
            preview.image.as_deref(),
            Some("https://cdn.example.com/og.jpg")
        );
    }

    #[test]
    fn schema_graph_prefers_the_product_over_a_logo() {
        let html = r#"
            <head><title>Generator</title></head>
            <script type="application/ld+json">
            {"@graph":[
              {"@type":"Organization","image":"https://cdn.example.com/logo.png"},
              {"@type":"https://schema.org/Product","image":[
                "https://cdn.example.com/product.jpg",
                "https://cdn.example.com/alt.jpg"
              ]}
            ]}
            </script>
        "#;
        let preview = from_html(html, "https://shop.example/item");
        assert_eq!(
            preview.image.as_deref(),
            Some("https://cdn.example.com/product.jpg")
        );
    }

    #[test]
    fn schema_image_object_resolves_against_the_page() {
        let html = r#"
            <head><title>Story</title></head>
            <script type="application/ld+json">
            {"@type":"NewsArticle","image":{"@type":"ImageObject","url":"/photos/a.jpg?x=1&amp;y=2"}}
            </script>
        "#;
        let preview = from_html(html, "https://news.example/story");
        assert_eq!(preview.title.as_deref(), Some("Story"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://news.example/photos/a.jpg?x=1&y=2")
        );
        let logo_only = r#"
            <head><title>Shop</title></head>
            <script type="application/ld+json">
            {"@type":"Organization","image":"https://cdn.example.com/logo.png"}
            </script>
        "#;
        assert_eq!(from_html(logo_only, "https://shop.example/").image, None);
    }
}
