//! The card's Symbols button opens this popover. One click copies that character as plain text
//! and leaves it out of history: the write is only `public.utf8-plain-text`, and the new change
//! count is ignored by the poller.

use std::cell::RefCell;

use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::AnyObject;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSButton, NSColor, NSFont, NSPopover, NSPopoverBehavior, NSTextAlignment, NSTextField, NSView,
    NSViewController,
};
use mac_ui::objc2_foundation::{
    NSObject, NSObjectNSDelayedPerforming, NSPoint, NSRect, NSRectEdge, NSSize, NSString,
};
use mac_ui::widgets;

use crate::locale;
use crate::symbols;

const COLUMNS: usize = 8;
const CELL_W: f64 = 36.0;
const CELL_H: f64 = 28.0;
const GAP: f64 = 4.0;
const PAD: f64 = 12.0;
const NOTE_H: f64 = 18.0;
/// How long "Gekopieerd: €" stays in the popover.
const NOTE_SECS: f64 = 1.2;

thread_local! {
    static POPOVER: RefCell<Option<Retained<NSPopover>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<SymbolsDelegate>>> = const { RefCell::new(None) };
    static NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    /// Symbols currently in the popover, in button-tag order.
    static SHOWN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftSymbolsDelegate"]
    struct SymbolsDelegate;

    impl SymbolsDelegate {
        #[unsafe(method(symbolClicked:))]
        fn symbol_clicked(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let index = button.tag();
            if index < 0 {
                return;
            }
            let symbol = SHOWN.with(|slot| slot.borrow().get(index as usize).cloned());
            let Some(symbol) = symbol else {
                return;
            };
            copy_symbol(self, &symbol);
        }

        #[unsafe(method(clearCopiedNote:))]
        fn clear_copied_note(&self, _sender: Option<&AnyObject>) {
            set_note("");
        }
    }
);

impl SymbolsDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm);
        unsafe { msg_send![this, init] }
    }
}

/// Open the popover under `anchor`, or close it when it is already open.
pub(crate) fn toggle(mtm: MainThreadMarker, anchor: &NSView) {
    if POPOVER.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|popover| popover.isShown())
    }) {
        close();
        return;
    }
    let delegate = DELEGATE.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| SymbolsDelegate::new(mtm))
            .clone()
    });
    let popover = NSPopover::init(NSPopover::alloc(mtm));
    popover.setBehavior(NSPopoverBehavior::Transient);
    popover.setAnimates(true);
    POPOVER.with(|slot| *slot.borrow_mut() = Some(popover.clone()));
    fill(&delegate, &popover);
    // An empty rect anchors to the button's bounds. MinY hangs the popover below the header.
    popover.showRelativeToRect_ofView_preferredEdge(NSRect::ZERO, anchor, NSRectEdge::MinY);
}

/// Close the popover. The card calls this when it hides, so the popover does not stay up alone.
pub(crate) fn close() {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            cancel_note(delegate);
        }
    });
    if let Some(popover) = POPOVER.with(|slot| slot.borrow_mut().take()) {
        popover.close();
    }
    NOTE.with(|slot| *slot.borrow_mut() = None);
    SHOWN.with(|slot| slot.borrow_mut().clear());
}

fn copy_symbol(delegate: &SymbolsDelegate, symbol: &str) {
    let change = match crate::macos_pasteboard::write_plain_text(symbol) {
        Ok(change) => change,
        Err(message) => {
            eprintln!("copycraft: symbol copy failed: {message}");
            return;
        }
    };
    // Before any further AppKit work, so the poller cannot observe the change first.
    symbols::ignore_change(change);
    crate::settings::remember_symbol(symbol);
    if let Some(popover) = POPOVER.with(|slot| slot.borrow().clone()) {
        fill(delegate, &popover);
    }
    set_note(&locale::copied_symbol(locale::lang(), symbol));
    schedule_clear(delegate);
}

fn offered() -> Vec<String> {
    symbols::display_order(
        &crate::settings::symbol_catalog(),
        &crate::settings::symbol_recent(),
    )
}

/// Rebuild an open popover after the list changes in Settings.
pub(crate) fn reload() {
    let popover = POPOVER.with(|slot| slot.borrow().clone());
    let Some(popover) = popover else {
        return;
    };
    if !popover.isShown() {
        return;
    }
    let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) else {
        return;
    };
    fill(&delegate, &popover);
}

fn fill(delegate: &SymbolsDelegate, popover: &NSPopover) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let symbols = offered();
    let (view, note, size) = build_grid(mtm, delegate, &symbols);
    NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    let controller = NSViewController::new(mtm);
    controller.setView(&view);
    popover.setContentSize(size);
    popover.setContentViewController(Some(&controller));
}

fn build_grid(
    mtm: MainThreadMarker,
    delegate: &SymbolsDelegate,
    symbols: &[String],
) -> (Retained<NSView>, Retained<NSTextField>, NSSize) {
    SHOWN.with(|slot| *slot.borrow_mut() = symbols.to_vec());
    let columns = symbols.len().clamp(1, COLUMNS);
    let rows = symbols.len().div_ceil(columns);
    let width = PAD * 2.0 + columns as f64 * CELL_W + (columns.saturating_sub(1) as f64) * GAP;
    let grid_h = if symbols.is_empty() {
        0.0
    } else {
        rows as f64 * CELL_H + (rows.saturating_sub(1) as f64) * GAP
    };
    let height = PAD + grid_h + GAP + NOTE_H + PAD;
    let size = NSSize::new(width, height);
    let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::ZERO, size));
    let grid_bottom = PAD + NOTE_H + GAP;
    for (index, symbol) in symbols.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = PAD + column as f64 * (CELL_W + GAP);
        let y = grid_bottom + (rows - 1 - row) as f64 * (CELL_H + GAP);
        let button = symbol_button(mtm, symbol, index);
        button
            .view()
            .setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(CELL_W, CELL_H)));
        // SAFETY: DELEGATE keeps the target alive for the process, and `symbolClicked:` takes
        // the sender.
        unsafe { widgets::set_target_action(button.button(), delegate, sel!(symbolClicked:)) };
        view.addSubview(button.view());
    }
    let note = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    note.setAlignment(NSTextAlignment::Center);
    note.setFrame(NSRect::new(
        NSPoint::new(PAD, PAD),
        NSSize::new((width - PAD * 2.0).max(0.0), NOTE_H),
    ));
    view.addSubview(&note);
    (view, note, size)
}

fn symbol_button(mtm: MainThreadMarker, symbol: &str, index: usize) -> GlassButton {
    let button = GlassButton::pill(mtm, symbol, ButtonSize::Small);
    button
        .button()
        .setFont(Some(&NSFont::systemFontOfSize(15.0)));
    button.button().setTag(index as isize);
    button.set_accessibility_label(&locale::copy_symbol(locale::lang(), symbol));
    button
        .button()
        .setToolTip(Some(&NSString::from_str(&locale::copy_symbol(
            locale::lang(),
            symbol,
        ))));
    button
}

fn set_note(text: &str) {
    NOTE.with(|slot| {
        if let Some(note) = slot.borrow().as_ref() {
            note.setStringValue(&NSString::from_str(text));
        }
    });
}

fn schedule_clear(delegate: &SymbolsDelegate) {
    cancel_note(delegate);
    // SAFETY: `clearCopiedNote:` is a method of this delegate and takes an optional sender.
    // DELEGATE keeps it alive past the delay.
    unsafe {
        delegate.performSelector_withObject_afterDelay(sel!(clearCopiedNote:), None, NOTE_SECS);
    }
}

fn cancel_note(delegate: &SymbolsDelegate) {
    let target: &AnyObject = delegate.as_ref();
    // SAFETY: same selector as [`schedule_clear`].
    unsafe {
        NSObject::cancelPreviousPerformRequestsWithTarget_selector_object(
            target,
            sel!(clearCopiedNote:),
            None,
        );
    }
}
