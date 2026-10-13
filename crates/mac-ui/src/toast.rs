//! A short confirmation toast: a green check (SF Symbol `checkmark.circle.fill`) and a label in
//! a small glass panel, held about 1.2 s and faded out. VoiceOver announces the text.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{NSAccessibility, NSColor, NSImageView, NSTextField, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::panel::{self, NearCursor};
use crate::style::{FONT_BODY, RADIUS_TOAST, SPACE_M, SPACE_S, TOAST_FADE_SECS, TOAST_HOLD_SECS};

const HEIGHT: f64 = 36.0;
const SYMBOL: f64 = 18.0;
const PLACE: NearCursor = NearCursor {
    margin: 16.0,
    lead_x: 0.0,
    gap_y: 24.0,
};

struct Toast {
    window: Retained<NSWindow>,
    label: Retained<NSTextField>,
    check: Retained<NSImageView>,
}

thread_local! {
    static TOAST: RefCell<Option<Toast>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Width of the toast for a label `text_width` points wide.
pub fn width_for(text_width: f64) -> f64 {
    SPACE_M + SYMBOL + SPACE_S + text_width.ceil() + SPACE_M
}

fn build(mtm: MainThreadMarker) -> Option<Toast> {
    let window: Retained<NSWindow> =
        panel::borderless(NSWindow::alloc(mtm), NSSize::new(200.0, HEIGHT));
    panel::configure_floating(&window);
    window.setIgnoresMouseEvents(true);
    window.setMovableByWindowBackground(false);
    let view = window.contentView()?;
    let background = panel::rounded_glass(mtm, &view, RADIUS_TOAST);
    let check = crate::widgets::image_view(mtm);
    if let Some(image) = crate::icon::system_symbol("checkmark.circle.fill", "") {
        check.setImage(Some(&image));
    }
    check.setContentTintColor(Some(&NSColor::systemGreenColor()));
    check.setAccessibilityElement(false);
    let label = crate::widgets::label(mtm, FONT_BODY, &NSColor::labelColor());
    background.content.addSubview(&check);
    background.content.addSubview(&label);
    Some(Toast {
        window,
        label,
        check,
    })
}

/// Show `text` with the green check near the pointer, and fade it out after about 1.2 s. With
/// `announce`, VoiceOver also says it, at normal priority. A new toast replaces one still
/// showing.
pub fn show(mtm: MainThreadMarker, text: &str, announce: bool) {
    let generation = GENERATION.with(|g| {
        g.set(g.get().wrapping_add(1));
        g.get()
    });
    TOAST.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = build(mtm);
        }
        let Some(toast) = slot.as_ref() else {
            return;
        };
        toast.label.setStringValue(&NSString::from_str(text));
        toast.label.sizeToFit();
        let text_width = toast.label.frame().size.width;
        let size = NSSize::new(width_for(text_width), HEIGHT);
        toast
            .window
            .setFrame_display(panel::near_cursor(mtm, size, &PLACE), true);
        toast.check.setFrame(NSRect::new(
            NSPoint::new(SPACE_M, (HEIGHT - SYMBOL) / 2.0),
            NSSize::new(SYMBOL, SYMBOL),
        ));
        let label_height = toast.label.frame().size.height;
        toast.label.setFrameOrigin(NSPoint::new(
            SPACE_M + SYMBOL + SPACE_S,
            (HEIGHT - label_height) / 2.0,
        ));
        toast.window.setAlphaValue(0.0);
        toast.window.orderFrontRegardless();
        if announce && let Some(view) = toast.window.contentView() {
            crate::a11y::announce_at(&view, text, crate::a11y::Priority::Medium);
        }
        fade_window(&toast.window, 1.0, None);
    });
    let spawned = std::thread::Builder::new()
        .name("toast".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs_f64(TOAST_HOLD_SECS));
            crate::actions::run_on_main(move || hide(generation));
        });
    if spawned.is_err() {
        hide(generation);
    }
}

/// Apply the current Reduce Transparency setting to the toast, if it was built.
pub fn restyle(mtm: MainThreadMarker) {
    TOAST.with(|slot| {
        if let Some(toast) = slot.borrow().as_ref() {
            crate::glass::restyle_window(mtm, &toast.window, RADIUS_TOAST);
        }
    });
}

fn fade_window(window: &NSWindow, alpha: f64, done: Option<Box<dyn Fn()>>) {
    use objc2_app_kit::NSAnimatablePropertyContainer;
    let target = window.retain();
    crate::motion::animate(
        TOAST_FADE_SECS,
        move || target.animator().setAlphaValue(alpha),
        done,
    );
}

fn hide(generation: u64) {
    if GENERATION.with(Cell::get) != generation {
        return;
    }
    TOAST.with(|slot| {
        if let Some(toast) = slot.borrow().as_ref() {
            let window = toast.window.clone();
            fade_window(
                &toast.window,
                0.0,
                Some(Box::new(move || {
                    if GENERATION.with(Cell::get) == generation {
                        window.orderOut(None);
                    }
                })),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::width_for;

    #[test]
    fn width_fits_check_gap_and_text_with_padding() {
        assert_eq!(width_for(100.0), 12.0 + 18.0 + 8.0 + 100.0 + 12.0);
        assert_eq!(width_for(99.2), width_for(100.0));
    }
}
