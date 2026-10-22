//! The history strip along the bottom of the card: a row of small cards (type mark and a few
//! short lines, or a small picture), horizontally scrollable, the copy on screen marked. A click
//! shows that copy ([`CommandId::HistoryView`]); nothing is copied. Masked copies (privacy
//! filter) show their type mark only. Views are rebuilt only when what they show changes; the
//! few decoded pictures are kept in memory only, and nothing is logged.
//!
//! Each small card has a ✕ in its top-right corner, shown on the marked card and the one under
//! the pointer: it takes that copy out of history ([`CommandId::RemoveHistory`]), and ⌘Z puts
//! it back. The clipboard is left alone. VoiceOver has the same as actions on each card:
//! *Remove from History* and, on the other cards, *Compare with Shown*. A card's tooltip names
//! its type and how long ago it was copied, never its content.

use super::*;

const RADIUS: f64 = 8.0;

/// A small card's button, its type name and when it was copied, by history position.
type ThumbButton = (
    usize,
    Retained<NSButton>,
    &'static str,
    Option<std::time::Instant>,
);

/// What the strip draws, kept for the layout passes.
pub(crate) struct StripViews {
    scroll: Retained<NSScrollView>,
    document: Retained<NSView>,
}

// Its own block: one `thread_local!` this long hits the macro's recursion limit.
thread_local! {
    /// The cards of the card on screen ([`LaunchData::strip`]).
    pub(crate) static STRIP_ITEMS: RefCell<Vec<commands::Thumb>> = const { RefCell::new(Vec::new()) };
    /// The history position on screen: the marked card.
    static STRIP_CURSOR: Cell<usize> = const { Cell::new(0) };
    static STRIP_VIEWS: RefCell<Option<StripViews>> = const { RefCell::new(None) };
    /// What the cards were last built from, so a layout pass that changes nothing builds nothing.
    static STRIP_SIGNATURE: Cell<u64> = const { Cell::new(0) };
    /// Small pictures already decoded, by the allocation of their bytes (a few, memory only).
    static THUMB_IMAGES: RefCell<Vec<(usize, Retained<NSImage>)>> = const { RefCell::new(Vec::new()) };
    /// The ✕ of each small card, by history position.
    static REMOVE_BUTTONS: RefCell<Vec<(usize, Retained<NSButton>)>> = const { RefCell::new(Vec::new()) };
    /// Each small card's button, its type name and when it was copied, by history position,
    /// so the tooltip's age is fresh when the pointer comes over it.
    static THUMB_BUTTONS: RefCell<Vec<ThumbButton>> =
        const { RefCell::new(Vec::new()) };
    /// The history position of the small card under the pointer.
    static STRIP_HOVER: Cell<Option<usize>> = const { Cell::new(None) };
    /// A copy was just removed from history: ⌘Z puts it back.
    static CAN_RESTORE: Cell<bool> = const { Cell::new(false) };
    /// The hint row under the strip ([`LaunchData::strip_hint`]).
    static STRIP_HINT: Cell<commands::StripHint> = const { Cell::new(commands::StripHint::None) };
    /// Its one-line label, made the first time it shows.
    static STRIP_HINT_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
}

const IMAGE_KEEP: usize = 24;

/// Take the strip's cards from the card being shown. The previous ones' text is wiped.
pub(crate) fn store_strip(items: &[commands::Thumb], cursor: usize, can_restore: bool) {
    CAN_RESTORE.set(can_restore);
    STRIP_ITEMS.with(|slot| {
        let mut slot = slot.borrow_mut();
        for thumb in slot.iter_mut() {
            thumb.wipe();
        }
        *slot = items.to_vec();
    });
    STRIP_CURSOR.set(cursor);
}

/// Take the hint under the strip from the card being shown.
pub(crate) fn store_strip_hint(hint: commands::StripHint) {
    STRIP_HINT.set(hint);
}

/// The hint row under the strip, for the layout.
pub(crate) fn strip_hint() -> commands::StripHint {
    STRIP_HINT.get()
}

/// Show the hint in its row (`placed.hint_h` 0: hidden). Small secondary text, centred.
fn place_strip_hint(mtm: MainThreadMarker, placed: &crate::card_layout::Sections) {
    let hint = STRIP_HINT.get();
    STRIP_HINT_LABEL.with(|slot| {
        let mut slot = slot.borrow_mut();
        if placed.hint_h <= 0.0 || hint != commands::StripHint::Shown {
            if let Some(label) = slot.as_ref() {
                label.setHidden(true);
            }
            return;
        }
        if slot.is_none() {
            let label = mac_ui::widgets::label(
                mtm,
                mac_ui::metrics::FONT_CAPTION,
                &NSColor::secondaryLabelColor(),
            );
            label.setAlignment(mac_ui::objc2_app_kit::NSTextAlignment::Center);
            if let Some(window) = WINDOW.with(|slot| slot.borrow().clone())
                && let Some(content) = window.contentView()
            {
                content.addSubview(&label);
            }
            *slot = Some(label);
        }
        let label = slot.as_ref().expect("just made");
        label.setTextColor(Some(&crate::appearance::caption_color()));
        label.setStringValue(&NSString::from_str(crate::locale::t("strip_compare_tip")));
        label.setFrame(NSRect::new(
            NSPoint::new(PAD, placed.hint_y),
            NSSize::new(card_width() - PAD * 2.0, placed.hint_h),
        ));
        label.setHidden(false);
    });
}

/// Two cards of the strip are being compared (Esc ends it first).
pub(crate) fn strip_comparing() -> bool {
    STRIP_ITEMS.with(|slot| commands::strip_comparing(&slot.borrow()))
}

/// Forget the strip's text, pictures and views (the card closes or is wiped).
pub(crate) fn clear_strip() {
    STRIP_ITEMS.with(|slot| {
        let mut slot = slot.borrow_mut();
        for thumb in slot.iter_mut() {
            thumb.wipe();
        }
        slot.clear();
    });
    CAN_RESTORE.set(false);
    STRIP_HOVER.set(None);
    REMOVE_BUTTONS.with(|slot| slot.borrow_mut().clear());
    THUMB_BUTTONS.with(|slot| slot.borrow_mut().clear());
    THUMB_IMAGES.with(|slot| slot.borrow_mut().clear());
    STRIP_SIGNATURE.set(0);
    STRIP_HINT.set(commands::StripHint::None);
    STRIP_HINT_LABEL.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(true);
        }
    });
    STRIP_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            for card in views.document.subviews().iter() {
                card.removeFromSuperview();
            }
        }
    });
}

/// ⌘Z puts back the copy removed just now ([`commands::restore_key`]).
pub(crate) fn strip_can_restore() -> bool {
    CAN_RESTORE.get()
}

/// The history position ⌫ removes: the marked small card, while the strip shows.
pub(crate) fn strip_delete_target() -> (bool, usize) {
    let shown = STRIP_ITEMS.with(|slot| !slot.borrow().is_empty())
        && STRIP_VIEWS.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|views| !views.scroll.isHidden())
        });
    (shown, STRIP_CURSOR.get())
}

/// The pointer moved over the card: show the ✕ of the small card under it.
pub(crate) fn update_strip_hover() {
    let hovered = STRIP_VIEWS.with(|slot| {
        let slot = slot.borrow();
        let views = slot.as_ref()?;
        if views.scroll.isHidden() {
            return None;
        }
        let window = WINDOW.with(|slot| slot.borrow().clone())?;
        let point = window.mouseLocationOutsideOfEventStream();
        let clip = views.scroll.contentView();
        let in_clip = clip.convertPoint_fromView(point, None);
        let bounds = clip.bounds();
        if in_clip.x < bounds.origin.x
            || in_clip.y < bounds.origin.y
            || in_clip.x > bounds.origin.x + bounds.size.width
            || in_clip.y > bounds.origin.y + bounds.size.height
        {
            return None;
        }
        let local = views.document.convertPoint_fromView(point, None);
        let count = STRIP_ITEMS.with(|items| items.borrow().len());
        let slot = commands::thumb_at(local.x, count)?;
        STRIP_ITEMS.with(|items| items.borrow().get(slot).map(|thumb| thumb.index))
    });
    if hovered != STRIP_HOVER.get() {
        STRIP_HOVER.set(hovered);
        show_remove_buttons();
        if let Some(index) = hovered {
            refresh_tooltip(index);
        }
    }
}

/// Set the tooltip of the small card at history position `index`: type and age.
fn refresh_tooltip(index: usize) {
    THUMB_BUTTONS.with(|slot| {
        for (at, button, kind, copied) in slot.borrow().iter() {
            if *at == index {
                set_tooltip(button, kind, *copied);
            }
        }
    });
}

fn set_tooltip(button: &NSButton, kind: &str, copied: Option<std::time::Instant>) {
    let tip = commands::thumb_tooltip(kind, copied.map(|at| at.elapsed()));
    button.setToolTip(Some(&NSString::from_str(&tip)));
}

/// The ✕ shows on the marked small card and on the one under the pointer.
fn show_remove_buttons() {
    let cursor = STRIP_CURSOR.get();
    let hovered = STRIP_HOVER.get();
    REMOVE_BUTTONS.with(|slot| {
        for (index, button) in slot.borrow().iter() {
            button.setHidden(!commands::remove_shown(*index, cursor, hovered));
        }
    });
}

fn thumb_image(picture: &SecretBytes) -> Option<Retained<NSImage>> {
    let id = picture.allocation_id();
    if let Some(found) = THUMB_IMAGES.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, image)| image.clone())
    }) {
        return Some(found);
    }
    let image = picture.with(mac_ui::image::from_bytes)?;
    THUMB_IMAGES.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.len() >= IMAGE_KEEP {
            slot.remove(0);
        }
        slot.push((id, image.clone()));
    });
    Some(image)
}

fn views(mtm: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NSView>) {
    STRIP_VIEWS.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), NSRect::ZERO);
            scroll.setHasHorizontalScroller(true);
            scroll.setHasVerticalScroller(false);
            scroll.setAutohidesScrollers(true);
            scroll.setDrawsBackground(false);
            scroll.setBorderType(mac_ui::objc2_app_kit::NSBorderType::NoBorder);
            scroll
                .setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("strip_group"))));
            let document = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
            scroll.setDocumentView(Some(&document));
            if let Some(window) = WINDOW.with(|slot| slot.borrow().clone())
                && let Some(content) = window.contentView()
            {
                content.addSubview(&scroll);
            }
            *slot = Some(StripViews { scroll, document });
        }
        let views = slot.as_ref().expect("just made");
        (views.scroll.clone(), views.document.clone())
    })
}

/// Show the cards in the row `placed` gives them, or hide the strip when there are none.
pub(crate) fn place_strip(
    mtm: MainThreadMarker,
    placed: &crate::card_layout::Sections,
    items: &[commands::Thumb],
) {
    place_strip_hint(mtm, placed);
    if items.is_empty() {
        STRIP_VIEWS.with(|slot| {
            if let Some(views) = slot.borrow().as_ref() {
                views.scroll.setHidden(true);
            }
        });
        return;
    }
    let (scroll, document) = views(mtm);
    let inner = card_width() - PAD * 2.0;
    scroll.setHidden(false);
    scroll.setFrame(NSRect::new(
        NSPoint::new(PAD, placed.strip_y),
        NSSize::new(inner, placed.strip_h),
    ));
    let cursor = STRIP_CURSOR.get();
    let filter = crate::settings::privacy_filter();
    let current_masked = well_is_masked();
    let masked_of = |thumb: &commands::Thumb| {
        if thumb.index == cursor {
            current_masked
        } else {
            commands::thumb_masked(filter, false, false)
        }
    };
    let signature = signature_of(items, cursor, inner, filter, current_masked);
    let rebuilt = STRIP_SIGNATURE.get() != signature;
    if rebuilt {
        STRIP_SIGNATURE.set(signature);
        for old in document.subviews().iter() {
            old.removeFromSuperview();
        }
        REMOVE_BUTTONS.with(|slot| slot.borrow_mut().clear());
        THUMB_BUTTONS.with(|slot| slot.borrow_mut().clear());
        document.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(
                commands::strip_content_width(items.len()).max(inner),
                placed.strip_h,
            ),
        ));
        for (slot, thumb) in items.iter().enumerate() {
            let card = thumb_card(
                mtm,
                thumb,
                items.len(),
                thumb.index == cursor,
                masked_of(thumb),
            );
            card.setFrame(NSRect::new(
                NSPoint::new(commands::thumb_x(slot), commands::THUMB_MARGIN),
                NSSize::new(commands::THUMB_W, commands::THUMB_H),
            ));
            document.addSubview(&card);
        }
        show_remove_buttons();
    }
    // The marked card scrolls into view (only the strip moves, never the card).
    if let Some(slot) = items.iter().position(|thumb| thumb.index == cursor) {
        let clip = scroll.contentView();
        let now = clip.bounds().origin.x;
        let target = commands::scroll_to_thumb(slot, items.len(), inner, now);
        if (target - now).abs() > 0.5 {
            clip.scrollToPoint(NSPoint::new(target, 0.0));
            scroll.reflectScrolledClipView(&clip);
        }
    }
}

/// A fingerprint of what the cards show: positions, text, pictures, the marked one, masking and
/// the width. Hashed in memory, never stored.
fn signature_of(
    items: &[commands::Thumb],
    cursor: usize,
    width: f64,
    filter: bool,
    current_masked: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for thumb in items {
        thumb.index.hash(&mut hasher);
        thumb.mark.hash(&mut hasher);
        thumb.side.hash(&mut hasher);
        thumb.lines.hash(&mut hasher);
        thumb
            .picture
            .as_ref()
            .map(SecretBytes::allocation_id)
            .hash(&mut hasher);
    }
    (cursor, width.to_bits(), filter, current_masked).hash(&mut hasher);
    crate::locale::t("strip_current").hash(&mut hasher);
    crate::locale::t("strip_compare_hint").hash(&mut hasher);
    hasher.finish() | 1
}

fn thumb_card(
    mtm: MainThreadMarker,
    thumb: &commands::Thumb,
    total: usize,
    current: bool,
    masked: bool,
) -> Retained<NSBox> {
    let fill = if current {
        NSColor::controlAccentColor().colorWithAlphaComponent(0.18)
    } else {
        NSColor::labelColor().colorWithAlphaComponent(0.07)
    };
    let card = filled_box(mtm, RADIUS, &fill);
    if thumb.side.is_some() {
        // One side of the diff: an orange frame, and `A ·` / `B ·` before the type mark.
        card.setBorderWidth(2.0);
        card.setBorderColor(&NSColor::systemOrangeColor());
    } else if current {
        card.setBorderWidth(2.0);
        card.setBorderColor(&NSColor::controlAccentColor());
    }
    let mark = commands::thumb_mark(thumb);
    // Inset by the card's padding, so text and picture keep clear of the rounded edge.
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(commands::THUMB_PAD, commands::THUMB_PAD),
            NSSize::new(
                commands::THUMB_W - 2.0 * commands::THUMB_PAD,
                commands::THUMB_H - 2.0 * commands::THUMB_PAD,
            ),
        ),
    );
    button.setBordered(false);
    button.setFocusRingType(NSFocusRingType::Default);
    button.setAlignment(NSTextAlignment::Left);
    button.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    button.setTag(thumb.index as isize);
    let mut spoken = format!(
        "{}, {}",
        commands::thumb_spoken(thumb.index + 1, total, current),
        thumb.mark
    );
    if let Some(side) = thumb.side {
        spoken = format!(
            "{spoken}, {}",
            crate::locale::tf("strip_compare_side", &[&side])
        );
    }
    button.setAccessibilityLabel(Some(&NSString::from_str(&spoken)));
    button.setAccessibilityHelp(Some(&NSString::from_str(crate::locale::t(
        "strip_compare_hint",
    ))));
    // Type and age only: the ⌘-click hint is in VoiceOver's help and the one-time hint row.
    set_tooltip(&button, thumb.kind, thumb.copied);
    let index = thumb.index;
    let mut actions: Vec<(String, Box<dyn Fn() + 'static>)> = vec![(
        crate::locale::t("history_remove_item").to_string(),
        Box::new(move || launcher::emit(UserEvent::Run(CommandId::RemoveHistory(index)))),
    )];
    if !current {
        actions.push((
            crate::locale::t("strip_action_compare").to_string(),
            Box::new(move || launcher::emit(UserEvent::Run(CommandId::StripCompare(index)))),
        ));
    }
    mac_ui::a11y::set_custom_actions(&button, actions);
    THUMB_BUTTONS.with(|slot| {
        slot.borrow_mut()
            .push((index, button.clone(), thumb.kind, thumb.copied));
    });
    wire_button(&button, sel!(thumbClicked:));
    let picture = (!masked)
        .then(|| thumb.picture.as_ref().and_then(thumb_image))
        .flatten();
    if let Some(image) = picture {
        button.setImage(Some(&image));
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::ImageOnly);
        button.setImageScaling(mac_ui::objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    } else {
        // The type mark first, small; then the lines (dots while masked).
        let body = if masked {
            "••••••".to_string()
        } else {
            thumb.lines.clone()
        };
        let text = format!("{mark}\n{body}");
        let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
        let attributed = AttrText::new(&text, &font, &NSColor::labelColor());
        let mark_end = mark.len();
        attributed.color(&(0..mark_end), &crate::appearance::caption_color());
        if let Some(side) = thumb.side {
            attributed.color(&(0..side.len()), &NSColor::systemOrangeColor());
        }
        button.setAttributedTitle(&attributed.into_attributed());
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::NoImage);
    }
    card.addSubview(&button);
    card.addSubview(&remove_button(mtm, thumb.index));
    card
}

/// The ✕ in the small card's top-right corner (hidden until [`show_remove_buttons`]).
fn remove_button(mtm: MainThreadMarker, index: usize) -> Retained<NSButton> {
    let label = crate::locale::t("history_remove_item");
    let (x, y, side) = commands::remove_frame();
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(side, side)),
    );
    button.setBordered(false);
    button.setTag(index as isize);
    if let Some(image) = mac_ui::icon::system_symbol("xmark.circle.fill", label) {
        button.setImage(Some(&image));
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::ImageOnly);
        button.setImageScaling(mac_ui::objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
        button.setContentTintColor(Some(&crate::appearance::caption_color()));
    } else {
        button.setTitle(&NSString::from_str("✕"));
    }
    button.setToolTip(Some(&NSString::from_str(label)));
    button.setAccessibilityLabel(Some(&NSString::from_str(label)));
    button.setHidden(true);
    REMOVE_BUTTONS.with(|slot| slot.borrow_mut().push((index, button.clone())));
    wire_button(&button, sel!(thumbRemoveClicked:));
    button
}
