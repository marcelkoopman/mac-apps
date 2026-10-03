//! The end of a session: the screen locks, the Mac goes to sleep, or another user takes over
//! (fast user switching). Copycraft then forgets its history, whatever the retention setting.

use std::cell::OnceCell;

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::AnyObject;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSWorkspace, NSWorkspaceScreensDidSleepNotification,
    NSWorkspaceSessionDidResignActiveNotification, NSWorkspaceWillSleepNotification,
};
use mac_ui::objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSObject, NSString,
};

/// Posted by loginwindow when the screen locks (no public constant).
const SCREEN_LOCKED: &str = "com.apple.screenIsLocked";

thread_local! {
    static HOOK: OnceCell<fn()> = const { OnceCell::new() };
    /// The notification centers do not retain their observers: kept for the app's lifetime.
    static OBSERVER: OnceCell<Retained<SessionObserver>> = const { OnceCell::new() };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftSessionObserver"]
    struct SessionObserver;

    impl SessionObserver {
        #[unsafe(method(sessionEnded:))]
        fn session_ended(&self, _note: &NSNotification) {
            HOOK.with(|hook| {
                if let Some(hook) = hook.get() {
                    hook();
                }
            });
        }
    }
);

/// Run `hook` (on the main thread) when the screen locks, the Mac sleeps or the user switches.
pub fn observe(hook: fn()) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    HOOK.with(|slot| {
        let _ = slot.set(hook);
    });
    OBSERVER.with(|slot| {
        if slot.get().is_some() {
            return;
        }
        let observer: Retained<SessionObserver> =
            unsafe { msg_send![SessionObserver::alloc(mtm), init] };
        let target: &AnyObject = &observer;
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        unsafe {
            for name in [
                NSWorkspaceWillSleepNotification,
                NSWorkspaceScreensDidSleepNotification,
                NSWorkspaceSessionDidResignActiveNotification,
            ] {
                workspace.addObserver_selector_name_object(
                    target,
                    sel!(sessionEnded:),
                    Some(name),
                    None,
                );
            }
            NSDistributedNotificationCenter::defaultCenter().addObserver_selector_name_object(
                target,
                sel!(sessionEnded:),
                Some(&NSString::from_str(SCREEN_LOCKED)),
                None,
            );
        }
        let _ = slot.set(observer);
    });
}
