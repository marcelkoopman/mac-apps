//! User notifications through the UserNotifications framework (listed as the app under System
//! Settings > Notifications). Only from an app bundle: without a bundle identifier (for example
//! `cargo run`) `UNUserNotificationCenter` raises an Objective-C exception, so [`send`] sends
//! nothing then and says so. The shared layer starts no subprocesses (AGENTS.md); an app that
//! wants a fallback without a bundle keeps it itself (ticker uses `osascript`).

use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

static SETUP: Once = Once::new();
static SENT: AtomicU64 = AtomicU64::new(0);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` has no ivars or Drop.
    #[unsafe(super(NSObject))]
    #[name = "MacUiNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// Also show banners while the app is frontmost (for example right after one of its
        /// dialogs), as `display notification` did.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        // SAFETY: plain NSObject initialiser.
        unsafe { msg_send![Self::alloc(), init] }
    }
}

/// Whether the process runs from an app bundle (has a bundle identifier), so notifications use
/// the UserNotifications framework.
pub fn is_bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Ask for permission to show alerts and play sounds, and install the delegate that also shows
/// notifications while the app is frontmost. Call once at startup; later calls (and [`send`],
/// which calls it too) do nothing. Returns at once: macOS asks the user only the first time,
/// and the answer arrives on a background queue and is not reported. Without a bundle it does
/// nothing.
pub fn request_authorization() {
    if !is_bundled() {
        return;
    }
    SETUP.call_once(|| {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = Delegate::new();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The center holds its delegate weakly; keep this one for the rest of the process.
        let _ = Retained::into_raw(delegate);
        let done = RcBlock::new(|_granted: Bool, _error: *mut NSError| {});
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &done,
        );
    });
}

/// Post a notification with `title` and `body` (no sound), shown right away. Does not wait and
/// does not report failures, for example when the user turned notifications off. Returns
/// `false`, having sent nothing, when the process does not run from an app bundle.
pub fn send(title: &str, body: &str) -> bool {
    if !is_bundled() {
        return false;
    }
    request_authorization();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&unique_id()),
        &content,
        None,
    );
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
    true
}

/// Unique per process and per call; requests with the same identifier replace each other.
fn unique_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = SENT.fetch_add(1, Ordering::Relaxed);
    format!("mac-ui-{}-{nanos}-{count}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_differ_per_call() {
        assert_ne!(unique_id(), unique_id());
    }
}
