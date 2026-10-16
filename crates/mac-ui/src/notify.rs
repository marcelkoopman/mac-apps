//! User notifications through the UserNotifications framework (listed as the app under System
//! Settings > Notifications). Only from an app bundle: without a bundle identifier (for example
//! `cargo run`) `UNUserNotificationCenter` raises an Objective-C exception, so [`send`] sends
//! nothing then and says so. The shared layer starts no subprocesses (AGENTS.md); an app that
//! wants a fallback without a bundle keeps it itself (ticker uses `osascript`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, define_class, msg_send};
use objc2_foundation::{NSArray, NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSettings, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

static DELEGATE: Once = Once::new();
static REQUESTED: Once = Once::new();
static SENT: AtomicU64 = AtomicU64::new(0);
/// Called with the identifier of a notification the user clicked.
static CLICKED: Mutex<Option<fn(&str)>> = Mutex::new(None);

/// What the user decided about notifications for this app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// The user was not asked yet.
    NotDetermined,
    /// The user refused (or turned them off in System Settings).
    Denied,
    /// Notifications may be shown.
    Granted,
}

impl Permission {
    fn from_status(status: UNAuthorizationStatus) -> Self {
        if status == UNAuthorizationStatus::NotDetermined {
            Self::NotDetermined
        } else if status == UNAuthorizationStatus::Denied {
            Self::Denied
        } else {
            // Authorized, provisional and ephemeral all deliver.
            Self::Granted
        }
    }
}

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

        /// The user clicked a notification: hand its identifier (never its content) to the
        /// handler from [`set_click_handler`]. Runs on a system queue, not the main thread.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            let handler = *CLICKED.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(handler) = handler {
                handler(&id);
            }
            completion_handler.call(());
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

/// Install the delegate that also shows notifications while the app is frontmost and reports
/// clicks. Once; without a bundle it does nothing.
fn install_delegate() {
    if !is_bundled() {
        return;
    }
    DELEGATE.call_once(|| {
        let delegate = Delegate::new();
        UNUserNotificationCenter::currentNotificationCenter()
            .setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The center holds its delegate weakly; keep this one for the rest of the process.
        let _ = Retained::into_raw(delegate);
    });
}

/// Ask for permission to show alerts and play sounds, and install the delegate that also shows
/// notifications while the app is frontmost. Call once at startup; later calls (and [`send`],
/// which calls it too) do nothing. Returns at once: macOS asks the user only the first time,
/// and the answer arrives on a background queue and is not reported (see [`request_permission`]
/// for that). Without a bundle it does nothing.
pub fn request_authorization() {
    install_delegate();
    if !is_bundled() {
        return;
    }
    REQUESTED.call_once(|| {
        ask(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            None,
        )
    });
}

fn ask(options: UNAuthorizationOptions, done: Option<Box<dyn Fn(bool) + Send>>) {
    let handler = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        if let Some(done) = &done {
            done(granted.as_bool());
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(options, &handler);
}

/// Ask for permission to show alerts (no sound), now, and report the answer to `done` on a
/// background queue. macOS shows its question only while the state is
/// [`Permission::NotDetermined`]; otherwise `done` gets the stored answer. For an app that asks
/// only when it first needs a notification, instead of at startup. Without a bundle `done` gets
/// `false` at once.
pub fn request_permission(done: impl Fn(bool) + Send + 'static) {
    if !is_bundled() {
        done(false);
        return;
    }
    install_delegate();
    ask(UNAuthorizationOptions::Alert, Some(Box::new(done)));
}

/// The current permission, reported to `done` on a background queue. Never asks the user.
/// Without a bundle `done` gets [`Permission::Denied`] at once.
pub fn permission(done: impl Fn(Permission) + Send + 'static) {
    if !is_bundled() {
        done(Permission::Denied);
        return;
    }
    let handler = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        // SAFETY: the system passes a valid settings object for the duration of the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        done(Permission::from_status(status));
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .getNotificationSettingsWithCompletionHandler(&handler);
}

/// Set the function called with the identifier of a notification the user clicked, on a system
/// queue (hand over to the main thread yourself). Installs the delegate.
pub fn set_click_handler(handler: fn(&str)) {
    *CLICKED.lock().unwrap_or_else(PoisonError::into_inner) = Some(handler);
    install_delegate();
}

/// Remove the notification `id` from Notification Center, and cancel it when it is still
/// pending. Does nothing for an unknown id or without a bundle.
pub fn remove(id: &str) {
    if !is_bundled() {
        return;
    }
    let ids = NSArray::from_slice(&[&*NSString::from_str(id)]);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.removeDeliveredNotificationsWithIdentifiers(&ids);
    center.removePendingNotificationRequestsWithIdentifiers(&ids);
}

/// Post a notification with `title` and `body` (no sound), shown right away. Does not wait and
/// does not report failures, for example when the user turned notifications off. Returns
/// `false`, having sent nothing, when the process does not run from an app bundle.
pub fn send(title: &str, body: &str) -> bool {
    if !is_bundled() {
        return false;
    }
    request_authorization();
    post(&unique_id(), title, body)
}

/// Post a notification under `id` (a new one with the same `id` replaces the old), shown right
/// away. Asks nothing: with the permission not yet decided macOS holds back the banner, so ask
/// first ([`request_permission`]). Returns `false`, having sent nothing, without an app bundle.
pub fn post(id: &str, title: &str, body: &str) -> bool {
    if !is_bundled() {
        return false;
    }
    install_delegate();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(id),
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

    #[test]
    fn the_status_maps_to_a_permission() {
        let of = |status| Permission::from_status(status);
        assert_eq!(
            of(UNAuthorizationStatus::NotDetermined),
            Permission::NotDetermined
        );
        assert_eq!(of(UNAuthorizationStatus::Denied), Permission::Denied);
        for granted in [
            UNAuthorizationStatus::Authorized,
            UNAuthorizationStatus::Provisional,
            UNAuthorizationStatus::Ephemeral,
        ] {
            assert_eq!(of(granted), Permission::Granted);
        }
    }
}
