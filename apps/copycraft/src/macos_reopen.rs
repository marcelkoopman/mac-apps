//! The way back to CopyCraft when its menu bar icon is hidden (macOS lets you hide a menu bar
//! extra, and a crowded menu bar hides it behind the notch): opening CopyCraft again from
//! Finder, Launchpad or Spotlight while it runs opens the card in the middle of the screen.
//!
//! LaunchServices sends a running app the reopen Apple event (`kAEReopenApplication`) for that.
//! AppKit's own handler only asks the app delegate (`applicationShouldHandleReopen:…`), which
//! is winit's and does not answer it, so CopyCraft handles the event itself.

use std::cell::RefCell;

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::AnyObject;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_foundation::{NSAppleEventManager, NSObject};

use crate::launcher::{self, UserEvent};

/// `kCoreEventClass`, `'aevt'`.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
/// `kAEReopenApplication`, `'rapp'`.
const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
/// `kAEQuitApplication`, `'quit'`.
const QUIT_APPLICATION: u32 = u32::from_be_bytes(*b"quit");

thread_local! {
    /// The event manager does not retain its handler.
    static HANDLER: RefCell<Option<Retained<ReopenHandler>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The handler only receives the reopen
    // event.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftReopenHandler"]
    struct ReopenHandler;

    impl ReopenHandler {
        #[unsafe(method(handleReopen:withReplyEvent:))]
        fn handle_reopen(&self, _event: Option<&AnyObject>, _reply: Option<&AnyObject>) {
            launcher::emit(UserEvent::Reopen);
        }

        /// The quit Apple event (Quit in the Dock or Activity Monitor, log out, shut down).
        #[unsafe(method(handleQuit:withReplyEvent:))]
        fn handle_quit(&self, _event: Option<&AnyObject>, _reply: Option<&AnyObject>) {
            quit();
        }

        /// Quit ⌘Q in the app menu (winit's item, aimed here by `macos_main_menu`).
        #[unsafe(method(terminate:))]
        fn terminate(&self, _sender: Option<&AnyObject>) {
            quit();
        }
    }
);

/// Quit the way the menu bar icon's Quit does: the event loop ends and the app returns from
/// `main`. Not `-[NSApplication terminate:]`, whose `applicationWillTerminate:` in winit
/// closes every window of the app, AppKit's own included, while the process is going away.
fn quit() {
    launcher::emit(UserEvent::Run(crate::commands::CommandId::Quit));
}

/// The handler as a menu item target (it answers `terminate:`).
pub fn quit_target() -> Option<Retained<NSObject>> {
    HANDLER.with(|slot| slot.borrow().clone().map(Retained::into_super))
}

/// Handle the reopen and quit events from now on. Runs once, after launch; a second call does nothing.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if HANDLER.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let manager = NSAppleEventManager::sharedAppleEventManager();
    // SAFETY: NSObject's initialiser on a fresh instance; then
    // `setEventHandler:andSelector:forEventClass:andEventID:` (twice) with a handler that
    // implements each selector (two object arguments, no result) and is kept alive in HANDLER for the
    // process, and two four-character codes (`AEEventClass` and `AEEventID` are `u32`).
    let handler: Retained<ReopenHandler> = unsafe {
        let handler: Retained<ReopenHandler> = msg_send![ReopenHandler::alloc(mtm), init];
        let object: &AnyObject = handler.as_ref();
        let _: () = msg_send![
            &manager,
            setEventHandler: object,
            andSelector: sel!(handleReopen:withReplyEvent:),
            forEventClass: CORE_EVENT_CLASS,
            andEventID: REOPEN_APPLICATION
        ];
        let _: () = msg_send![
            &manager,
            setEventHandler: object,
            andSelector: sel!(handleQuit:withReplyEvent:),
            forEventClass: CORE_EVENT_CLASS,
            andEventID: QUIT_APPLICATION
        ];
        handler
    };
    HANDLER.with(|slot| *slot.borrow_mut() = Some(handler));
}
