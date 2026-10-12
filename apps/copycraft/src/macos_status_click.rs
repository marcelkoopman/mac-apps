//! Left click on the menu-bar icon opens or closes the card.
//!
//! `tray-icon` lays a view over the status button and waits for that view's mouse-up. On this
//! macOS a real click is delivered to the status button, not to a subview, so the click hit the
//! overlay and stopped there. The overlay is taken off the button. The button's action opens
//! the card on left click and the status menu on right click.

use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::time::{Duration, Instant};

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::AnyObject;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{NSEvent, NSEventMask, NSStatusBarButton};
use mac_ui::objc2_foundation::NSObject;
use mac_ui::tray_icon::TrayIcon;

use crate::launcher::{self, UserEvent};

/// Class `tray-icon` adds over the status button. It stays alive (the crate holds it) but is
/// not in the button, so a click reaches the button.
const TRAY_TARGET: &CStr = c"TaoTrayTarget";
/// `+[NSEvent pressedMouseButtons]`: bit 1 is the right button.
const RIGHT_BUTTON: u64 = 2;
/// A right click sends the button action twice before the menu handler runs.
const CLICK_GAP: Duration = Duration::from_millis(50);

thread_local! {
    /// The button does not retain its action target.
    static TARGET: RefCell<Option<Retained<StatusClick>>> = const { RefCell::new(None) };
    /// `show_menu` calls `performClick:`, which sends this action again. Ignore that one.
    static SHOWING_MENU: Cell<bool> = const { Cell::new(false) };
    static LAST_CLICK: Cell<Option<Instant>> = const { Cell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The target only receives `click:`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftStatusClick"]
    struct StatusClick;

    impl StatusClick {
        #[unsafe(method(click:))]
        fn click(&self, _sender: Option<&AnyObject>) {
            if SHOWING_MENU.with(Cell::get) || !accept_click(Instant::now()) {
                return;
            }
            if right_button_down() {
                launcher::emit(UserEvent::ShowStatusMenu);
            } else {
                launcher::emit(UserEvent::ToggleUnderIcon);
            }
        }
    }
);

/// Point the status button's action at the card toggle, and keep `tray-icon`'s overlay from
/// taking the click.
///
/// Safe to call again. A right-click menu assigns the button's menu for the tracking session
/// and clears the action afterwards.
pub fn install(tray: &TrayIcon) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(button) = tray.ns_status_item().and_then(|item| item.button(mtm)) else {
        return;
    };
    lift_tray_overlay(&button);
    TARGET.with(|slot| {
        let mut slot = slot.borrow_mut();
        let target = slot.get_or_insert_with(|| new_target(mtm));
        let action = sel!(click:);
        let object: &AnyObject = target.as_ref();
        let aimed =
            button.action() == Some(action) && same_ptr(button.target().as_deref(), Some(object));
        if aimed {
            return;
        }
        // SAFETY: TARGET keeps `target` for the process, and it implements `click:`.
        unsafe {
            button.setTarget(Some(target.as_ref()));
            button.setAction(Some(action));
        }
        let _ = button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseDown);
    });
}

/// `show_menu` is about to call `performClick:` (or just finished). The button action must not
/// also toggle the card.
pub fn set_menu_action(showing: bool) {
    SHOWING_MENU.set(showing);
}

fn new_target(mtm: MainThreadMarker) -> Retained<StatusClick> {
    // SAFETY: NSObject's initialiser.
    unsafe { msg_send![StatusClick::alloc(mtm), init] }
}

fn lift_tray_overlay(button: &NSStatusBarButton) {
    for sub in &button.subviews() {
        if sub.class().name() == TRAY_TARGET {
            sub.removeFromSuperview();
        }
    }
}

/// The first click in a 50 ms gap counts. A second delivery of the same click does not.
fn accept_click(now: Instant) -> bool {
    LAST_CLICK.with(|slot| {
        let last = slot.get();
        if last.is_some_and(|at| now.saturating_duration_since(at) < CLICK_GAP) {
            return false;
        }
        slot.set(Some(now));
        true
    })
}

fn right_button_down() -> bool {
    let pressed: u64 = NSEvent::pressedMouseButtons().try_into().unwrap_or(0);
    pressed & RIGHT_BUTTON != 0
}

fn same_ptr<T: ?Sized, U: ?Sized>(left: Option<&T>, right: Option<&U>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => std::ptr::eq(
            std::ptr::from_ref(left).cast::<()>(),
            std::ptr::from_ref(right).cast::<()>(),
        ),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{CLICK_GAP, LAST_CLICK, accept_click};

    #[test]
    fn a_second_status_click_inside_the_gap_is_ignored() {
        LAST_CLICK.with(|slot| slot.set(None));
        let now = std::time::Instant::now();
        assert!(accept_click(now));
        assert!(!accept_click(now));
        assert!(!accept_click(
            now + CLICK_GAP - std::time::Duration::from_millis(1)
        ));
        assert!(accept_click(now + CLICK_GAP));
    }
}
