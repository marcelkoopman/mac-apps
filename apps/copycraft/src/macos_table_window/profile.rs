//! The table window's profile (*Profile* on the toolbar): the top three findings and a short
//! list per column of the version shown ([`crate::table_profile`]), over the sidebar and grid,
//! with *Copy Profile* and *Table* to go back (as Esc). The app works it out on a thread; a
//! result for an older request is dropped, and a new version (sorted, deduplicated, other
//! columns) is profiled again. Memory only: never in history or on disk. Masked like the grid:
//! hidden under the cover. Closing the window wipes the text. Nothing here is logged.

use std::sync::Arc;

use super::*;
use crate::table_profile::{ProfileOutcome, TableProfile};
use mac_ui::objc2_app_kit::{NSBezelStyle, NSCellImagePosition, NSImage};

const ROW_H: f64 = 26.0;
const NOTE_H: f64 = 18.0;
const GAP: f64 = 8.0;

struct Pane {
    pane: Retained<NSView>,
    scroll: Retained<NSScrollView>,
    text: Retained<NSTextView>,
    copy: Retained<NSButton>,
    back: Retained<NSButton>,
    /// "Making the profile…" while it is worked out.
    note: Retained<NSTextField>,
}

thread_local! {
    static PANE: RefCell<Option<Pane>> = const { RefCell::new(None) };
    /// The profile is up (instead of the grid).
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// The number of the last request; an outcome with another one is dropped.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    /// The profile shown (for the language), shared with the app's copy.
    static SHOWN_PROFILE: RefCell<Option<Arc<TableProfile>>> = const { RefCell::new(None) };
    /// The version the profile was asked for (table, version, versions, rows).
    static SIGNATURE: Cell<Option<(u64, usize, usize, usize)>> = const { Cell::new(None) };
}

/// Build the pane, hidden, over `frame` of `content` (the sidebar and grid). Call before the
/// shade and cover are added, so they stay on top.
pub(super) fn build(mtm: MainThreadMarker, content: &NSView, frame: NSRect) {
    use NSAutoresizingMaskOptions as A;
    let pane = NSView::initWithFrame(NSView::alloc(mtm), frame);
    pane.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    pane.setHidden(true);
    let fill = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    fill.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
    fill.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    pane.addSubview(&fill);
    let button = |symbol: &str, action: Sel, resize: &[A]| {
        let button = NSButton::initWithFrame(NSButton::alloc(mtm), NSRect::ZERO);
        button.setBezelStyle(NSBezelStyle::Push);
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        ) {
            button.setImage(Some(&image));
            button.setImagePosition(NSCellImagePosition::ImageLeading);
        }
        button.setAutoresizingMask(mask(resize));
        wire(&button, action);
        pane.addSubview(&button);
        button
    };
    let copy = button(
        "doc.on.doc",
        sel!(tableWindowProfileCopy:),
        &[A::ViewMinYMargin, A::ViewMaxXMargin],
    );
    let back = button(
        "tablecells",
        sel!(tableWindowProfileBack:),
        &[A::ViewMinYMargin, A::ViewMinXMargin],
    );
    let note = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    note.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    note.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewWidthSizable]));
    pane.addSubview(&note);
    let text = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    text.setFont(Some(&NSFont::systemFontOfSize(mac_ui::metrics::FONT_BODY)));
    text.setTextColor(Some(&NSColor::labelColor()));
    let scroll = widgets::passive_text_scroll(mtm, &text);
    scroll.setFrame(text_frame(&pane));
    scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    pane.addSubview(&scroll);
    content.addSubview(&pane);
    let built = Pane {
        pane,
        scroll,
        text,
        copy,
        back,
        note,
    };
    relabel_pane(&built);
    PANE.with(|slot| slot.replace(Some(built)));
}

/// The pane, for moving it with the grid when the version strip comes and goes.
pub(super) fn pane() -> Option<Retained<NSView>> {
    PANE.with(|slot| slot.borrow().as_ref().map(|pane| pane.pane.clone()))
}

/// Whether the profile is up.
pub(super) fn active() -> bool {
    ACTIVE.get()
}

/// *Profile* on the toolbar: show the profile, or go back to the table.
pub(super) fn toggle() {
    if ACTIVE.get() {
        leave();
    } else {
        enter();
    }
}

/// Esc: back to the table when the profile is up.
pub(super) fn escape() -> bool {
    if ACTIVE.get() {
        leave();
        true
    } else {
        false
    }
}

fn enter() {
    if PANE.with(|slot| slot.borrow().is_none()) {
        return;
    }
    let signature = SHOWN.with(|shown| {
        shown
            .borrow()
            .as_ref()
            .filter(|view| commands::table_window_profile_on(view))
            .map(signature_of)
    });
    let Some(signature) = signature else {
        NSBeep();
        return;
    };
    // One overlay at a time: the chart goes.
    chart::leave();
    ACTIVE.set(true);
    SIGNATURE.set(Some(signature));
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            pane.pane.setHidden(false);
        }
    });
    ask();
    apply_mask(masked());
}

/// Back to the table (*Table*, Esc, the chart, no table): the profile shown is wiped.
pub(super) fn leave() {
    ACTIVE.set(false);
    // An outcome still on its way is dropped.
    GENERATION.set(GENERATION.get().wrapping_add(1));
    SIGNATURE.set(None);
    SHOWN_PROFILE.with(|slot| slot.replace(None));
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            pane.pane.setHidden(true);
            widgets::wipe_text_view(&pane.text);
            widgets::wipe_text_field(&pane.note);
            pane.copy.setEnabled(false);
        }
    });
}

/// *Copy Profile*: the app copies the profile it keeps for this request.
pub(super) fn copy() {
    let ready = ACTIVE.get() && SHOWN_PROFILE.with(|slot| slot.borrow().is_some());
    if !ready || masked() {
        NSBeep();
        return;
    }
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::CopyProfile {
        generation: GENERATION.get(),
    }));
}

/// A new view in the window: a new version (sorted, deduplicated, other columns) is profiled
/// again once its grid is there; without a table the profile goes.
pub(super) fn view_changed(view: &TableWindowView) {
    if !ACTIVE.get() || view.working || view.grid.is_empty() {
        return;
    }
    if !commands::table_window_profile_on(view) {
        leave();
        return;
    }
    let signature = signature_of(view);
    if SIGNATURE.get() == Some(signature) {
        return;
    }
    SIGNATURE.set(Some(signature));
    ask();
}

/// Show what the profile thread worked out, unless it is for an older request.
pub(super) fn show(outcome: ProfileOutcome) {
    if !ACTIVE.get() || outcome.generation != GENERATION.get() {
        return;
    }
    let Some(profile) = outcome.profile else {
        leave();
        return;
    };
    SHOWN_PROFILE.with(|slot| slot.replace(Some(profile)));
    paint_profile();
}

/// Masked: the text is hidden (under the cover) and *Copy Profile* is off.
pub(super) fn apply_mask(masked: bool) {
    let ready = SHOWN_PROFILE.with(|slot| slot.borrow().is_some());
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            pane.scroll.setHidden(masked);
            pane.copy.setEnabled(!masked && ready);
        }
    });
}

/// The labels in the current language; an open profile is written again in it.
pub(super) fn relabel() {
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            relabel_pane(pane);
        }
    });
    if ACTIVE.get() && SHOWN_PROFILE.with(|slot| slot.borrow().is_some()) {
        paint_profile();
    }
}

/// The window is wiped: back to the table, the text emptied.
pub(super) fn wipe() {
    leave();
}

/// Write the profile shown into the text view, in the current language.
fn paint_profile() {
    let text = SHOWN_PROFILE.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|profile| profile.text(crate::locale::lang()))
    });
    let Some(text) = text else {
        return;
    };
    let hidden = masked();
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            widgets::wipe_text_view(&pane.text);
            pane.text.setString(&NSString::from_str(&text));
            widgets::fit_text_view(&pane.text, Some(pane.scroll.contentSize().width));
            widgets::wipe_text_field(&pane.note);
            pane.copy.setEnabled(!hidden);
        }
    });
}

fn relabel_pane(pane: &Pane) {
    let t = crate::locale::t;
    let s = |text: &str| NSString::from_str(text);
    pane.copy.setTitle(&s(t("profile_copy")));
    pane.copy.setToolTip(Some(&s(t("profile_copy_tip"))));
    pane.back.setTitle(&s(t("table_view")));
    pane.back
        .setToolTip(Some(&s(&commands::keyed_tip(t("chart_back_tip"), "Esc"))));
    pane.text
        .setAccessibilityLabel(Some(&s(t("table_profile"))));
    layout(pane);
}

/// Ask the app for the profile of the version shown, as a new generation.
fn ask() {
    let generation = GENERATION.get().wrapping_add(1);
    GENERATION.set(generation);
    SHOWN_PROFILE.with(|slot| slot.replace(None));
    PANE.with(|slot| {
        if let Some(pane) = slot.borrow().as_ref() {
            widgets::wipe_text_view(&pane.text);
            pane.copy.setEnabled(false);
            pane.note
                .setStringValue(&NSString::from_str(crate::locale::t("profile_working")));
        }
    });
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::Profile {
        generation,
    }));
}

/// *Copy Profile* at the left, *Table* at the right along the top; the note under them.
fn layout(pane: &Pane) {
    let size = pane.pane.frame().size;
    let top = size.height - PAD - ROW_H;
    pane.copy.sizeToFit();
    let copy_w = pane.copy.frame().size.width;
    pane.copy.setFrame(NSRect::new(
        NSPoint::new(PAD, top),
        NSSize::new(copy_w, ROW_H),
    ));
    pane.back.sizeToFit();
    let back_w = pane.back.frame().size.width;
    pane.back.setFrame(NSRect::new(
        NSPoint::new(size.width - PAD - back_w, top),
        NSSize::new(back_w, ROW_H),
    ));
    pane.note.setFrame(NSRect::new(
        NSPoint::new(PAD + copy_w + GAP, top + (ROW_H - NOTE_H) / 2.0),
        NSSize::new(
            (size.width - PAD * 2.0 - copy_w - back_w - GAP * 2.0).max(0.0),
            NOTE_H,
        ),
    ));
}

/// Where the text goes in the pane: under the buttons.
fn text_frame(pane: &NSView) -> NSRect {
    let size = pane.frame().size;
    let height = (size.height - PAD - ROW_H - GAP - PAD).max(0.0);
    NSRect::new(
        NSPoint::new(PAD, PAD),
        NSSize::new((size.width - PAD * 2.0).max(0.0), height),
    )
}

fn signature_of(view: &TableWindowView) -> (u64, usize, usize, usize) {
    (
        view.table_key,
        view.versions.current,
        view.versions.labels.len(),
        view.rows,
    )
}
