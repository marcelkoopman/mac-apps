//! Short animations with the timings from [`crate::style`]: the card window fades and grows in,
//! fades out, and content crossfades. Reduce Motion keeps the fades and drops the movement.

use std::cell::Cell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::Message;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSView, NSWindow, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::style::{Motion, scaled_about_centre};

thread_local! {
    /// Bumped by every window animation, so a fade-out that finishes after the window was shown
    /// again does not hide it.
    static WINDOW_GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// System setting "Reduce motion".
pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// System setting "Reduce transparency".
pub fn reduce_transparency() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

fn next_generation() -> u64 {
    WINDOW_GENERATION.with(|generation| {
        let next = generation.get().wrapping_add(1);
        generation.set(next);
        next
    })
}

fn is_current(generation: u64) -> bool {
    WINDOW_GENERATION.with(Cell::get) == generation
}

/// Run `changes` in an animation group of `secs`, then `done`.
pub(crate) fn animate(secs: f64, changes: impl Fn() + 'static, done: Option<Box<dyn Fn()>>) {
    let changes: RcBlock<dyn Fn(NonNull<NSAnimationContext>)> =
        RcBlock::new(move |_context: NonNull<NSAnimationContext>| {
            NSAnimationContext::currentContext().setDuration(secs);
            changes();
        });
    let done: Option<RcBlock<dyn Fn()>> = done.map(RcBlock::new);
    NSAnimationContext::runAnimationGroup_completionHandler(&changes, done.as_deref());
}

fn scaled(frame: NSRect, scale: f64) -> NSRect {
    let (x, y, width, height) = scaled_about_centre(
        (
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        ),
        scale,
    );
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

/// Drop a pending fade-out's completion: the caller orders the window out itself now.
pub fn cancel_window_out() {
    next_generation();
}

/// Whether `window` is hidden or still fading out, so showing it should animate it in.
pub fn window_needs_in(window: &NSWindow) -> bool {
    !window.isVisible() || window.alphaValue() < 0.99
}

/// Fade `window` in (150 ms) while it grows from 0.96 to its frame; Reduce Motion: fade only.
/// Call after the window has its final frame and is on screen.
pub fn window_in(window: &NSWindow) {
    next_generation();
    let motion = Motion::card_in(reduce_motion());
    let full = window.frame();
    window.setAlphaValue(0.0);
    if motion.moves() {
        window.setFrame_display(scaled(full, motion.scale_from), false);
    }
    let target = window.retain();
    animate(
        motion.fade_secs,
        move || {
            let animator = target.animator();
            animator.setAlphaValue(1.0);
            if motion.moves() {
                animator.setFrame_display(full, true);
            }
        },
        None,
    );
}

/// Fade `window` out (100 ms), then run `done` (usually `orderOut`) and restore its alpha.
/// Skipped when the window was shown again in the meantime.
pub fn window_out(window: &NSWindow, done: impl Fn() + 'static) {
    let generation = next_generation();
    let motion = Motion::card_out(reduce_motion());
    let target = window.retain();
    let after = window.retain();
    animate(
        motion.fade_secs,
        move || target.animator().setAlphaValue(0.0),
        Some(Box::new(move || {
            if is_current(generation) {
                done();
                after.setAlphaValue(1.0);
            }
        })),
    );
}

/// Crossfade `view` to new content (120 ms): it dips and fades back to full. Allowed with
/// Reduce Motion, since nothing moves.
pub fn crossfade(view: &NSView) {
    let motion = Motion::crossfade(reduce_motion());
    if view.isHidden() || view.alphaValue() < 0.99 {
        return;
    }
    view.setAlphaValue(CROSSFADE_FROM);
    let target = view.retain();
    animate(
        motion.fade_secs,
        move || target.animator().setAlphaValue(1.0),
        None,
    );
}

/// Where a crossfade starts: enough of the old look to read as one continuous change.
const CROSSFADE_FROM: f64 = 0.35;
