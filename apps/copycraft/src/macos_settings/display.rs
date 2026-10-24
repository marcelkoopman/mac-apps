//! The Display tab: the card theme with its preview, long text in pages and the symbols.

use super::*;

/// Width of the card theme popup; the preview takes the rest of the row.
pub(super) const THEME_POPUP_W: f64 = 176.0;

/// Weergave: the card theme, long text in pages and the symbols.
/// Returns the unused height.
pub(super) fn build_display(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = Tab::Display.pane_height();
    let y = take(&mut top, BUTTON_H);
    place_card_theme(mtm, body, y);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let pages = checkbox(mtm, crate::locale::t("page_long_text"), y);
    pages.setState(state_of(settings::load().page_long_text));
    wire(&pages, sel!(pageLongTextToggled:));
    body.addSubview(&pages);
    place_explanation(mtm, body, crate::locale::t("page_long_text_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_label(
        mtm,
        body,
        crate::locale::t("symbols"),
        take(&mut top, BUTTON_H),
        WIDTH - MARGIN * 2.0,
    );
    place_explanation(mtm, body, crate::locale::t("symbols_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    // The field and Restore share one row, so the card theme fits in the pane.
    let y = take(&mut top, BUTTON_H);
    let restore = GlassButton::pill(mtm, crate::locale::t("symbols_restore"), ButtonSize::Small);
    restore.set_symbol("arrow.counterclockwise");
    restore.set_accessibility_label(crate::locale::t("symbols_restore"));
    let restore_w = restore.width_within(200.0).max(BUTTON_H);
    restore.view().setFrame(NSRect::new(
        NSPoint::new(WIDTH - MARGIN - restore_w, y),
        NSSize::new(restore_w, BUTTON_H),
    ));
    wire(restore.button(), sel!(restoreSymbols:));
    body.addSubview(restore.view());
    let field = text_field(mtm, y);
    field.setFrameSize(NSSize::new(
        WIDTH - MARGIN * 2.0 - mac_ui::metrics::BUTTON_GAP - restore_w,
        BUTTON_H,
    ));
    field.setStringValue(&NSString::from_str(&settings::symbol_catalog_text()));
    field.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "symbols_field_a11y",
    ))));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: SettingsDelegate implements controlTextDidEndEditing: with the protocol
            // signature. DELEGATE keeps it alive while the window is open; the field holds it
            // weakly.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    body.addSubview(&field);
    SYMBOLS_FIELD.with(|slot| *slot.borrow_mut() = Some(field));
    top
}

/// The name of a card theme in the popup (literal keys, for the locale scan test).
pub(super) fn card_theme_name(theme: crate::card_theme::CardTheme) -> &'static str {
    theme.title()
}

/// The theme names, a separator between the basic, classic and colourful groups, each item
/// tagged with its place in [`crate::card_theme::CardTheme::ALL`]; the stored theme selected.
pub(super) fn fill_theme_popup(mtm: MainThreadMarker, popup: &NSPopUpButton) {
    popup.removeAllItems();
    let menu = popup.menu();
    let mut group = None;
    for (index, theme) in crate::card_theme::CardTheme::ALL.into_iter().enumerate() {
        if group.is_some_and(|previous| previous != theme.group())
            && let Some(menu) = &menu
        {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        group = Some(theme.group());
        popup.addItemWithTitle(&NSString::from_str(card_theme_name(theme)));
        if let Some(item) = popup.lastItem() {
            item.setTag(index as isize);
        }
    }
    select_card_theme(popup);
}

/// The card theme was picked elsewhere (`⋯` › Theme ▸): an open Settings window follows it.
pub fn card_theme_changed() {
    THEME_POPUP.with(|slot| {
        if let Some(popup) = slot.borrow().as_ref() {
            select_card_theme(popup);
        }
    });
    paint_theme_preview();
}

/// Select the stored theme by its tag (the separators make item positions differ).
pub(super) fn select_card_theme(popup: &NSPopUpButton) {
    popup.selectItemWithTag(settings::card_theme().index() as isize);
}

/// "Card theme", its popup, and a small preview plate to the right of it, on one row at `y`.
pub(super) fn place_card_theme(mtm: MainThreadMarker, body: &NSView, y: f64) {
    place_name(mtm, body, crate::locale::t("card_theme"), y);
    let popup = popup(mtm, y);
    let x = popup.frame().origin.x;
    popup.setFrameSize(NSSize::new(THEME_POPUP_W, BUTTON_H));
    fill_theme_popup(mtm, &popup);
    popup.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("card_theme"))));
    wire(popup.as_ref(), sel!(cardThemeChanged:));
    body.addSubview(&popup);
    THEME_POPUP.with(|slot| *slot.borrow_mut() = Some(popup));

    let plate_x = x + THEME_POPUP_W + mac_ui::metrics::BUTTON_GAP;
    let plate_w = WIDTH - MARGIN - plate_x;
    let plate = widgets::filled_box(
        mtm,
        mac_ui::style::SPACE_XS,
        &NSColor::textBackgroundColor(),
    );
    plate.setBorderColor(&NSColor::separatorColor());
    plate.setBorderWidth(1.0);
    plate.setFrame(NSRect::new(
        NSPoint::new(plate_x, y),
        NSSize::new(plate_w, BUTTON_H),
    ));
    body.addSubview(&plate);
    let sample = widgets::label(mtm, mac_ui::style::FONT_CODE, &NSColor::labelColor());
    let inset = mac_ui::metrics::SPACE_S;
    sample.setFrame(NSRect::new(
        NSPoint::new(plate_x + inset, y + (BUTTON_H - NOTE_H) / 2.0),
        NSSize::new(plate_w - inset * 2.0, NOTE_H),
    ));
    sample.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "card_theme_preview",
    ))));
    body.addSubview(&sample);
    THEME_PLATE.with(|slot| *slot.borrow_mut() = Some(plate));
    THEME_SAMPLE.with(|slot| *slot.borrow_mut() = Some(sample));
    paint_theme_preview();
}

/// The preview sample: "Aa" on the selection colour, then a JSON key and number, in the stored
/// theme on its background (System: the dynamic system colours, which follow the appearance).
pub(super) fn paint_theme_preview() {
    const SAMPLE: &str = "Aa {\"k\": 42}";
    let palette = settings::card_theme().palette();
    let srgb = crate::appearance::srgb;
    let (background, text, key, number, selection) = match palette {
        Some(p) => (
            srgb(p.background),
            srgb(p.text),
            srgb(p.key),
            srgb(p.number),
            srgb(p.selection),
        ),
        None => (
            NSColor::textBackgroundColor(),
            NSColor::labelColor(),
            NSColor::systemBlueColor(),
            NSColor::systemOrangeColor(),
            NSColor::selectedTextBackgroundColor(),
        ),
    };
    THEME_PLATE.with(|slot| {
        if let Some(plate) = slot.borrow().as_ref() {
            plate.setFillColor(&background);
        }
    });
    THEME_SAMPLE.with(|slot| {
        if let Some(sample) = slot.borrow().as_ref() {
            let font = mac_ui::fonts::monospace(mac_ui::style::FONT_CODE, &[]);
            let attr = mac_ui::text::AttrText::new(SAMPLE, &font, &text);
            attr.background(&(0..2), &selection);
            attr.color(&(4..7), &key);
            attr.color(&(9..11), &number);
            sample.setAttributedStringValue(&attr.into_attributed());
        }
    });
}

pub(super) fn save_symbols_field() {
    let raw = SYMBOLS_FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| field.stringValue().to_string())
    });
    let Some(raw) = raw else {
        return;
    };
    settings::set_symbol_catalog(&raw);
    show_symbol_catalog();
    crate::macos_symbols::reload();
}

pub(super) fn show_symbol_catalog() {
    SYMBOLS_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            let text = settings::symbol_catalog_text();
            if field.stringValue().to_string() != text {
                field.setStringValue(&NSString::from_str(&text));
            }
        }
    });
}
