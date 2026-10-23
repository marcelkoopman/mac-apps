//! Header, fields, history navigation, well actions, the version row and pop-up menus.

use super::*;

/// Size and filename stay quiet. PII is a yellow caution; financial and
/// credential are a red alert.
pub(crate) fn paint_warning_meta(label: &NSTextField, meta: &str) {
    let flat = meta.replace('\n', "  ·  ");
    let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    let attr = AttrText::new(&flat, &font, &crate::appearance::caption_color());
    let warn_font = NSFont::boldSystemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    for mark in crate::sensitivity::warning_marks(&flat) {
        let range = mark.start..mark.end;
        let (ink, wash) = warning_colors(mark.label);
        attr.font(&range, &warn_font)
            .color(&range, &ink)
            .background(&range, &wash);
    }
    label.setUsesSingleLineMode(true);
    label.setMaximumNumberOfLines(1);
    let style = NSMutableParagraphStyle::new();
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let built = attr.into_attributed();
    let range = NSRange {
        location: 0,
        length: built.length(),
    };
    mac_ui::text::set_paragraph_style(&built, &style, range);
    label.setAttributedStringValue(&built);
}

/// The image footer: shorten the filename in the middle so format and pixel size stay visible.
pub(crate) fn display_image_meta(meta: &str, max_width: f64) -> String {
    let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    let width_of = |text: &str| {
        let attr = AttrText::new(text, &font, &NSColor::secondaryLabelColor()).into_attributed();
        attr.size().width
    };
    commands::fit_leading_name(meta, max_width, width_of)
}

pub(crate) fn warning_colors(
    label: crate::sensitivity::Label,
) -> (Retained<NSColor>, Retained<NSColor>) {
    match label {
        crate::sensitivity::Label::Pii => (NSColor::blackColor(), NSColor::systemYellowColor()),
        crate::sensitivity::Label::Financial | crate::sensitivity::Label::Credential => {
            (NSColor::whiteColor(), NSColor::systemRedColor())
        }
    }
}

pub(crate) fn set_header(
    slot: &RefCell<Option<Retained<NSTextField>>>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    text: &str,
) {
    let borrowed = slot.borrow();
    let Some(label) = borrowed.as_ref() else {
        return;
    };
    label.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
    label.setAttributedStringValue(&header_line(text));
}

/// The kind in semibold, cut in the middle when it is too long.
pub(crate) fn header_line(text: &str) -> Retained<NSMutableAttributedString> {
    let semibold = mac_ui::fonts::semibold_weight();
    let title_font = NSFont::systemFontOfSize_weight(mac_ui::metrics::FONT_TITLE, semibold);
    let attr = AttrText::new(text, &title_font, &NSColor::labelColor());
    let built = attr.into_attributed();
    let style = NSMutableParagraphStyle::new();
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let range = NSRange {
        location: 0,
        length: built.length(),
    };
    mac_ui::text::set_paragraph_style(&built, &style, range);
    built
}

/// Space between Clear All and the special characters ⋯ ✕ group: wider than the usual gap.
pub(crate) const CLEAR_GAP: f64 = mac_ui::metrics::SPACE_L;

/// Place a header button at `x` inside the header group (which is one button tall).
pub(crate) fn place_header_button(slot: &RefCell<Option<GlassButton>>, x: f64, width: f64) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    button.view().setFrame(NSRect::new(
        NSPoint::new(x, 0.0),
        NSSize::new(width, mac_ui::metrics::BUTTON_H),
    ));
}

pub(crate) fn search_field(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = widgets::plain_field(mtm, mac_ui::metrics::FONT_BODY, crate::locale::t("search"));
    // Native rounded border, background and focus ring while the field is shown.
    field.setBezeled(true);
    field.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);
    field.setDrawsBackground(true);
    field.setFocusRingType(NSFocusRingType::Default);
    field.setAlphaValue(0.0);
    field.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "search_commands",
    ))));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: LauncherDelegate implements controlTextDidChange: and
            // control:textView:doCommandBySelector: with their protocol signatures; DELEGATE keeps
            // it alive for the app's lifetime.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    field
}

pub(crate) fn item_search_field(mtm: MainThreadMarker) -> Retained<NSSearchField> {
    let field = widgets::search_field(mtm, mac_ui::metrics::FONT_BODY, crate::locale::t("find"));
    field.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("find_in_item"))));
    field.setHidden(true);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: LauncherDelegate implements controlTextDidChange: and
            // control:textView:doCommandBySelector: with their protocol signatures; DELEGATE keeps
            // it alive for the app's lifetime.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    field
}

pub(crate) fn place_item_find(y: f64, shown: bool) {
    let inner = card_width() - PAD * 2.0;
    let field_w = (inner - ITEM_COUNT_W - mac_ui::metrics::BUTTON_GAP).max(40.0);
    ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setHidden(!shown);
            field.setFrame(NSRect::new(
                NSPoint::new(PAD, y + 1.0),
                NSSize::new(field_w, ITEM_FIND_H - 2.0),
            ));
        }
    });
    ITEM_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(!shown);
            label.setFrame(NSRect::new(
                NSPoint::new(
                    PAD + field_w + mac_ui::metrics::BUTTON_GAP,
                    y + (ITEM_FIND_H - mac_ui::metrics::FONT_CAPTION) / 2.0,
                ),
                NSSize::new(ITEM_COUNT_W, 16.0),
            ));
        }
    });
}

pub(crate) fn payload_view(mtm: MainThreadMarker) -> Retained<NSTextView> {
    let text = widgets::read_only_text_view(
        mtm,
        NSSize::new(mac_ui::metrics::SPACE_M, mac_ui::metrics::SPACE_M),
    );
    widgets::scroll_text_both_ways(&text);
    text.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "clipboard_content",
    ))));
    text
}

pub(crate) fn text_scroll(mtm: MainThreadMarker, text: &NSTextView) -> Retained<NSScrollView> {
    let scroll = widgets::passive_text_scroll(mtm, text);
    scroll.setHidden(true);
    scroll.setAlphaValue(0.0);
    scroll
}

pub(crate) fn image_view(mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = widgets::image_view(mtm);
    view.setHidden(true);
    // Copied and dropped pictures show here.
    view.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("image_preview"))));
    view
}

/// The version capsule on top of the chips.
pub(crate) fn raise_version_row() {
    VERSION_ROW.with(|slot| {
        if let Some(row) = slot.borrow().as_ref()
            && !row.capsule.isHidden()
        {
            raise_view(&row.capsule);
        }
    });
}

/// How wide a strip along the well's right edge counts as "over the dock".
const DOCK_STRIP: f64 = WELL_ACTION + WELL_INSET * 2.0 + 8.0;

pub(crate) fn place_content_actions(preview_y: f64) {
    let actions = CONTENT_ACTIONS.get();
    let title = CARD_TITLE.with(|slot| slot.borrow().clone());
    // Vertical dock along the right edge (top to bottom): numbers, eye, copy, save, preview.
    // Hidden until the mouse is over that edge ([`update_well_dock_hover`]); may cover the text.
    let x = card_width() - PAD - WELL_INSET - WELL_ACTION;
    let mut y = preview_y + PREVIEW_H - WELL_INSET - WELL_ACTION;
    WELL_TOP.set(preview_y + PREVIEW_H);
    WELL_BOTTOM.set(preview_y);
    let open = DOCK_HOVERED.get();
    let save_tip = commands::keyed_tip(&commands::save_tip(&title), "⌘S");
    let copy_tip = commands::copy_tip(&title);
    let preview_tip = commands::preview_tip(&title);
    let (eye_symbol, eye_label) = commands::hide_button_look(well_is_masked());
    let eye_label = crate::locale::t(eye_label);
    let (num_symbol, num_label) = commands::line_numbers_button_look(commands::line_numbers_on());
    NUMBERS_BUTTON.with(|slot| {
        if let Some(button) = slot.borrow().as_ref()
            && let Some(image) = mac_ui::icon::system_symbol(num_symbol, num_label)
        {
            button.button().setImage(Some(&image));
            button.set_accessibility_label(num_label);
        }
        place_dock_action(slot, x, &mut y, OFFERS_NUMBERS.get(), open, num_label);
    });
    HIDE_BUTTON.with(|slot| {
        if let Some(button) = slot.borrow().as_ref()
            && let Some(image) = mac_ui::icon::system_symbol(eye_symbol, eye_label)
        {
            button.button().setImage(Some(&image));
            button.set_accessibility_label(eye_label);
        }
        place_dock_action(
            slot,
            x,
            &mut y,
            actions.copy || actions.save,
            open,
            eye_label,
        );
    });
    COPY_BUTTON.with(|slot| {
        place_dock_action(slot, x, &mut y, actions.copy, open, &copy_tip);
    });
    SAVE_BUTTON.with(|slot| {
        place_dock_action(slot, x, &mut y, actions.save, open, &save_tip);
    });
    PREVIEW_BUTTON.with(|slot| {
        place_dock_action(slot, x, &mut y, actions.preview, open, &preview_tip);
    });
}

/// One dock button. Not `offered`: hidden, and it takes no place in the column. Offered: at
/// `(x, *y)`, faded in when the dock is `open` and out when not, then `*y` steps down.
fn place_dock_action(
    slot: &RefCell<Option<WellAction>>,
    x: f64,
    y: &mut f64,
    offered: bool,
    open: bool,
    tip: &str,
) {
    place_well_action(slot, x, *y, offered, tip);
    if !offered {
        return;
    }
    if let Some(button) = slot.borrow().as_ref() {
        let alpha = if open { 1.0 } else { 0.0 };
        if (button.view().alphaValue() - alpha).abs() > 0.01 {
            button.view().animator().setAlphaValue(alpha);
        }
    }
    *y -= WELL_GAP + WELL_ACTION;
}

/// The mouse moved on the card: show the dock while it is over the well's right edge, hide
/// it when it leaves.
pub(crate) fn update_well_dock_hover() {
    let point = WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|window| window.mouseLocationOutsideOfEventStream())
    });
    let Some(point) = point else {
        return;
    };
    let hovered = dock_hit(point.x, point.y);
    if hovered != DOCK_HOVERED.get() {
        DOCK_HOVERED.set(hovered);
        place_content_actions(WELL_BOTTOM.get());
    }
}

/// Close the dock (the card hides).
pub(crate) fn reset_well_dock() {
    DOCK_HOVERED.set(false);
}

fn dock_hit(x: f64, y: f64) -> bool {
    let right = card_width() - PAD;
    x >= right - DOCK_STRIP && x <= right && y >= WELL_BOTTOM.get() && y <= WELL_TOP.get()
}

/// Keep the well's text clear of the Show all footer (the dock may cover the text).
/// The text view itself is narrowed and raised; content insets stay at zero so the padding
/// is not applied twice.
pub(crate) fn fit_text_well(preview_y: f64) {
    if SHOWS_IMAGE.with(Cell::get) {
        set_text_gutter(0.0, 0.0);
        return;
    }
    // The dock buttons fade in over the text on hover: no room is kept for them.
    let reserve = 0.0;
    let note = PREVIEW_NOTE.with(|slot| !slot.borrow().is_empty());
    let footer = if note && !well_is_masked() && !picker_open() {
        SHOW_ALL_H + WELL_INSET + 6.0
    } else {
        0.0
    };
    let full_w = card_width() - PAD * 2.0;
    let text_w = (full_w - reserve).max(80.0);
    let text_h = (PREVIEW_H - footer).max(40.0);
    let frame = NSRect::new(
        NSPoint::new(PAD, preview_y + footer),
        NSSize::new(text_w, text_h),
    );
    PREVIEW_CLIP.with(|slot| {
        if let Some(clip) = slot.borrow().as_ref() {
            clip.setFrame(frame);
        }
    });
    PREVIEW_SCROLL.with(|slot| {
        if let Some(scroll) = slot.borrow().as_ref() {
            scroll.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
        }
    });
    set_text_gutter(0.0, 0.0);
}

pub(crate) fn place_well_action(
    slot: &RefCell<Option<WellAction>>,
    x: f64,
    y: f64,
    shown: bool,
    tip: &str,
) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    // Under the column picker they are out of reach (Tab, VoiceOver) too.
    let shown = shown && !picker_open();
    button.view().setHidden(!shown);
    button.view().setFrame(NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(WELL_ACTION, WELL_ACTION),
    ));
    button.button().setToolTip(Some(&NSString::from_str(tip)));
    if shown {
        raise_view(button.view());
    }
}

pub(crate) fn raise_content_actions() {
    for slot in [
        &COPY_BUTTON,
        &SAVE_BUTTON,
        &HIDE_BUTTON,
        &NUMBERS_BUTTON,
        &PREVIEW_BUTTON,
    ] {
        slot.with(|slot| {
            if let Some(button) = slot.borrow().as_ref()
                && !button.view().isHidden()
            {
                raise_view(button.view());
            }
        });
    }
    SHOW_ALL.with(|slot| {
        if let Some(pill) = slot.borrow().as_ref()
            && !pill.button.view().isHidden()
        {
            raise_view(pill.button.view());
        }
    });
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref()
            && spinner.is_added()
        {
            raise_view(spinner.view());
        }
    });
    // Last: above Show all / spinner / well actions so All / None / Apply stay clickable.
    raise_picker();
}

/// "Show all" pill, with the preview note in its title.
pub(crate) struct ShowAllButton {
    pub(crate) button: GlassButton,
    pub(crate) title: String,
}

pub(crate) const SHOW_ALL_H: f64 = 24.0;

/// Show the "Show all" pill centered at the bottom of the well while it holds a preview and is
/// not blurred. The pill title carries the note, e.g. "Showing 200 of 23,220 rows · Show all".
pub(crate) fn place_show_all(mtm: MainThreadMarker, preview_y: f64) {
    let note = PREVIEW_NOTE.with(|slot| slot.borrow().clone());
    // Under the column picker the pill would sit on All / None / Apply and steal clicks.
    let shown =
        !note.is_empty() && !well_is_masked() && !SHOWS_IMAGE.with(Cell::get) && !picker_open();
    let title = crate::locale::tf("show_all_title", &[&note]);
    SHOW_ALL.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !shown {
            if let Some(pill) = slot.as_ref() {
                pill.button.view().setHidden(true);
            }
            return;
        }
        if slot.is_none() {
            let button = GlassButton::pill(mtm, &title, ButtonSize::Small);
            wire_button(button.button(), sel!(showAllClicked:));
            button
                .button()
                .setToolTip(Some(&NSString::from_str(crate::locale::t("show_all_tip"))));
            let parent = WELL.with(|well| {
                well.borrow()
                    .as_ref()
                    .and_then(|well| mac_ui::widgets::superview(well))
            });
            if let Some(parent) = parent {
                parent.addSubview(button.view());
            }
            *slot = Some(ShowAllButton {
                button,
                title: title.clone(),
            });
        }
        let Some(pill) = slot.as_mut() else {
            return;
        };
        if pill.title != title {
            pill.button.set_title(&title);
            pill.title = title;
        }
        let inner = card_width() - PAD * 2.0 - WELL_INSET * 2.0;
        let width = pill.button.width_within(inner);
        pill.button.view().setFrame(NSRect::new(
            NSPoint::new(
                PAD + (card_width() - PAD * 2.0 - width) / 2.0,
                preview_y + WELL_INSET,
            ),
            NSSize::new(width, SHOW_ALL_H),
        ));
        pill.button.view().setHidden(false);
        raise_view(pill.button.view());
    });
}

pub(crate) fn set_text_gutter(right: f64, bottom: f64) {
    let (old_right, old_bottom) = TEXT_GUTTER.get();
    if (old_right - right).abs() < 0.5 && (old_bottom - bottom).abs() < 0.5 {
        return;
    }
    TEXT_GUTTER.set((right, bottom));
    PREVIEW_SCROLL.with(|slot| {
        if let Some(scroll) = slot.borrow().as_ref() {
            scroll.setContentInsets(NSEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom,
                right,
            });
        }
    });
}

pub(crate) type WellAction = GlassButton;

pub(crate) fn well_action(
    mtm: MainThreadMarker,
    symbol: &str,
    label: &str,
    fallback: &str,
    action: Sel,
) -> WellAction {
    let button = GlassButton::symbol(mtm, symbol, label, fallback, mac_ui::metrics::FONT_BODY);
    wire_button(button.button(), action);
    button.view().setHidden(true);
    button
}

/// Header symbol button (More, Close); `label` is what VoiceOver reads.
pub(crate) fn header_symbol(
    mtm: MainThreadMarker,
    symbol: &str,
    label: &str,
    fallback: &str,
    action: Sel,
) -> GlassButton {
    let button = GlassButton::symbol(mtm, symbol, label, fallback, mac_ui::metrics::FONT_BODY);
    wire_button(button.button(), action);
    button
}

/// ⌘Z right after a copy was removed from history puts it back ([`commands::restore_key`]);
/// else ⌘Z or ⇧⌘Z while the table has a version to go to ([`commands::undo_key`]), or a
/// picture or text a step's copy ([`commands::image_undo_key`], [`commands::text_undo_key`]). Typed
/// search text keeps the keys.
pub(crate) fn undo_chord(event: &NSEvent) -> Option<CommandId> {
    if SEARCHING.with(Cell::get) && !current_query().is_empty() {
        return None;
    }
    let flags = event.modifierFlags();
    let chars = event.charactersIgnoringModifiers()?.to_string();
    if let Some(id) = commands::restore_key(
        &chars,
        flags.contains(NSEventModifierFlags::Command),
        flags.contains(NSEventModifierFlags::Shift),
        flags.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option),
        strip_can_restore(),
    ) {
        return Some(id);
    }
    let command = flags.contains(NSEventModifierFlags::Command);
    let shift = flags.contains(NSEventModifierFlags::Shift);
    let other = flags.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option);
    VERSION_BAR
        .with(|slot| commands::undo_key(&chars, command, shift, other, slot.borrow().as_ref()))
        .or_else(|| {
            IMAGE_STEPS.with(|slot| {
                commands::image_undo_key(&chars, command, shift, other, slot.borrow().as_ref())
            })
        })
        .or_else(|| {
            TEXT_STEPS.with(|slot| {
                commands::text_undo_key(&chars, command, shift, other, slot.borrow().as_ref())
            })
        })
}

/// The table's version row at the right end of the chip row: undo `↶`, the
/// version shown (`v2/3`, a button that pops the list of versions) and redo `↷`.
pub(crate) struct VersionRow {
    pub(crate) capsule: Retained<NSView>,
    pub(crate) undo: GlassButton,
    pub(crate) title: Retained<NSButton>,
    pub(crate) redo: GlassButton,
}

pub(crate) fn version_row(mtm: MainThreadMarker) -> VersionRow {
    let height = commands::CHIP_PILL_H;
    let capsule = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::VERSION_SPAN, height),
        ),
    );
    capsule.setHidden(true);
    let step_button = |symbol: &str, name: &str, glyph: &str, keys: &str, action: Sel| {
        let button = GlassButton::symbol(mtm, symbol, name, glyph, NAV_SYMBOL);
        let ns = button.button();
        ns.setAlignment(NSTextAlignment::Center);
        ns.setAccessibilityLabel(Some(&NSString::from_str(name)));
        ns.setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
        wire_button(ns, action);
        button
    };
    let undo = step_button(
        "arrow.uturn.backward",
        crate::locale::t("undo_table_step"),
        "↶",
        "⌘Z",
        sel!(tableUndoClicked:),
    );
    let redo = step_button(
        "arrow.uturn.forward",
        crate::locale::t("redo_table_step"),
        "↷",
        "⇧⌘Z",
        sel!(tableRedoClicked:),
    );
    // The history count's look (tabular digits, secondary colour), as a borderless button
    // sized to its text and centred the same way.
    let title = widgets::text_button(mtm, "v20/20", NAV_FONT);
    title.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
        NAV_FONT, 0.0,
    )));
    title.setContentTintColor(Some(&crate::appearance::caption_color()));
    title.setAlignment(NSTextAlignment::Center);
    wire_button(&title, sel!(versionsClicked:));
    let title_h = title.fittingSize().height.ceil();
    undo.view().setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    title.setFrame(NSRect::new(
        NSPoint::new(commands::nav_mid_x(), ((height - title_h) / 2.0).floor()),
        NSSize::new(commands::NAV_COUNT_W, title_h),
    ));
    redo.view().setFrame(NSRect::new(
        NSPoint::new(commands::nav_end_x(), 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    capsule.addSubview(undo.view());
    capsule.addSubview(&title);
    capsule.addSubview(redo.view());
    VersionRow {
        capsule,
        undo,
        title,
        redo,
    }
}

/// Show the version capsule at the right end of the chip row at `y`, with `bar`; or hide it
/// (`None`).
pub(crate) fn place_version_row(y: f64, bar: Option<&commands::VersionBar>) {
    VERSION_ROW.with(|slot| {
        let borrowed = slot.borrow();
        let Some(row) = borrowed.as_ref() else {
            return;
        };
        row.capsule.setHidden(bar.is_none());
        let Some(bar) = bar else {
            return;
        };
        row.capsule.setFrame(NSRect::new(
            NSPoint::new(commands::version_capsule_x(card_width() - PAD), y),
            NSSize::new(commands::VERSION_SPAN, commands::CHIP_PILL_H),
        ));
        row.undo.button().setEnabled(bar.can_undo());
        row.redo.button().setEnabled(bar.can_redo());
        let spoken = NSString::from_str(&bar.spoken());
        row.title.setTitle(&NSString::from_str(&bar.title()));
        row.title.setAccessibilityLabel(Some(&spoken));
        row.title.setToolTip(Some(&spoken));
    });
}

/// Pop a menu of `items` (a check mark on the `true` ones) under `anchor`; a pick runs its
/// command like a chip.
pub(crate) fn pop_menu(items: Vec<(Command, bool)>, anchor: &NSView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    // Items of one group ([`commands::menu_group`]) go into a submenu under its name.
    // With the item last put in it, for the separators inside a submenu.
    let mut group: Option<(&str, Retained<NSMenu>, usize)> = None;
    // The Text ▾ menu has section headers like the ⋯ menu.
    let ids: Vec<CommandId> = items.iter().map(|(cmd, _)| cmd.id.clone()).collect();
    let sections = commands::text_sections(&ids);
    let icons = commands::menu_shows_icons(&ids);
    let mut section: Option<commands::TextSection> = None;
    for (index, (cmd, checked)) in items.iter().enumerate() {
        if let Some(this) = sections
            .as_ref()
            .and_then(|sections| sections.get(index).copied().flatten())
            && section != Some(this)
        {
            add_section_header(mtm, &menu, this.title(), section.is_some());
            section = Some(this);
            group = None;
        }
        let mut previous: Option<usize> = None;
        let target = match commands::menu_group(&cmd.id) {
            None => {
                group = None;
                menu.clone()
            }
            Some(name) => match &mut group {
                Some((shown, submenu, last)) if *shown == name => {
                    previous = Some(std::mem::replace(last, index));
                    submenu.clone()
                }
                _ => {
                    let parent = mac_ui::widgets::menu_item(mtm, name);
                    // The icon sits on the submenu's row (Case ▸, Encoding ▸, Hash ▸); the
                    // rows inside have none.
                    if icons
                        && let Some(image) = commands::symbol_for(&cmd.id)
                            .and_then(|symbol| mac_ui::icon::system_symbol(symbol, name))
                    {
                        parent.setImage(Some(&image));
                    }
                    let submenu =
                        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(name));
                    submenu.setAutoenablesItems(false);
                    parent.setSubmenu(Some(&submenu));
                    menu.addItem(&parent);
                    group = Some((name, submenu.clone(), index));
                    submenu
                }
            },
        };
        let previous_id = previous
            .and_then(|at| items.get(at))
            .map(|(cmd, _)| &cmd.id);
        if commands::separator_before(previous_id, &cmd.id) {
            target.addItem(&NSMenuItem::separatorItem(mtm));
        }
        let item = mac_ui::actions::menu_item_with_action(mtm, &cmd.title, sel!(poppedClicked:));
        item.setTag(index as isize);
        show_menu_key(&item, &cmd.id);
        if icons
            && commands::menu_group(&cmd.id).is_none()
            && let Some(image) = commands::symbol_for(&cmd.id)
                .and_then(|symbol| mac_ui::icon::system_symbol(symbol, &cmd.title))
        {
            item.setImage(Some(&image));
        }
        // A dimmed item says why in its tooltip (Apple Intelligence off).
        if !cmd.enabled {
            item.setEnabled(false);
            if !cmd.detail.is_empty() {
                item.setToolTip(Some(&NSString::from_str(&cmd.detail)));
            }
        }
        if *checked {
            item.setState(NSControlStateValueOn);
        }
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::set_menu_target(&item, delegate);
            }
        });
        target.addItem(&item);
    }
    POPPED.with(|slot| set_commands(slot, items.into_iter().map(|(cmd, _)| cmd).collect()));
    let below = NSPoint::new(0.0, anchor.frame().size.height + 4.0);
    menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(anchor));
    focus_card();
}

/// The "Table ▾" (or "Image ▾") chip's menu: the steps, undo and redo.
pub(crate) fn pop_table_menu() {
    let checks = TABLE_MENU_CHECKS.with(|slot| slot.borrow().clone());
    let items: Vec<(Command, bool)> = TABLE_MENU.with(|slot| {
        slot.borrow()
            .iter()
            .enumerate()
            .map(|(index, cmd)| (cmd.clone(), checks.get(index).copied().unwrap_or(false)))
            .collect()
    });
    let index = SHOWN.with(|slot| {
        slot.borrow().iter().position(|cmd| {
            matches!(
                cmd.id,
                CommandId::TableMenu | CommandId::ImageMenu | CommandId::TextMenu
            )
        })
    });
    let chip = index.and_then(|index| CHIPS.with(|slot| slot.borrow().get(index).cloned()));
    match chip {
        Some(chip) => pop_menu(items, chip.view()),
        None => {
            let field = FIELD.with(|slot| slot.borrow().clone());
            if let Some(field) = field {
                pop_menu(items, &field);
            }
        }
    }
}

/// The AI chip's menu: the five actions, under the chip. Dimmed with the
/// reason as tooltip when the model is not available.
pub(crate) fn pop_ai_menu() {
    let fns = AI_FNS.with(|slot| slot.borrow().clone());
    let availability = mac_ai::availability();
    let mut menu = commands::ai_menu_items_with(availability, AI_ACTIONS.get(), fns.as_ref());
    if availability.is_ok() {
        menu.push(commands::ai_on_device_note());
    }
    let items: Vec<(Command, bool)> = menu.into_iter().map(|cmd| (cmd, false)).collect();
    let index = SHOWN.with(|slot| {
        slot.borrow()
            .iter()
            .position(|cmd| cmd.id == CommandId::AiMenu)
    });
    let chip = index.and_then(|index| CHIPS.with(|slot| slot.borrow().get(index).cloned()));
    match chip {
        Some(chip) => pop_menu(items, chip.view()),
        None => {
            let field = FIELD.with(|slot| slot.borrow().clone());
            if let Some(field) = field {
                pop_menu(items, &field);
            }
        }
    }
}

/// The version capsule's menu: every version, the one shown checked.
pub(crate) fn pop_versions_menu() {
    let Some(bar) = VERSION_BAR.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let anchor = VERSION_ROW.with(|slot| slot.borrow().as_ref().map(|row| row.title.retain()));
    if let Some(anchor) = anchor {
        pop_menu(bar.menu(), &anchor);
    }
}

pub(crate) fn wire_button(button: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(button, delegate, action);
        }
    });
}
