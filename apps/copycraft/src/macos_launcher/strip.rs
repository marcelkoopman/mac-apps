//! The history strip along the bottom of the card: a row of small cards (type mark and a few
//! short lines, or a small picture), horizontally scrollable, the copy on screen marked. A click
//! shows that copy ([`CommandId::HistoryView`]); nothing is copied. Masked copies (privacy
//! filter) show their type mark only. Each card's views are kept per history entry: when
//! history changes the cards slide to their new places, a new one fades in and a removed one
//! fades out (no motion with Reduce Motion); a card is built again only when what it shows
//! changes. After ⌫ or ✕ the focus goes to the neighbouring card (right, else left), after
//! ⌘Z to the card that came back. The few decoded pictures are kept in memory only, and
//! nothing is logged.
//!
//! Each small card has a ✕ in its top-right corner, shown on the marked card and the one under
//! the pointer: it takes that copy out of history ([`CommandId::RemoveHistory`]), and ⌘Z puts
//! it back. The clipboard is left alone. VoiceOver has the same as actions on each card:
//! *Remove from History* and, on the other cards, *Compare with Shown*. A card's tooltip names
//! its type and how long ago it was copied, never its content.

use super::*;

const RADIUS: f64 = 8.0;

/// One small card on screen, kept per history entry: a change in history moves it (or fades
/// it in or out) instead of building the whole row again.
struct Tile {
    /// The history entry it shows ([`tile_key`]).
    key: u64,
    /// What it draws ([`look_of`]); a new look builds it again in its place.
    look: u64,
    card: Retained<NSBox>,
    button: Retained<NSButton>,
    remove: Retained<NSButton>,
    /// History position, type name and copy time, for the tags, the ✕ and the tooltip.
    index: usize,
    kind: &'static str,
    copied: Option<std::time::Instant>,
}

/// Where the keyboard focus goes once the strip shows a removal or a ⌘Z.
#[derive(Debug, Clone, Copy)]
enum StripFocus {
    /// After ⌫ or ✕: the card that takes the removed one's place (its right neighbour), else
    /// the one left of it ([`commands::focus_after_remove`]).
    Removed { key: u64, slot: usize },
    /// After ⌘Z: the card that came back.
    Restored,
}

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
    /// The small cards on screen, in the order drawn.
    static TILES: RefCell<Vec<Tile>> = const { RefCell::new(Vec::new()) };
    /// Focus to move once the strip shows a removal or a restore.
    static PENDING_FOCUS: Cell<Option<StripFocus>> = const { Cell::new(None) };
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
    TILES.with(|slot| slot.borrow_mut().clear());
    PENDING_FOCUS.set(None);
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

/// ⌫, the ✕, a card's menu or VoiceOver is taking history position `index` out: once the
/// strip shows it gone, the focus goes to its right neighbour (else the left one).
pub(crate) fn note_strip_remove(index: usize) {
    let found = STRIP_ITEMS.with(|items| {
        items
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, thumb)| thumb.index == index)
            .map(|(slot, thumb)| (slot, tile_key(thumb)))
    });
    if let Some((slot, key)) = found {
        PENDING_FOCUS.set(Some(StripFocus::Removed { key, slot }));
    }
}

/// ⌘Z is putting the removed copy back: once its card shows, it gets the focus.
pub(crate) fn note_strip_restore() {
    PENDING_FOCUS.set(Some(StripFocus::Restored));
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
        let slot = commands::thumb_at(local.x, local.y, count)?;
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
    TILES.with(|slot| {
        for tile in slot.borrow().iter() {
            if tile.index == index {
                set_tooltip(&tile.button, tile.kind, tile.copied);
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
    TILES.with(|slot| {
        for tile in slot.borrow().iter() {
            tile.remove
                .setHidden(!commands::remove_shown(tile.index, cursor, hovered));
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
            // A list of the cards for VoiceOver ("AXList" is NSAccessibilityListRole's value,
            // spelled out: reading AppKit's constant is an `unsafe` extern static).
            document.setAccessibilityElement(true);
            document.setAccessibilityRole(Some(&NSString::from_str("AXList")));
            document
                .setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("strip_group"))));
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
        document.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(
                commands::strip_content_width(items.len()).max(inner),
                placed.strip_h,
            ),
        ));
        update_tiles(mtm, &document, items, cursor, &masked_of);
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

/// The key a card is kept by: its history entry's id (a position only if it has none).
fn tile_key(thumb: &commands::Thumb) -> u64 {
    thumb.id.unwrap_or(u64::MAX - thumb.index as u64)
}

/// What one card draws (not where, nor its position in history): the mark, the lines, the
/// picture, marked, compared, masked and the theme. Hashed in memory, never stored.
fn look_of(thumb: &commands::Thumb, current: bool, masked: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    thumb.mark.hash(&mut hasher);
    thumb.side.hash(&mut hasher);
    thumb.lines.hash(&mut hasher);
    thumb
        .picture
        .as_ref()
        .map(SecretBytes::allocation_id)
        .hash(&mut hasher);
    (current, masked).hash(&mut hasher);
    crate::settings::card_theme().hash(&mut hasher);
    hasher.finish()
}

/// Match the cards on screen to `items` by history entry: a kept card slides to its new place,
/// a new one fades in, a gone one fades out (all at once, [`mac_ui::metrics::MOTION_SECS`];
/// instant with Reduce Motion or on the first build). A card whose look changed is built again
/// where it stood. Then the focus moves as [`PENDING_FOCUS`] asks.
fn update_tiles(
    mtm: MainThreadMarker,
    document: &NSView,
    items: &[commands::Thumb],
    cursor: usize,
    masked_of: &dyn Fn(&commands::Thumb) -> bool,
) {
    let window = WINDOW.with(|slot| slot.borrow().clone());
    let focused = window.as_ref().and_then(|window| window.firstResponder());
    let has_focus = |button: &NSButton| {
        focused.as_ref().is_some_and(|responder| {
            std::ptr::eq(
                Retained::as_ptr(responder).cast::<AnyObject>(),
                (button as *const NSButton).cast::<AnyObject>(),
            )
        })
    };
    let mut old = TILES.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    let animate = !old.is_empty() && !mac_ui::motion::reduce_motion();
    let mut moves: Vec<(Retained<NSBox>, NSRect)> = Vec::new();
    let mut fade_in: Vec<Retained<NSBox>> = Vec::new();
    let mut tiles = Vec::with_capacity(items.len());
    let mut inserted: Option<usize> = None;
    let mut refocus: Option<Retained<NSButton>> = None;
    let mut place = |card: &Retained<NSBox>, target: NSRect| {
        if card.frame() == target {
            return;
        }
        if animate {
            moves.push((card.clone(), target));
        } else {
            card.setFrame(target);
        }
    };
    for (slot, thumb) in items.iter().enumerate() {
        let current = thumb.index == cursor;
        let masked = masked_of(thumb);
        let key = tile_key(thumb);
        let look = look_of(thumb, current, masked);
        let target = NSRect::new(
            NSPoint::new(commands::thumb_x(slot), commands::THUMB_MARGIN),
            NSSize::new(commands::THUMB_W, commands::THUMB_H),
        );
        let previous = old
            .iter()
            .position(|tile| tile.key == key)
            .map(|at| old.remove(at));
        let mut tile = match previous {
            Some(kept) if kept.look == look => {
                place(&kept.card, target);
                kept
            }
            Some(stale) => {
                let fresh = build_tile(mtm, thumb, key, look, current, masked);
                fresh.card.setFrame(stale.card.frame());
                place(&fresh.card, target);
                document.addSubview(&fresh.card);
                if has_focus(&stale.button) {
                    refocus = Some(fresh.button.clone());
                }
                stale.card.removeFromSuperview();
                fresh
            }
            None => {
                let fresh = build_tile(mtm, thumb, key, look, current, masked);
                fresh.card.setFrame(target);
                if animate {
                    fresh.card.setAlphaValue(0.0);
                    fade_in.push(fresh.card.clone());
                }
                inserted.get_or_insert(slot);
                document.addSubview(&fresh.card);
                fresh
            }
        };
        bind_tile(mtm, &mut tile, thumb, current);
        tiles.push(tile);
    }
    // Entries no longer in history fade out where they stood.
    let fade_out: Vec<Retained<NSBox>> = old.into_iter().map(|tile| tile.card).collect();
    if animate {
        let gone = fade_out.clone();
        mac_ui::motion::animate(
            mac_ui::metrics::MOTION_SECS,
            move || {
                for (card, target) in &moves {
                    card.animator().setFrame(*target);
                }
                for card in &fade_in {
                    card.animator().setAlphaValue(1.0);
                }
                for card in &fade_out {
                    card.animator().setAlphaValue(0.0);
                }
            },
            Some(Box::new(move || {
                for card in &gone {
                    card.removeFromSuperview();
                }
            })),
        );
    } else {
        for card in &fade_out {
            card.removeFromSuperview();
        }
    }
    let focus_slot = match PENDING_FOCUS.get() {
        Some(StripFocus::Removed { key, slot }) if !tiles.iter().any(|tile| tile.key == key) => {
            PENDING_FOCUS.set(None);
            commands::focus_after_remove(slot, tiles.len())
        }
        Some(StripFocus::Restored) if inserted.is_some() => {
            PENDING_FOCUS.set(None);
            inserted
        }
        _ => None,
    };
    let focus = focus_slot
        .and_then(|slot| tiles.get(slot))
        .map(|tile| tile.button.clone())
        .or(refocus);
    TILES.with(|slot| *slot.borrow_mut() = tiles);
    show_remove_buttons();
    if let (Some(button), Some(window)) = (focus, window) {
        window.makeFirstResponder(Some(&button));
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
        for cmd in &thumb.menu {
            cmd.title.hash(&mut hasher);
            cmd.enabled.hash(&mut hasher);
        }
        thumb
            .picture
            .as_ref()
            .map(SecretBytes::allocation_id)
            .hash(&mut hasher);
    }
    (cursor, width.to_bits(), filter, current_masked).hash(&mut hasher);
    crate::settings::card_theme().hash(&mut hasher);
    crate::locale::t("strip_compare_hint").hash(&mut hasher);
    hasher.finish() | 1
}

/// Build a small card: its fill, frame, text or picture, button and ✕. What depends on its
/// position in history is set by [`bind_tile`].
fn build_tile(
    mtm: MainThreadMarker,
    thumb: &commands::Thumb,
    key: u64,
    look: u64,
    current: bool,
    masked: bool,
) -> Tile {
    // A fixed card theme paints the tile in its own colours (tested for contrast); System
    // keeps the translucent system tint.
    let themed = crate::appearance::card_palette().map(|palette| palette.strip_tile(current));
    let fill = match themed {
        Some(tile) => crate::appearance::srgb(tile.background),
        None if current => NSColor::controlAccentColor().colorWithAlphaComponent(0.18),
        None => NSColor::labelColor().colorWithAlphaComponent(0.07),
    };
    let card = filled_box(mtm, RADIUS, &fill);
    // The accent frame is always on the card shown, also while it is compared; a side of the
    // diff has its `A` / `B` badge instead ([`side_badge`]).
    if current {
        card.setBorderWidth(2.0);
        card.setBorderColor(&NSColor::controlAccentColor());
    }
    let mark = thumb.mark.clone();
    // The button covers the whole card, so a click lands wherever the pointer shows the ✕
    // ([`commands::thumb_at`]); the padding is in the content: the text is indented, the
    // picture sits in a smaller frame behind the button.
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::THUMB_W, commands::THUMB_H),
        ),
    );
    button.setBordered(false);
    button.setTitle(&NSString::from_str(""));
    button.setFocusRingType(NSFocusRingType::Default);
    button.setAlignment(NSTextAlignment::Left);
    button.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    wire_button(&button, sel!(thumbClicked:));
    let picture = (!masked)
        .then(|| thumb.picture.as_ref().and_then(thumb_image))
        .flatten();
    if let Some(image) = picture {
        // Inset by the card's padding, so the picture keeps clear of the rounded edge; the
        // button on top takes the clicks and speaks for the card.
        let view = mac_ui::widgets::image_view(mtm);
        view.setImage(Some(&image));
        view.setFrame(NSRect::new(
            NSPoint::new(commands::THUMB_PAD, commands::THUMB_PAD),
            NSSize::new(
                commands::THUMB_W - 2.0 * commands::THUMB_PAD,
                commands::THUMB_H - 2.0 * commands::THUMB_PAD,
            ),
        ));
        view.setAccessibilityElement(false);
        card.addSubview(&view);
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::NoImage);
    } else {
        // The type mark first, small; then the lines (dots while masked).
        let body = if masked {
            "••••••".to_string()
        } else {
            thumb.lines.clone()
        };
        let text = format!("{mark}\n{body}");
        let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
        let (text_colour, mark_colour) = match themed {
            Some(tile) => (
                crate::appearance::srgb(tile.text),
                crate::appearance::srgb(tile.mark),
            ),
            None => (NSColor::labelColor(), crate::appearance::caption_color()),
        };
        let attributed = AttrText::new(&text, &font, &text_colour);
        let mark_end = mark.len();
        attributed.color(&(0..mark_end), &mark_colour);
        // The card's padding as an indent on both sides, so the text keeps clear of the edge;
        // the mark line (its own paragraph) also clears the side badge.
        let built = attributed.into_attributed();
        let mark_line = mac_ui::text::utf16_range(&text, &(0..(mark_end + 1).min(text.len())));
        let rest = mac_ui::text::utf16_range(&text, &((mark_end + 1).min(text.len())..text.len()));
        let lead = commands::mark_indent(thumb.side.is_some());
        mac_ui::text::set_paragraph_style(&built, &tile_text_style(lead), mark_line);
        mac_ui::text::set_paragraph_style(&built, &tile_text_style(commands::THUMB_PAD), rest);
        button.setAttributedTitle(&built);
        button.setImagePosition(mac_ui::objc2_app_kit::NSCellImagePosition::NoImage);
    }
    // Under the button, so a click on it still reaches the card.
    if let Some(side) = thumb.side {
        card.addSubview(&side_badge(mtm, side));
    }
    card.addSubview(&button);
    let remove = remove_button(mtm);
    card.addSubview(&remove);
    Tile {
        key,
        look,
        card,
        button,
        remove,
        index: thumb.index,
        kind: thumb.kind,
        copied: thumb.copied,
    }
}

/// A card's paragraph style: one line per paragraph, cut with `…`, indented `lead` on the
/// left and the padding on the right.
fn tile_text_style(lead: f64) -> Retained<NSMutableParagraphStyle> {
    let style = NSMutableParagraphStyle::new();
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    style.setFirstLineHeadIndent(lead);
    style.setHeadIndent(lead);
    style.setTailIndent(-commands::THUMB_PAD);
    style
}

/// The `A` / `B` badge of a side of the diff: a small filled tag in the card's top-left
/// corner ([`commands::side_badge_frame`]). Drawing only: VoiceOver reads the side in the
/// card's label ([`commands::thumb_mark`]).
fn side_badge(mtm: MainThreadMarker, side: &str) -> Retained<NSBox> {
    let (x, y, w, h) = commands::side_badge_frame();
    let badge = filled_box(mtm, 4.0, &NSColor::systemOrangeColor());
    badge.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)));
    let label = mac_ui::widgets::label(mtm, mac_ui::metrics::FONT_CAPTION, &NSColor::blackColor());
    label.setFont(Some(&NSFont::boldSystemFontOfSize(
        mac_ui::metrics::FONT_CAPTION,
    )));
    label.setAlignment(NSTextAlignment::Center);
    label.setStringValue(&NSString::from_str(side));
    label.setFrame(NSRect::new(NSPoint::new(0.0, -1.0), NSSize::new(w, h)));
    label.setAccessibilityElement(false);
    badge.addSubview(&label);
    badge
}

/// Set what depends on the card's place in history (it shifts when a copy is removed or
/// added): the tags, what VoiceOver reads and its actions, the tooltip and the right-click
/// menu, which acts on that card's copy.
fn bind_tile(mtm: MainThreadMarker, tile: &mut Tile, thumb: &commands::Thumb, current: bool) {
    tile.index = thumb.index;
    tile.kind = thumb.kind;
    tile.copied = thumb.copied;
    let button = &tile.button;
    button.setTag(thumb.index as isize);
    tile.remove.setTag(thumb.index as isize);
    // The strip is a list and the card shown its selected row, so VoiceOver says the position
    // and "selected" itself: the label is only the type mark (and the diff side).
    button.setAccessibilityLabel(Some(&NSString::from_str(&commands::thumb_mark(thumb))));
    button.setAccessibilitySelected(current);
    button.setAccessibilityHelp(Some(&NSString::from_str(crate::locale::t(
        "strip_compare_hint",
    ))));
    // Type and age only: the ⌘-click hint is in VoiceOver's help and the one-time hint row.
    set_tooltip(button, thumb.kind, thumb.copied);
    let index = thumb.index;
    let mut actions: Vec<(String, Box<dyn Fn() + 'static>)> = vec![(
        crate::locale::t("history_remove_item").to_string(),
        Box::new(move || {
            note_strip_remove(index);
            launcher::emit(UserEvent::Run(CommandId::RemoveHistory(index)));
        }),
    )];
    if !current {
        actions.push((
            crate::locale::t("strip_action_compare").to_string(),
            Box::new(move || launcher::emit(UserEvent::Run(CommandId::StripCompare(index)))),
        ));
    }
    mac_ui::a11y::set_custom_actions(button, actions);
    // Right-click or ⌃-click: Pin, the diff action, Remove from History (AppKit shows a
    // view's menu for both; no subclass).
    mac_ui::widgets::set_context_menu(button, &thumb_context_menu(mtm, thumb));
}

/// The right-click menu of a small card ([`commands::thumb_menu`]); a row's tag says which
/// card and row ([`commands::thumb_menu_tag`]). Titles name actions only, never content.
fn thumb_context_menu(mtm: MainThreadMarker, thumb: &commands::Thumb) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let mut previous: Option<&CommandId> = None;
    for (slot, cmd) in thumb.menu.iter().enumerate() {
        if commands::separator_before(previous, &cmd.id) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        previous = Some(&cmd.id);
        let item = mac_ui::actions::menu_item_with_action(mtm, &cmd.title, sel!(thumbMenuClicked:));
        item.setTag(commands::thumb_menu_tag(thumb.index, slot));
        item.setEnabled(cmd.enabled);
        DELEGATE.with(|cell| {
            if let Some(delegate) = cell.borrow().as_ref() {
                mac_ui::actions::set_menu_target(&item, delegate);
            }
        });
        menu.addItem(&item);
    }
    menu
}

/// A row of a small card's right-click menu was picked: what it runs, by its tag. `None`
/// when the strip changed meanwhile and that card or row is gone, or the row is dimmed.
pub(crate) fn thumb_menu_command(tag: isize) -> Option<CommandId> {
    let (index, slot) = commands::thumb_menu_pick(tag)?;
    STRIP_ITEMS.with(|items| {
        items
            .borrow()
            .iter()
            .find(|thumb| thumb.index == index)
            .and_then(|thumb| thumb.menu.get(slot))
            .filter(|cmd| cmd.enabled)
            .map(|cmd| cmd.id.clone())
    })
}

/// The ✕ in the small card's top-right corner (hidden until [`show_remove_buttons`]; its tag
/// is set by [`bind_tile`]).
fn remove_button(mtm: MainThreadMarker) -> Retained<NSButton> {
    let label = crate::locale::t("history_remove_item");
    let (x, y, side) = commands::remove_frame();
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(NSPoint::new(x, y), NSSize::new(side, side)),
    );
    button.setBordered(false);
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
    wire_button(&button, sel!(thumbRemoveClicked:));
    button
}
