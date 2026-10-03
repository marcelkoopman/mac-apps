//! Recognising a copied YouTube link, for the YouTube card and Visit. String checks only:
//! nothing is fetched (copycraft works offline).

/// A clipboard that is only a YouTube video URL.
pub fn video_id(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let rest = strip_http(text);
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    let (host, path) = rest.split_once('/')?;
    let host = host.split(':').next().unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    match host {
        "youtu.be" => take_id(path),
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => {
            if let Some(query) = path.strip_prefix("watch?") {
                query_id(query, "v")
            } else if let Some(rest) = path
                .strip_prefix("embed/")
                .or_else(|| path.strip_prefix("shorts/"))
                .or_else(|| path.strip_prefix("live/"))
                .or_else(|| path.strip_prefix("v/"))
            {
                take_id(rest)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn strip_http(text: &str) -> &str {
    let Some((scheme, rest)) = text.split_once("://") else {
        return text;
    };
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        rest
    } else {
        text
    }
}

fn take_id(path: &str) -> Option<&str> {
    let id = path.split(['?', '/', '&', '#']).next()?;
    is_video_id(id).then_some(id)
}

fn query_id<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| take_id(value)).flatten()
    })
}

fn is_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::video_id;

    const WATCH: &str = "https://www.youtube.com/watch?v=bEN9Dyg48b0";

    #[test]
    fn reads_common_youtube_links() {
        assert_eq!(video_id(WATCH), Some("bEN9Dyg48b0"));
        assert_eq!(
            video_id("  https://youtu.be/bEN9Dyg48b0?t=12  "),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("https://www.youtube.com/shorts/bEN9Dyg48b0"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("https://www.youtube.com/embed/bEN9Dyg48b0"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("https://www.youtube.com/live/bEN9Dyg48b0"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("https://m.youtube.com/watch?feature=share&v=bEN9Dyg48b0"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("https://music.youtube.com/watch?v=bEN9Dyg48b0&list=abc"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(
            video_id("http://www.youtube-nocookie.com/embed/bEN9Dyg48b0"),
            Some("bEN9Dyg48b0")
        );
        assert_eq!(video_id("youtu.be/bEN9Dyg48b0"), Some("bEN9Dyg48b0"));
    }

    #[test]
    fn ignores_other_text() {
        assert_eq!(video_id("https://example.com/watch?v=bEN9Dyg48b0"), None);
        assert_eq!(video_id("https://www.youtube.com/playlist?list=abc"), None);
        assert_eq!(video_id("see https://youtu.be/bEN9Dyg48b0"), None);
        assert_eq!(video_id("https://youtu.be/short"), None);
        assert_eq!(video_id(""), None);
    }
}
