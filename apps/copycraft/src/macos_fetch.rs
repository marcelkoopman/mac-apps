#![cfg(target_os = "macos")]

#[cfg(test)]
use std::cell::Cell;

use objc2_foundation::{NSMutableURLRequest, NSString, NSURL, NSURLConnection};

#[cfg(test)]
thread_local! {
    static FETCH_ATTEMPTS: Cell<usize> = const { Cell::new(0) };
    static SKIP_SEND: Cell<bool> = const { Cell::new(false) };
}

const DOCUMENT_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// GET without cookies. Returns the body when the load succeeds.
pub fn get(url: &str) -> Option<Vec<u8>> {
    load(url, "Copycraft", None)
}

/// GET a preview image with a browser user agent and no byte cap.
pub fn get_asset(url: &str) -> Option<Vec<u8>> {
    load(url, DOCUMENT_AGENT, None)
}

/// GET the start of a page. Asks for the first 256KB so the HTML head can be
/// read without downloading the rest of the document.
pub fn get_document(url: &str) -> Option<Vec<u8>> {
    const MAX: usize = 256 * 1024;
    let ranged = load(url, DOCUMENT_AGENT, Some("bytes=0-262143"));
    let bytes = ranged
        .filter(|bytes| bytes.len() > 64)
        .or_else(|| load(url, DOCUMENT_AGENT, None))?;
    let end = bytes.len().min(MAX);
    Some(bytes[..end].to_vec())
}

fn load(url: &str, agent: &str, range: Option<&str>) -> Option<Vec<u8>> {
    if crate::page_preview::url_has_userinfo(url) {
        return None;
    }
    #[cfg(test)]
    {
        FETCH_ATTEMPTS.with(|cell| cell.set(cell.get() + 1));
        if SKIP_SEND.with(Cell::get) {
            return None;
        }
    }
    let nsurl = NSURL::URLWithString(&NSString::from_str(url))?;
    let request = NSMutableURLRequest::requestWithURL(&nsurl);
    request.setHTTPShouldHandleCookies(false);
    request.setTimeoutInterval(8.0);
    request.setValue_forHTTPHeaderField(
        Some(&NSString::from_str(agent)),
        &NSString::from_str("User-Agent"),
    );
    if let Some(range) = range {
        request.setValue_forHTTPHeaderField(
            Some(&NSString::from_str(range)),
            &NSString::from_str("Range"),
        );
    }
    let data = send(&request).ok()?;
    Some(data.to_vec())
}

#[allow(deprecated)]
fn send(
    request: &NSMutableURLRequest,
) -> Result<
    objc2::rc::Retained<objc2_foundation::NSData>,
    objc2::rc::Retained<objc2_foundation::NSError>,
> {
    NSURLConnection::sendSynchronousRequest_returningResponse_error(request, None)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{FETCH_ATTEMPTS, SKIP_SEND};

    struct HoldSend;

    impl HoldSend {
        fn arm() -> Self {
            SKIP_SEND.with(|cell| cell.set(true));
            FETCH_ATTEMPTS.with(|cell| cell.set(0));
            Self
        }
    }

    impl Drop for HoldSend {
        fn drop(&mut self) {
            SKIP_SEND.with(|cell| cell.set(false));
        }
    }

    #[test]
    fn credential_url_does_not_start_a_request() {
        let _hold = HoldSend::arm();
        let url = "https://deploy:s3cr3t@github.com/acme/app.git";
        assert!(super::get(url).is_none());
        assert!(super::get_document(url).is_none());
        assert!(super::get_asset(url).is_none());
        assert_eq!(FETCH_ATTEMPTS.with(Cell::get), 0);
    }

    #[test]
    fn plain_url_still_reaches_the_request() {
        let _hold = HoldSend::arm();
        assert!(super::get("https://github.com/acme/app").is_none());
        assert_eq!(FETCH_ATTEMPTS.with(Cell::get), 1);
        assert!(super::get_asset("https://cdn.example.com/a.jpg?x=a@b").is_none());
        assert_eq!(FETCH_ATTEMPTS.with(Cell::get), 2);
        // A missed range is retried once, so a document counts as two attempts.
        assert!(super::get_document("https://example.com/news/story").is_none());
        assert_eq!(FETCH_ATTEMPTS.with(Cell::get), 4);
    }
}
