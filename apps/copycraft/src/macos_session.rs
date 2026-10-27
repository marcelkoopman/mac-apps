//! The end of a session: the screen locks, the Mac goes to sleep, or another user takes over
//! (fast user switching). CopyCraft then forgets its history, whatever the retention setting.

use std::cell::OnceCell;

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::AnyObject;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
    NSWorkspaceDidWakeNotification, NSWorkspaceScreensDidSleepNotification,
    NSWorkspaceSessionDidResignActiveNotification, NSWorkspaceWillSleepNotification,
};
use mac_ui::objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSObject, NSString,
};

/// Posted by loginwindow when the screen locks (no public constant).
const SCREEN_LOCKED: &str = "com.apple.screenIsLocked";
/// Posted by the Text Input Sources service when another input source is selected
/// (`kTISNotifySelectedKeyboardInputSourceChanged`).
const INPUT_SOURCE_CHANGED: &str = "com.apple.Carbon.TISNotifySelectedKeyboardInputSourceChanged";

thread_local! {
    static HOOK: OnceCell<fn()> = const { OnceCell::new() };
    static DISPLAY_HOOK: OnceCell<fn()> = const { OnceCell::new() };
    static KEYBOARD_HOOK: OnceCell<fn()> = const { OnceCell::new() };
    /// The notification centers do not retain their observers: kept for the app's lifetime.
    static OBSERVER: OnceCell<Retained<SessionObserver>> = const { OnceCell::new() };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftSessionObserver"]
    struct SessionObserver;

    impl SessionObserver {
        #[unsafe(method(keyboardMayHaveChanged:))]
        fn keyboard_may_have_changed(&self, _note: &NSNotification) {
            KEYBOARD_HOOK.with(|hook| {
                if let Some(hook) = hook.get() {
                    hook();
                }
            });
        }

        #[unsafe(method(displayOptionsChanged:))]
        fn display_options_changed(&self, _note: &NSNotification) {
            DISPLAY_HOOK.with(|hook| {
                if let Some(hook) = hook.get() {
                    hook();
                }
            });
        }

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

/// Run `hook` (on the main thread) when the screen locks, the Mac sleeps or the user switches,
/// `display` when Reduce Transparency, Increase Contrast or Reduce Motion change, and
/// `keyboard` when the Mac wakes or the input source changes (another keyboard may be in use).
pub fn observe(hook: fn(), display: fn(), keyboard: fn()) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    HOOK.with(|slot| {
        let _ = slot.set(hook);
    });
    DISPLAY_HOOK.with(|slot| {
        let _ = slot.set(display);
    });
    KEYBOARD_HOOK.with(|slot| {
        let _ = slot.set(keyboard);
    });
    OBSERVER.with(|slot| {
        if slot.get().is_some() {
            return;
        }
        // SAFETY: `init` on a freshly allocated instance of a class that declares no other
        // initializer.
        let observer: Retained<SessionObserver> =
            unsafe { msg_send![SessionObserver::alloc(mtm), init] };
        let target: &AnyObject = &observer;
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: the observer is kept in OBSERVER for the life of the process (notification
        // centers do not retain observers) and implements `sessionEnded:`.
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
            workspace.addObserver_selector_name_object(
                target,
                sel!(displayOptionsChanged:),
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
            );
            workspace.addObserver_selector_name_object(
                target,
                sel!(keyboardMayHaveChanged:),
                Some(NSWorkspaceDidWakeNotification),
                None,
            );
            let distributed = NSDistributedNotificationCenter::defaultCenter();
            distributed.addObserver_selector_name_object(
                target,
                sel!(sessionEnded:),
                Some(&NSString::from_str(SCREEN_LOCKED)),
                None,
            );
            distributed.addObserver_selector_name_object(
                target,
                sel!(keyboardMayHaveChanged:),
                Some(&NSString::from_str(INPUT_SOURCE_CHANGED)),
                None,
            );
        }
        let _ = slot.set(observer);
    });
}
