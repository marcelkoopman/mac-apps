//! The history strip along the bottom of the card: a row of small cards (type mark and a few
//! short lines, or a small picture), horizontally scrollable, the copy on screen marked. A click
//! shows that copy ([`CommandId::HistoryView`]); nothing is copied. Masked copies (privacy
//! filter) show their type mark only. Views are rebuilt only when what they show changes; the
//! few decoded pictures are kept in memory only, and nothing is logged.
//!
//! Each small card has a ✕ in its top-right corner, shown on the marked card and the one under
//! the pointer: it takes that copy out of history ([`CommandId::RemoveHistory`]), and ⌘Z puts
//! it back. The clipboard is left alone.

use super::*;

const RADIUS: f64 = 8.0;

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
    /// The history position of the small card under the pointer.
    static STRIP_HOVER: Cell<Option<usize>> = const { Cell::new(None) };
    /// A copy was just removed from history: ⌘Z puts it back.
    static CAN_RESTORE: Cell<bool> = const { Cell::new(false) };
}

const REMOVE_SIZE: f64 = 16.0;
const REMOVE_INSET: f64 = 3.0;

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
    THUMB_IMAGES.with(|slot| slot.borrow_mut().clear());
    STRIP_SIGNATURE.set(0);
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
    }
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
                NSPoint::new(commands::thumb_x(slot), 2.0),
                NSSize::new(commands::THUMB_W, placed.strip_h - 4.0),
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
        thumb.lines.hash(&mut hasher);
        thumb
            .picture
            .as_ref()
            .map(SecretBytes::allocation_id)
            .hash(&mut hasher);
    }
    (cursor, width.to_bits(), filter, current_masked).hash(&mut hasher);
    crate::locale::t("strip_current").hash(&mut hasher);
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
    if current {
        card.setBorderWidth(2.0);
        card.setBorderColor(&NSColor::controlAccentColor());
    }
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::THUMB_W, commands::STRIP_H - 4.0),
        ),
    );
    button.setBordered(false);
    button.setFocusRingType(NSFocusRingType::Default);
    button.setAlignment(NSTextAlignment::Left);
    button.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    button.setTag(thumb.index as isize);
    button.setAccessibilityLabel(Some(&NSString::from_str(&format!(
        "{}, {}",
        commands::thumb_spoken(thumb.index + 1, total, current),
        thumb.mark
    ))));
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
        let text = format!("{}\n{body}", thumb.mark);
        let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
        let attributed = AttrText::new(&text, &font, &NSColor::labelColor());
        let mark_end = thumb.mark.len();
        attributed.color(&(0..mark_end), &crate::appearance::caption_color());
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
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(
                commands::THUMB_W - REMOVE_SIZE - REMOVE_INSET,
                commands::STRIP_H - 4.0 - REMOVE_SIZE - REMOVE_INSET,
            ),
            NSSize::new(REMOVE_SIZE, REMOVE_SIZE),
        ),
    );
    button.setBordered(false);
    button.setTag(index as isize);
    if let Some(image) = mac_ui::icon::system_symbol("xmark.circle.fill", label) {
        button.setImage(Some(&image));
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::ImageOnly);
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
