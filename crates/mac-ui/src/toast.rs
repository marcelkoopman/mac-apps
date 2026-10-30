//! A short confirmation toast: a green check (SF Symbol `checkmark.circle.fill`) and a label in
//! a small glass panel, held about 1.2 s and faded out. VoiceOver announces the text. A
//! warning ([`show_warning`]) has an orange triangle (`exclamationmark.triangle.fill`), a
//! second, smaller line under the first, and stays about 3 s.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{NSAccessibility, NSColor, NSImageView, NSTextField, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::panel::{self, NearCursor};
use crate::style::{
    FONT_BODY, FONT_CAPTION, RADIUS_TOAST, SPACE_M, SPACE_S, TOAST_FADE_SECS, TOAST_HOLD_SECS,
    TOAST_WARNING_HOLD_SECS,
};

const HEIGHT: f64 = 36.0;
const SYMBOL: f64 = 18.0;
/// Between a warning's two lines.
const LINE_GAP: f64 = 2.0;
const PLACE: NearCursor = NearCursor {
    margin: 16.0,
    lead_x: 0.0,
    gap_y: 24.0,
};

struct Toast {
    window: Retained<NSWindow>,
    label: Retained<NSTextField>,
    /// A warning's second line; hidden on a confirmation.
    detail: Retained<NSTextField>,
    check: Retained<NSImageView>,
}

/// What the toast says: done (the green check) or a warning (the orange triangle).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    Done,
    Warning,
}

thread_local! {
    static TOAST: RefCell<Option<Toast>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Width of the toast for a label `text_width` points wide.
pub fn width_for(text_width: f64) -> f64 {
    SPACE_M + SYMBOL + SPACE_S + text_width.ceil() + SPACE_M
}

/// Height of the toast for a first line `title` points high and an optional second line
/// `detail` points high: one line keeps the usual height, two get their own (never less).
pub fn height_for(title: f64, detail: Option<f64>) -> f64 {
    match detail {
        None => HEIGHT,
        Some(detail) => (SPACE_S + title.ceil() + LINE_GAP + detail.ceil() + SPACE_S).max(HEIGHT),
    }
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
    let detail = crate::widgets::label(mtm, FONT_CAPTION, &NSColor::secondaryLabelColor());
    detail.setHidden(true);
    background.content.addSubview(&check);
    background.content.addSubview(&label);
    background.content.addSubview(&detail);
    Some(Toast {
        window,
        label,
        detail,
        check,
    })
}

/// Show `text` with the green check near the pointer, and fade it out after about 1.2 s. With
/// `announce`, VoiceOver also says it, at normal priority. A new toast replaces one still
/// showing.
pub fn show(mtm: MainThreadMarker, text: &str, announce: bool) {
    present(mtm, Look::Done, text, None, announce);
}

/// Show a warning near the pointer: `title`, with `detail` as a smaller second line, beside an
/// orange triangle, and fade it out after about 3 s. With `announce`, VoiceOver says both lines.
/// A new toast replaces one still showing.
pub fn show_warning(mtm: MainThreadMarker, title: &str, detail: &str, announce: bool) {
    present(mtm, Look::Warning, title, Some(detail), announce);
}

fn present(
    mtm: MainThreadMarker,
    look: Look,
    text: &str,
    detail_text: Option<&str>,
    announce: bool,
) {
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
        let (symbol, tint) = match look {
            Look::Done => ("checkmark.circle.fill", NSColor::systemGreenColor()),
            Look::Warning => (
                "exclamationmark.triangle.fill",
                NSColor::systemOrangeColor(),
            ),
        };
        if let Some(image) = crate::icon::system_symbol(symbol, "") {
            toast.check.setImage(Some(&image));
        }
        toast.check.setContentTintColor(Some(&tint));
        toast.label.setStringValue(&NSString::from_str(text));
        toast.label.sizeToFit();
        let title = toast.label.frame().size;
        let detail = detail_text.map(|detail| {
            toast.detail.setStringValue(&NSString::from_str(detail));
            toast.detail.sizeToFit();
            toast.detail.frame().size
        });
        toast.detail.setHidden(detail.is_none());
        let text_width = detail.map_or(title.width, |detail| title.width.max(detail.width));
        let height = height_for(title.height, detail.map(|detail| detail.height));
        let size = NSSize::new(width_for(text_width), height);
        toast
            .window
            .setFrame_display(panel::near_cursor(mtm, size, &PLACE), true);
        toast.check.setFrame(NSRect::new(
            NSPoint::new(SPACE_M, (height - SYMBOL) / 2.0),
            NSSize::new(SYMBOL, SYMBOL),
        ));
        let x = SPACE_M + SYMBOL + SPACE_S;
        match detail {
            None => toast
                .label
                .setFrameOrigin(NSPoint::new(x, (height - title.height) / 2.0)),
            Some(detail) => {
                // Bottom-up: the second line at the bottom, the first above it.
                let bottom = (height - title.height - LINE_GAP - detail.height) / 2.0;
                toast.detail.setFrameOrigin(NSPoint::new(x, bottom));
                toast
                    .label
                    .setFrameOrigin(NSPoint::new(x, bottom + detail.height + LINE_GAP));
            }
        }
        toast.window.setAlphaValue(0.0);
        toast.window.orderFrontRegardless();
        if announce && let Some(view) = toast.window.contentView() {
            let spoken = match detail_text {
                Some(detail) => format!("{text}. {detail}"),
                None => text.to_string(),
            };
            crate::a11y::announce_at(&view, &spoken, crate::a11y::Priority::Medium);
        }
        fade_window(&toast.window, 1.0, None);
    });
    let hold = match look {
        Look::Done => TOAST_HOLD_SECS,
        Look::Warning => TOAST_WARNING_HOLD_SECS,
    };
    let spawned = std::thread::Builder::new()
        .name("toast".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs_f64(hold));
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
    use super::{HEIGHT, height_for, width_for};

    #[test]
    fn width_fits_check_gap_and_text_with_padding() {
        assert_eq!(width_for(100.0), 12.0 + 18.0 + 8.0 + 100.0 + 12.0);
        assert_eq!(width_for(99.2), width_for(100.0));
    }

    #[test]
    fn a_second_line_makes_room_for_both() {
        assert_eq!(height_for(16.0, None), HEIGHT);
        assert_eq!(height_for(16.0, Some(15.0)), 8.0 + 16.0 + 2.0 + 15.0 + 8.0);
        assert_eq!(height_for(15.4, Some(14.2)), height_for(16.0, Some(15.0)));
        // Never smaller than one line.
        assert_eq!(height_for(4.0, Some(4.0)), HEIGHT);
    }
}
