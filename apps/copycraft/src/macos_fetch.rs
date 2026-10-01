#![cfg(target_os = "macos")]

#[cfg(test)]
use std::cell::Cell;

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_foundation::{
    NSData, NSHTTPURLResponse, NSMutableURLRequest, NSString, NSURL, NSURLConnection, NSURLResponse,
};

#[cfg(test)]
thread_local! {
    static FETCH_ATTEMPTS: Cell<usize> = const { Cell::new(0) };
    static SKIP_SEND: Cell<bool> = const { Cell::new(false) };
}

/// Largest body any fetch accepts (thumbnails, oEmbed JSON, preview images, pages).
///
/// `NSURLConnection`'s synchronous API only returns once the body is in memory, so the cap
/// rejects oversized bodies after the transfer, not during it; the 8 s request timeout bounds
/// slow transfers. A streaming cap needs an `NSURLSession` delegate (not done: no Mac to test it
/// here, and the completion-handler API would need `block2` as a new direct dependency).
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

const DOCUMENT_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// GET without cookies. Returns the body when the load succeeds.
pub fn get(url: &str) -> Option<Vec<u8>> {
    load(url, "Copycraft", None)
}

/// GET a preview image with a browser user agent (capped at `MAX_BODY_BYTES`).
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
    let (data, response) = send(&request)?;
    let status = response
        .and_then(|response| response.downcast::<NSHTTPURLResponse>().ok())
        .map(|http| http.statusCode());
    if let Err(why) = check_response(status, data.length(), MAX_BODY_BYTES) {
        eprintln!("fetch skipped: {why}");
        return None;
    }
    Some(data.to_vec())
}

/// Only a 2xx HTTP response with a body within `cap` is used. Without an HTTP response (no
/// status) the body is refused too.
fn check_response(status: Option<isize>, len: usize, cap: usize) -> Result<(), String> {
    match status {
        Some(code) if (200..300).contains(&code) => {}
        Some(code) => return Err(format!("HTTP status {code}")),
        None => return Err("not an HTTP response".into()),
    }
    if len > cap {
        return Err(format!("body of {len} bytes is over the {cap} byte limit"));
    }
    Ok(())
}

#[allow(deprecated)]
fn send(
    request: &NSMutableURLRequest,
) -> Option<(Retained<NSData>, Option<Retained<NSURLResponse>>)> {
    let mut response = None;
    let data = NSURLConnection::sendSynchronousRequest_returningResponse_error(
        request,
        Some(&mut response),
    )
    .ok()?;
    Some((data, response))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{FETCH_ATTEMPTS, MAX_BODY_BYTES, SKIP_SEND, check_response};

    #[test]
    fn only_2xx_within_the_cap_is_accepted() {
        assert!(check_response(Some(200), 10, MAX_BODY_BYTES).is_ok());
        assert!(check_response(Some(206), MAX_BODY_BYTES, MAX_BODY_BYTES).is_ok());
        assert!(check_response(Some(204), 0, MAX_BODY_BYTES).is_ok());
        for code in [101, 301, 304, 404, 416, 500, 503] {
            assert!(
                check_response(Some(code), 10, MAX_BODY_BYTES).is_err(),
                "{code}"
            );
        }
        assert!(check_response(None, 10, MAX_BODY_BYTES).is_err());
        assert!(check_response(Some(200), MAX_BODY_BYTES + 1, MAX_BODY_BYTES).is_err());
    }

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
