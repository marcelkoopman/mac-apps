//! Link previews: which clipboard text gets one, whether its page may be fetched, and the
//! per-session state of each preview. The fetch and the rich view are `mac_ui::link`
//! (LinkPresentation); this module is plain data so it is tested on every platform.
//!
//! Privacy: nothing is fetched on copy. The card is blurred until the user reveals it, and only
//! then is the link on the card fetched ([`Previews::begin`] takes the reveal). Results stay for
//! the session (until Wipe), failures until the card closes, so the next open tries again.

use std::time::Duration;

use zeroize::Zeroize;

/// How long LinkPresentation may take before the card falls back to the plain URL.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Fetched previews kept per session. The oldest goes first.
const KEEP: usize = 20;

/// The `http(s)` URL to preview for clipboard `text` that is one link and nothing else (a page
/// or a YouTube video, surrounding whitespace ignored). A link without a scheme gets `https://`,
/// as Format and Visit give it. `None` for anything else: text around the URL, several URLs,
/// `file:`, `javascript:` and other schemes, links to files (`.pdf`, `.jpg`, …).
pub fn preview_target(text: &str) -> Option<String> {
    let text = text.trim();
    let link =
        crate::page_preview::page_url(text).is_some() || crate::youtube::video_id(text).is_some();
    if !link || other_scheme(text) {
        return None;
    }
    let target = crate::format::format_text(text);
    mac_ui::link::fetchable(&target).then_some(target)
}

/// `mailto:`, `tel:`, `javascript:` and the like: a scheme that is not `http(s)`. A host with a
/// port (`localhost:3000`, `example.com:8080/x`) is not a scheme.
fn other_scheme(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let named = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-'));
    let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    named && !web && !rest.starts_with(|c: char| c.is_ascii_digit())
}

/// No fetch for a URL with `user:password@`, a local or LAN host, or token-like query
/// parameters (`url_policy::may_prefetch`), even after a reveal.
pub fn blocked(target: &str) -> bool {
    crate::page_preview::url_has_userinfo(target) || !crate::url_policy::may_prefetch(target)
}

/// Where the preview of one link stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not fetched yet: the card shows the URL until the user reveals it.
    Idle,
    Loading,
    Ready,
    /// The page did not load, timed out or has no usable metadata.
    Failed,
    /// Never fetched ([`blocked`]).
    Blocked,
}

enum Entry<M> {
    Ready(M),
    Failed,
}

/// The previews of one session. `M` is the fetched metadata (`mac_ui::link::Metadata` in the
/// app). One fetch runs at a time; each gets a token so a late answer for a fetch that was
/// cancelled or replaced is dropped.
pub struct Previews<M> {
    entries: Vec<(String, Entry<M>)>,
    loading: Option<(String, u64)>,
    next_token: u64,
}

impl<M> Previews<M> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            loading: None,
            next_token: 0,
        }
    }

    pub fn state(&self, target: &str) -> State {
        if blocked(target) {
            return State::Blocked;
        }
        match self.entry(target) {
            Some(Entry::Ready(_)) => State::Ready,
            Some(Entry::Failed) => State::Failed,
            None if self.loading_target() == Some(target) => State::Loading,
            None => State::Idle,
        }
    }

    pub fn metadata(&self, target: &str) -> Option<&M> {
        match self.entry(target) {
            Some(Entry::Ready(metadata)) => Some(metadata),
            _ => None,
        }
    }

    pub fn loading_target(&self) -> Option<&str> {
        self.loading.as_ref().map(|(target, _)| target.as_str())
    }

    /// Start fetching `target`: only once the card is `revealed`, and only when the link is
    /// [`State::Idle`]. Returns the token to pass to [`Previews::finish`]. A fetch already
    /// running for another link is replaced (cancel its request; its answer is dropped).
    pub fn begin(&mut self, target: &str, revealed: bool) -> Option<u64> {
        if !revealed || self.state(target) != State::Idle {
            return None;
        }
        self.next_token += 1;
        let token = self.next_token;
        self.forget_loading();
        self.loading = Some((target.to_string(), token));
        Some(token)
    }

    /// The fetch with `token` ended: with metadata, or `None` when it failed. Returns the link
    /// it was for, or `None` when that fetch was cancelled or replaced in the meantime.
    pub fn finish(&mut self, token: u64, found: Option<M>) -> Option<String> {
        if !self
            .loading
            .as_ref()
            .is_some_and(|(_, current)| *current == token)
        {
            return None;
        }
        let (target, _) = self.loading.take()?;
        self.entries.retain(|(key, _)| *key != target);
        let entry = match found {
            Some(metadata) => Entry::Ready(metadata),
            None => Entry::Failed,
        };
        self.entries.insert(0, (target.clone(), entry));
        while self.entries.len() > KEEP {
            if let Some((mut key, _)) = self.entries.pop() {
                key.zeroize();
            }
        }
        Some(target)
    }

    /// Stop waiting for the running fetch. Returns whether one was running (cancel its request).
    pub fn cancel(&mut self) -> bool {
        let running = self.loading.is_some();
        self.forget_loading();
        running
    }

    /// The card closed: stop the running fetch and forget failures, so the next open retries
    /// them. Fetched previews stay. Returns whether a fetch was running.
    pub fn close(&mut self) -> bool {
        let running = self.cancel();
        self.entries.retain_mut(|(key, entry)| {
            let keep = matches!(entry, Entry::Ready(_));
            if !keep {
                key.zeroize();
            }
            keep
        });
        running
    }

    /// Wipe: stop the running fetch and forget every link and preview. Returns whether a fetch
    /// was running.
    pub fn wipe(&mut self) -> bool {
        let running = self.cancel();
        for (key, _) in &mut self.entries {
            key.zeroize();
        }
        self.entries.clear();
        running
    }

    fn entry(&self, target: &str) -> Option<&Entry<M>> {
        self.entries
            .iter()
            .find(|(key, _)| key == target)
            .map(|(_, entry)| entry)
    }

    fn forget_loading(&mut self) {
        if let Some((mut target, _)) = self.loading.take() {
            target.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KEEP, Previews, State, blocked, preview_target};

    #[test]
    fn previews_one_http_or_https_link_only() {
        assert_eq!(
            preview_target("https://tweakers.net/").as_deref(),
            Some("https://tweakers.net/")
        );
        assert_eq!(
            preview_target("  http://example.com/news?id=3 \n").as_deref(),
            Some("http://example.com/news?id=3")
        );
        assert_eq!(
            preview_target("https://www.youtube.com/watch?v=bEN9Dyg48b0").as_deref(),
            Some("https://www.youtube.com/watch?v=bEN9Dyg48b0")
        );
        assert_eq!(
            preview_target("grok.com").as_deref(),
            Some("https://grok.com")
        );
        // A port is not a scheme (the host is local, so `blocked` keeps it from being fetched).
        assert_eq!(
            preview_target("localhost:3000/app").as_deref(),
            Some("https://localhost:3000/app")
        );
        for text in [
            "see https://tweakers.net/ for the news",
            "https://tweakers.net/ https://example.com/",
            "https://tweakers.net/\nhttps://example.com/",
            "file:///etc/hosts",
            "file://localhost/Users/me/secret.txt",
            "javascript:alert(document.cookie)",
            "JavaScript://example.com/%0Aalert(1)",
            "ftp://example.com/file",
            "mailto:me@example.com",
            "tel:+31201234567",
            "https://example.com/photo.jpg",
            "just text",
            "",
            "   ",
        ] {
            assert_eq!(preview_target(text), None, "{text:?}");
        }
    }

    #[test]
    fn private_links_are_never_fetched() {
        assert!(blocked("https://deploy:s3cr3t@github.com/acme/app.git"));
        assert!(blocked("deploy:s3cr3t@github.com/acme"));
        assert!(blocked(
            "https://viewer:s3cr3t@www.youtube.com/watch?v=bEN9Dyg48b0"
        ));
        assert!(!blocked("https://www.youtube.com/watch?v=bEN9Dyg48b0"));
        assert!(blocked("https://router.local/admin"));
        assert!(blocked("http://192.168.1.1/"));
        assert!(blocked("https://example.com/reset?token=abc123"));
        assert!(!blocked("https://tweakers.net/"));
        // Still a link card, so the URL shows, but it is never fetched.
        let lan = "http://nas.local/admin";
        assert_eq!(preview_target(lan).as_deref(), Some(lan));
        assert!(blocked(lan));
        let mut previews = Previews::<&str>::new();
        let url = "http://localhost:8080/";
        assert_eq!(previews.state(url), State::Blocked);
        assert_eq!(previews.begin(url, true), None);
    }

    #[test]
    fn fetches_only_after_the_reveal_and_once() {
        let url = "https://tweakers.net/";
        let mut previews = Previews::new();
        assert_eq!(previews.state(url), State::Idle);
        // Copy and open: blurred, nothing fetched.
        assert_eq!(previews.begin(url, false), None);
        assert_eq!(previews.state(url), State::Idle);
        let token = previews.begin(url, true).expect("revealed");
        assert_eq!(previews.state(url), State::Loading);
        // Layout runs again while it loads: no second request.
        assert_eq!(previews.begin(url, true), None);
        assert_eq!(
            previews.finish(token, Some("Tweakers")).as_deref(),
            Some(url)
        );
        assert_eq!(previews.state(url), State::Ready);
        assert_eq!(previews.metadata(url), Some(&"Tweakers"));
        // Cached for the session, also after the card closes and opens again.
        assert!(!previews.close());
        assert_eq!(previews.begin(url, true), None);
        assert_eq!(previews.metadata(url), Some(&"Tweakers"));
    }

    #[test]
    fn failure_falls_back_until_the_card_closes() {
        let url = "https://example.com/slow";
        let mut previews = Previews::<&str>::new();
        let token = previews.begin(url, true).unwrap();
        assert_eq!(previews.finish(token, None).as_deref(), Some(url));
        assert_eq!(previews.state(url), State::Failed);
        assert_eq!(previews.metadata(url), None);
        assert_eq!(previews.begin(url, true), None);
        assert!(!previews.close());
        assert_eq!(previews.state(url), State::Idle);
        assert!(previews.begin(url, true).is_some());
    }

    #[test]
    fn close_and_wipe_cancel_and_drop_late_answers() {
        let url = "https://tweakers.net/";
        let mut previews = Previews::new();
        let token = previews.begin(url, true).unwrap();
        assert!(previews.close());
        assert_eq!(previews.state(url), State::Idle);
        // The cancelled request still answers: ignored.
        assert_eq!(previews.finish(token, Some("late")), None);
        assert_eq!(previews.state(url), State::Idle);

        let token = previews.begin(url, true).unwrap();
        assert_eq!(
            previews.finish(token, Some("Tweakers")).as_deref(),
            Some(url)
        );
        let other = "https://example.com/";
        previews.begin(other, true).unwrap();
        assert!(previews.wipe());
        assert_eq!(previews.state(url), State::Idle);
        assert_eq!(previews.state(other), State::Idle);
        assert_eq!(previews.loading_target(), None);
        assert!(!previews.wipe());
    }

    #[test]
    fn another_link_replaces_the_running_fetch() {
        let first = "https://tweakers.net/";
        let second = "https://example.com/";
        let mut previews = Previews::new();
        let old = previews.begin(first, true).unwrap();
        let new = previews.begin(second, true).unwrap();
        assert_ne!(old, new);
        assert_eq!(previews.loading_target(), Some(second));
        assert_eq!(previews.state(first), State::Idle);
        assert_eq!(previews.finish(old, Some("old")), None);
        assert_eq!(
            previews.finish(new, Some("Example")).as_deref(),
            Some(second)
        );
        assert_eq!(previews.state(first), State::Idle);
    }

    #[test]
    fn keeps_the_latest_previews() {
        let mut previews = Previews::new();
        for index in 0..=KEEP {
            let url = format!("https://example.com/{index}");
            let token = previews.begin(&url, true).unwrap();
            previews.finish(token, Some(index));
        }
        assert_eq!(previews.state("https://example.com/0"), State::Idle);
        let newest = format!("https://example.com/{KEEP}");
        assert_eq!(previews.metadata(&newest), Some(&KEEP));
        assert_eq!(previews.state("https://example.com/1"), State::Ready);
    }
}
