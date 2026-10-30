//! The version strip along the bottom of the table window, above the meta line: one small card
//! per version (`v2`, its step and, once seen, its size), the version shown framed in the
//! accent colour, as the history strip along the bottom of the card shows the copies. A click
//! shows that version ([`commands::version_tile_click`]); ⌘Z and ⇧⌘Z step as before. Hidden
//! with one version, as the history strip is with one copy. While the table is masked the
//! cards show their `v` mark and dots only. Built again only when what it shows changes;
//! nothing is logged.
//!
//! Every card but the original's has a × in its top-right corner, as a history card
//! (`crate::macos_strip_parts`): shown on the version shown and the card under the pointer, it
//! takes that version's step out ([`commands::version_remove_click`]); ⌘Z right after puts
//! it back. VoiceOver has it as the card's *Remove Version* action.

use super::*;
use mac_ui::objc2_app_kit::NSCellImagePosition;

/// Space between the strip and the grid above it.
const STRIP_GAP: f64 = 6.0;
const RADIUS: f64 = 8.0;

thread_local! {
    /// The sizes of the versions shown so far (memory only, wiped with the window).
    static SIZES: RefCell<commands::VersionSizes> = RefCell::new(commands::VersionSizes::default());
    /// What the cards were last built from ([`signature_of`]).
    static SIGNATURE: Cell<u64> = const { Cell::new(0) };
    /// The version of each card, left to right (the cards show a range of the versions).
    static SLOTS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    /// Each card's × by version (not the original's).
    static REMOVES: RefCell<Vec<(usize, Retained<NSButton>)>> = const { RefCell::new(Vec::new()) };
    /// The card framed (the version shown, or the one a switch goes to).
    static FRAMED: Cell<usize> = const { Cell::new(0) };
    /// The version of the card under the pointer.
    static HOVER: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The strip's scroll view and the row of cards in it.
pub(super) struct StripViews {
    pub(super) scroll: Retained<NSScrollView>,
    document: Retained<NSView>,
}

/// The strip, hidden, along the bottom of `content` above the meta line.
pub(super) fn build(mtm: MainThreadMarker, content: &NSView, width: f64) -> StripViews {
    use NSAutoresizingMaskOptions as A;
    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(
            NSPoint::new(PAD, META_H),
            NSSize::new(width - PAD * 2.0, commands::STRIP_H),
        ),
    );
    scroll.setHasHorizontalScroller(true);
    scroll.setHasVerticalScroller(false);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setBorderType(NSBorderType::NoBorder);
    scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMaxYMargin]));
    let label = NSString::from_str(crate::locale::t("table_versions_group"));
    scroll.setAccessibilityLabel(Some(&label));
    let document = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    // A list of the cards for VoiceOver, as the history strip ("AXList", spelled out).
    document.setAccessibilityElement(true);
    document.setAccessibilityRole(Some(&NSString::from_str("AXList")));
    document.setAccessibilityLabel(Some(&label));
    scroll.setDocumentView(Some(&document));
    scroll.setHidden(true);
    content.addSubview(&scroll);
    if let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) {
        let owner: &AnyObject = &delegate;
        crate::macos_strip_parts::watch_pointer(
            owner,
            &scroll,
            &document,
            sel!(tableWindowStripScrolled:),
        );
    }
    StripViews { scroll, document }
}

/// Show the cards of `view` (or hide the strip), and move the grid and sidebar up or down
/// to make room. `masked`: the cards show their mark only.
pub(super) fn paint(views: &Views, view: &TableWindowView, masked: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let shown = commands::table_strip_shown(view);
    place_body(
        views,
        if shown {
            commands::STRIP_H + STRIP_GAP
        } else {
            0.0
        },
    );
    let strip = &views.strip;
    if !shown {
        strip.scroll.setHidden(true);
        clear(strip);
        return;
    }
    strip.scroll.setHidden(false);
    let mut tiles = SIZES.with(|sizes| {
        let mut sizes = sizes.borrow_mut();
        sizes.note(view);
        commands::table_version_tiles(view, &sizes, super::switching::framed())
    });
    let inner = strip.scroll.frame().size.width;
    let signature = signature_of(&tiles, masked, inner);
    if SIGNATURE.get() != signature {
        SIGNATURE.set(signature);
        clear(strip);
        strip.document.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(
                commands::strip_content_width(tiles.len()).max(inner),
                commands::STRIP_H,
            ),
        ));
        SLOTS.with(|slots| *slots.borrow_mut() = tiles.iter().map(|tile| tile.index).collect());
        for (slot, tile) in tiles.iter().enumerate() {
            let card = build_tile(mtm, tile, masked);
            card.setFrame(NSRect::new(
                NSPoint::new(commands::thumb_x(slot), commands::THUMB_MARGIN),
                NSSize::new(commands::THUMB_W, commands::THUMB_H),
            ));
            strip.document.addSubview(&card);
        }
    }
    if let Some(tile) = tiles.iter().find(|tile| tile.current) {
        FRAMED.set(tile.index);
    }
    show_remove_buttons();
    // The version shown scrolls into view (only the strip moves).
    if let Some(slot) = tiles.iter().position(|tile| tile.current) {
        let clip = strip.scroll.contentView();
        let now = clip.bounds().origin.x;
        let target = commands::scroll_to_thumb(slot, tiles.len(), inner, now);
        if (target - now).abs() > 0.5 {
            clip.scrollToPoint(NSPoint::new(target, 0.0));
            strip.scroll.reflectScrolledClipView(&clip);
        }
    }
    for tile in &mut tiles {
        tile.wipe();
    }
}

/// Take the cards out and forget the sizes (the window is wiped).
pub(super) fn wipe(views: &Views) {
    clear(&views.strip);
    views.strip.scroll.setHidden(true);
    place_body(views, 0.0);
    SIZES.with(|sizes| sizes.borrow_mut().wipe());
}

fn clear(strip: &StripViews) {
    SIGNATURE.set(0);
    SLOTS.with(|slots| slots.borrow_mut().clear());
    REMOVES.with(|removes| removes.borrow_mut().clear());
    HOVER.set(None);
    for card in strip.document.subviews().iter() {
        card.removeFromSuperview();
    }
}

/// A fingerprint of what the cards show, hashed in memory, never stored.
fn signature_of(tiles: &[commands::VersionTile], masked: bool, width: f64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for tile in tiles {
        (
            tile.index,
            &tile.mark,
            &tile.lines,
            &tile.spoken,
            &tile.spoken_masked,
            &tile.tooltip,
            tile.current,
        )
            .hash(&mut hasher);
    }
    (masked, width.to_bits()).hash(&mut hasher);
    crate::settings::card_theme().hash(&mut hasher);
    hasher.finish() | 1
}

/// One card, read and drawn as a history card: the fill, the accent frame on the version
/// shown, the copy's type mark small above the version's first lines; one button over the
/// whole card.
fn build_tile(
    mtm: MainThreadMarker,
    tile: &commands::VersionTile,
    masked: bool,
) -> Retained<NSBox> {
    let themed = crate::appearance::card_palette().map(|palette| palette.strip_tile(tile.current));
    let fill = match themed {
        Some(colours) => crate::appearance::srgb(colours.background),
        None if tile.current => NSColor::controlAccentColor().colorWithAlphaComponent(0.18),
        None => NSColor::labelColor().colorWithAlphaComponent(0.07),
    };
    let card = widgets::filled_box(mtm, RADIUS, &fill);
    if tile.current {
        card.setBorderWidth(2.0);
        card.setBorderColor(&NSColor::controlAccentColor());
    }
    let button = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::THUMB_W, commands::THUMB_H),
        ),
    );
    button.setBordered(false);
    button.setFocusRingType(NSFocusRingType::Default);
    button.setAlignment(NSTextAlignment::Left);
    button.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    button.setImagePosition(NSCellImagePosition::NoImage);
    let body = if masked {
        "••••••"
    } else {
        tile.lines.as_str()
    };
    let mut text = format!("{}\n{body}", tile.mark);
    let (text_colour, mark_colour) = match themed {
        Some(colours) => (
            crate::appearance::srgb(colours.text),
            crate::appearance::srgb(colours.mark),
        ),
        None => (NSColor::labelColor(), crate::appearance::caption_color()),
    };
    let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    let attributed = AttrText::new(&text, &font, &text_colour);
    attributed.color(&(0..tile.mark.len()), &mark_colour);
    let built = attributed.into_attributed();
    let style = NSMutableParagraphStyle::new();
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    style.setFirstLineHeadIndent(commands::THUMB_PAD);
    style.setHeadIndent(commands::THUMB_PAD);
    style.setTailIndent(-commands::THUMB_PAD);
    mac_ui::text::set_paragraph_style(
        &built,
        &style,
        mac_ui::text::utf16_range(&text, &(0..text.len())),
    );
    button.setAttributedTitle(&built);
    text.zeroize();
    button.setTag(tile.index as isize);
    wire(&button, sel!(tableWindowVersionClicked:));
    // Masked: the mark and dots on the card, as a masked history card; VoiceOver and the
    // tooltip say the version's number only, not its step or size.
    let (spoken, tip) = if masked {
        (tile.spoken_masked.as_str(), tile.spoken_masked.as_str())
    } else {
        (tile.spoken.as_str(), tile.tooltip.as_str())
    };
    button.setAccessibilityLabel(Some(&NSString::from_str(spoken)));
    button.setAccessibilitySelected(tile.current);
    button.setToolTip(Some(&NSString::from_str(tip)));
    card.addSubview(&button);
    if let Some(id) = tile
        .removable
        .then(|| commands::version_remove_click(tile.index))
        .flatten()
    {
        let label = crate::locale::t("table_remove_version");
        // VoiceOver: the same as the ×, on the card.
        let action = id.clone();
        mac_ui::a11y::set_custom_actions(
            &button,
            vec![(label.to_string(), Box::new(move || run(action.clone())))],
        );
        let remove = crate::macos_strip_parts::remove_button(mtm, label);
        remove.setTag(tile.index as isize);
        wire(&remove, sel!(tableWindowVersionRemoveClicked:));
        card.addSubview(&remove);
        REMOVES.with(|removes| removes.borrow_mut().push((tile.index, remove)));
    }
    card
}

/// Show the × on the card framed and the one under the pointer
/// ([`commands::version_remove_shown`]); hide the others'.
fn show_remove_buttons() {
    let framed = FRAMED.get();
    let hovered = HOVER.get();
    REMOVES.with(|removes| {
        for (index, button) in removes.borrow().iter() {
            button.setHidden(!commands::version_remove_shown(*index, framed, hovered));
        }
    });
}

/// The strip's tracking area saw the pointer come in or move, or the strip scrolled under it:
/// show the × of the card it is over.
pub(super) fn pointer_moved(point: NSPoint) {
    let hovered = VIEWS.with(|slot| {
        let borrowed = slot.try_borrow().ok()?;
        let strip = &borrowed.as_ref()?.strip;
        if strip.scroll.isHidden() {
            return None;
        }
        let clip = strip.scroll.contentView();
        let in_clip = clip.convertPoint_fromView(point, None);
        let bounds = clip.bounds();
        if in_clip.x < bounds.origin.x
            || in_clip.y < bounds.origin.y
            || in_clip.x > bounds.origin.x + bounds.size.width
            || in_clip.y > bounds.origin.y + bounds.size.height
        {
            return None;
        }
        let local = strip.document.convertPoint_fromView(point, None);
        let count = SLOTS.with(|slots| slots.borrow().len());
        let slot = commands::thumb_at(local.x, local.y, count)?;
        SLOTS.with(|slots| slots.borrow().get(slot).copied())
    });
    set_hover(hovered);
}

/// The strip scrolled: the card under a still pointer may be another.
pub(super) fn scrolled() {
    let point = VIEWS.with(|slot| {
        let borrowed = slot.try_borrow().ok()?;
        let window = borrowed.as_ref()?.strip.scroll.window()?;
        Some(window.mouseLocationOutsideOfEventStream())
    });
    match point {
        Some(point) => pointer_moved(point),
        None => set_hover(None),
    }
}

/// The pointer left the strip (or the window): no card is hovered.
pub(super) fn pointer_left() {
    set_hover(None);
}

fn set_hover(hovered: Option<usize>) {
    if HOVER.get() != hovered {
        HOVER.set(hovered);
        show_remove_buttons();
    }
}

/// The grid, its well, the sidebar and the cover start `strip_h` above the meta line; their
/// top edge stays where it is.
fn place_body(views: &Views, strip_h: f64) {
    let bottom = META_H + strip_h;
    let place = |part: &NSView| {
        let frame = part.frame();
        let top = frame.origin.y + frame.size.height;
        if (frame.origin.y - bottom).abs() >= 0.5 {
            part.setFrame(NSRect::new(
                NSPoint::new(frame.origin.x, bottom),
                NSSize::new(frame.size.width, (top - bottom).max(0.0)),
            ));
        }
    };
    place(&views.well);
    place(&views.cover);
    place(&views.shade);
    if let Some(pane) = super::chart::pane() {
        place(&pane);
    }
    if let Some(scroll) = views.grid.enclosingScrollView()
        && let Some(clip) = mac_ui::widgets::superview(&scroll)
    {
        place(&clip);
    }
    if let Some(scroll) = views.sidebar.enclosingScrollView() {
        place(&scroll);
    }
}
