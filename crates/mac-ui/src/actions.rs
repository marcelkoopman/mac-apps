//! Safe target/action wiring: controls, menu items and delayed performs aimed at an app delegate.
//!
//! AppKit sends an action as `-action:(id)sender` and a delayed perform as `-selector:(id)object`.
//! Each helper first checks with `respondsToSelector:` that the target implements the selector
//! and skips the call (returning `false`) when it does not, so a wrong selector is a no-op rather
//! than an "unrecognized selector" crash. Controls and menu items hold their target weakly and a
//! delayed perform retains it, so no dangling target is left behind either.
//!
//! The selector's method must take at most one object argument and return void. The apps'
//! `define_class!` delegates declare their action methods that way (`fn x(&self, sender:
//! Option<&NSButton>)`), which is what the `SAFETY` comments below rely on.

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSButton, NSControl, NSMenuItem};
use objc2_foundation::{NSObjectNSDelayedPerforming, NSOperationQueue, NSString};

/// Send `action` to `target` when `control` is clicked. `false` (and nothing set) when `target`
/// does not implement `action`.
pub fn connect(control: &NSControl, target: &NSObject, action: Sel) -> bool {
    if !target.respondsToSelector(action) {
        return false;
    }
    // SAFETY: `target` implements `action` (checked above) as a one-argument action method, and
    // the control holds it weakly, so a released target is never messaged.
    unsafe {
        control.setTarget(Some(target));
        control.setAction(Some(action));
    }
    true
}

/// Aim `item`'s action at `target`. `false` (and nothing set) when the item has no action or
/// `target` does not implement it.
pub fn set_menu_target(item: &NSMenuItem, target: &NSObject) -> bool {
    let Some(action) = item.action() else {
        return false;
    };
    if !target.respondsToSelector(action) {
        return false;
    }
    // SAFETY: `target` implements the item's action (checked above) as a one-argument action
    // method, and the item holds it weakly.
    unsafe { item.setTarget(Some(target)) };
    true
}

/// A menu item with `action` and no key equivalent. Until [`set_menu_target`] aims it, AppKit
/// sends the action only to a responder that implements it.
pub fn menu_item_with_action(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
) -> Retained<NSMenuItem> {
    // SAFETY: a freshly allocated item with valid strings. AppKit validates the action against
    // the target (set later through `set_menu_target`, which checks it) or the responder chain
    // before sending it.
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(""),
        )
    }
}

/// A checkbox with `title` and no target or action yet (see [`connect`]).
pub fn checkbox(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    titled_button(mtm, title, true)
}

/// A push button with `title` and no target or action yet (see [`connect`]).
pub fn push_button(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    titled_button(mtm, title, false)
}

fn titled_button(mtm: MainThreadMarker, title: &str, checkbox: bool) -> Retained<NSButton> {
    let title = NSString::from_str(title);
    // SAFETY: no target or action is passed, so no weak reference has to stay valid.
    unsafe {
        if checkbox {
            NSButton::checkboxWithTitle_target_action(&title, None, None, mtm)
        } else {
            NSButton::buttonWithTitle_target_action(&title, None, None, mtm)
        }
    }
}

/// Send `selector` (with a nil argument) to `target` after `delay` seconds on this run loop.
/// `false` (and nothing scheduled) when `target` does not implement it.
pub fn perform_after(target: &NSObject, selector: Sel, delay: f64) -> bool {
    if !target.respondsToSelector(selector) {
        return false;
    }
    // SAFETY: `target` implements `selector` (checked above) as a method taking one object, here
    // nil. The run loop retains `target` until the perform has run or been cancelled.
    unsafe { target.performSelector_withObject_afterDelay(selector, None, delay) };
    true
}

/// Cancel the pending [`perform_after`] calls of `selector` (nil argument) on `target`.
pub fn cancel_perform(target: &NSObject, selector: Sel) {
    // SAFETY: only removes requests the run loop holds for `target`; nothing is sent.
    unsafe {
        NSObject::cancelPreviousPerformRequestsWithTarget_selector_object(target, selector, None)
    };
}

/// Run `f` once on the main thread, from any thread. The main queue is also served while a
/// modal alert runs, so a background result can reach an open [`crate::dialog::form`].
pub fn run_on_main(f: impl FnOnce() + Send + 'static) {
    let once = std::sync::Mutex::new(Some(f));
    let block: block2::RcBlock<dyn Fn()> = block2::RcBlock::new(move || {
        let f = once.lock().ok().and_then(|mut slot| slot.take());
        if let Some(f) = f {
            f();
        }
    });
    // SAFETY: the block holds only `f`, which is `Send`, behind a Mutex, so it may run on the
    // main queue's thread as `addOperationWithBlock:` requires; the queue retains the block
    // until it has run.
    unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&block) };
}
