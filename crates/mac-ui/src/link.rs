//! Link previews with Apple's LinkPresentation framework: [`fetch`] asks an
//! `LPMetadataProvider` for a web page's metadata (title, site, icon, image) and [`view`] shows
//! the result in an `LPLinkView`.
//!
//! Only `http` and `https` URLs are fetched ([`fetchable`]): LinkPresentation would also read a
//! `file:` URL from disk. The fetch runs in the background and reports once, on the main thread.
//! Sandboxed apps need the `com.apple.security.network.client` entitlement.

/// Whether [`fetch`] takes `url`: one `http://` or `https://` URL with a host, nothing else
/// around it (surrounding whitespace is ignored).
pub fn fetchable(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() || url.contains(char::is_whitespace) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let web = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    web && !host.is_empty()
}

#[cfg(target_os = "macos")]
pub use appkit::{Fetch, Metadata, fetch, view};

#[cfg(target_os = "macos")]
mod appkit {
    use std::sync::Mutex;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2::MainThreadOnly;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSAccessibility, NSView};
    use objc2_foundation::{NSError, NSOperationQueue, NSString, NSURL};
    use objc2_link_presentation::{LPLinkMetadata, LPLinkView, LPMetadataProvider};

    /// The metadata of one page, as fetched. Main thread only.
    #[derive(Clone)]
    pub struct Metadata(Retained<LPLinkMetadata>);

    impl Metadata {
        /// The page title, when the page has a non-blank one.
        pub fn title(&self) -> Option<String> {
            // SAFETY: a plain property read on a metadata object owned here.
            let title = unsafe { self.0.title() }?.to_string();
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            (!title.is_empty()).then_some(title)
        }
    }

    /// A metadata fetch in flight. Keep it until the fetch ends; [`Fetch::cancel`] stops it.
    pub struct Fetch(Retained<LPMetadataProvider>);

    impl Fetch {
        /// Stop the fetch. The callback still runs once, with `None`, unless it already ran.
        pub fn cancel(&self) {
            // SAFETY: `cancel` may be called at any time, also after the fetch finished.
            unsafe { self.0.cancel() };
        }
    }

    /// What the provider's background completion handler hands to the main queue.
    struct Handoff<F> {
        done: F,
        found: Option<Retained<LPLinkMetadata>>,
    }

    // SAFETY: `done` is `Send`. The metadata is only moved here, from the provider's completion
    // queue to the main queue, which is how Apple documents its use (fetch in the background,
    // show it on the main queue); nothing touches it on the way.
    unsafe impl<F: Send> Send for Handoff<F> {}

    /// Fetch the metadata of `url` (when [`super::fetchable`]), giving up after `timeout`.
    /// `done` runs once on the main thread: with the metadata, or `None` when the page could not
    /// be loaded, timed out or was cancelled. Returns `None`, without calling `done`, for a URL
    /// that is not fetched. Keep the returned [`Fetch`] to cancel it.
    pub fn fetch<F>(url: &str, timeout: Duration, done: F) -> Option<Fetch>
    where
        F: FnOnce(Option<Metadata>) + Send + 'static,
    {
        if !super::fetchable(url) {
            return None;
        }
        let address = NSURL::URLWithString(&NSString::from_str(url.trim()))?;
        // SAFETY: plain initialiser. One provider per fetch, as LinkPresentation requires.
        let provider = unsafe { LPMetadataProvider::new() };
        // SAFETY: a plain property write before the fetch starts.
        unsafe { provider.setTimeout(timeout.as_secs_f64()) };
        let done = Mutex::new(Some(done));
        let handler = RcBlock::new(move |metadata: *mut LPLinkMetadata, error: *mut NSError| {
            let Some(done) = done.lock().ok().and_then(|mut slot| slot.take()) else {
                return;
            };
            let found = if error.is_null() {
                // SAFETY: on success the provider passes a valid metadata object (or nil),
                // retained here so it outlives the handler.
                unsafe { Retained::retain(metadata) }
            } else {
                None
            };
            let handoff = Mutex::new(Some(Handoff { done, found }));
            let on_main = RcBlock::new(move || {
                if let Some(Handoff { done, found }) =
                    handoff.lock().ok().and_then(|mut slot| slot.take())
                {
                    done(found.map(Metadata));
                }
            });
            // SAFETY: the block only holds a `Send` handoff behind a mutex.
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&on_main) };
        });
        // SAFETY: `address` is an http(s) URL and this provider has not fetched before. The
        // handler takes the (metadata, error) pair LinkPresentation passes.
        unsafe { provider.startFetchingMetadataForURL_completionHandler(&address, &handler) };
        Some(Fetch(provider))
    }

    /// A rich preview of `metadata` (title, site, image). VoiceOver reads the page title, or
    /// `fallback_label` when the page has none.
    pub fn view(
        mtm: MainThreadMarker,
        metadata: &Metadata,
        fallback_label: &str,
    ) -> Retained<NSView> {
        // SAFETY: plain initialiser with metadata from `fetch`.
        let view = unsafe { LPLinkView::initWithMetadata(LPLinkView::alloc(mtm), &metadata.0) };
        let label = metadata
            .title()
            .unwrap_or_else(|| fallback_label.to_string());
        view.setAccessibilityLabel(Some(&NSString::from_str(&label)));
        Retained::into_super(view)
    }
}

#[cfg(test)]
mod tests {
    use super::fetchable;

    #[test]
    fn fetches_only_one_http_or_https_url() {
        assert!(fetchable("https://tweakers.net/"));
        assert!(fetchable("  http://example.com/a?b=c#d \n"));
        assert!(fetchable("HTTPS://Example.com"));
        assert!(!fetchable("file:///etc/hosts"));
        assert!(!fetchable("javascript:alert(1)"));
        assert!(!fetchable("ftp://example.com/file"));
        assert!(!fetchable("https://"));
        assert!(!fetchable("https:///path"));
        assert!(!fetchable("tweakers.net"));
        assert!(!fetchable("see https://tweakers.net/"));
        assert!(!fetchable(""));
    }
}
