//! The history strip along the bottom of the card: a row of small cards (type mark and a few
//! short lines, or a small picture), horizontally scrollable, the copy on screen marked. A click
//! shows that copy ([`CommandId::HistoryView`]); nothing is copied. Masked copies (privacy
//! filter) show their type mark only. Views are rebuilt only when what they show changes; the
//! few decoded pictures are kept in memory only, and nothing is logged.

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
}

const IMAGE_KEEP: usize = 24;

/// Take the strip's cards from the card being shown. The previous ones' text is wiped.
pub(crate) fn store_strip(items: &[commands::Thumb], cursor: usize) {
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
    card
}
