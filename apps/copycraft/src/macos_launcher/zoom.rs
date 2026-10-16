//! Zoom of the PDF page in the well. Below 100 % the picture view shrinks (a smaller frame); above
//! it the page, drawn again at that zoom ([`crate::macos_pdf::render_page_zoomed`]), sits in a
//! scroll view that pans with the wheel or trackpad. The views hold the picture only while it is
//! shown; Wipe and hiding the card drop it, and a masked well hides them (the blur filter is on
//! the fit picture's view only).

use super::*;

/// The scroller over the well, its document view and the picture inside it.
pub(crate) struct ZoomViews {
    scroll: Retained<NSScrollView>,
    holder: Retained<NSView>,
    image: Retained<NSImageView>,
}

pub(crate) fn make_views(mtm: MainThreadMarker) -> ZoomViews {
    let scroll = widgets::passive_scroll(mtm);
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setHidden(true);
    let holder = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    let image = widgets::image_view(mtm);
    image.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("image_preview"))));
    holder.addSubview(&image);
    scroll.setDocumentView(Some(&holder));
    ZoomViews {
        scroll,
        holder,
        image,
    }
}

/// The scroller, to put in the card.
pub(crate) fn scroll_view(views: &ZoomViews) -> &NSScrollView {
    &views.scroll
}

/// The zoomed page is what the well shows: above fit, with its drawing there, and not masked.
fn wanted() -> bool {
    ZOOM_SHOWN.get() > crate::pdf::ZOOM_FIT
        && (ZOOM_PICTURE.with(|slot| slot.borrow().is_some()) || ZOOM_CARD.get().is_none())
        && !SHADE_ON.get()
        && !well_is_masked()
}

/// The zoom of the picture on a card that is no PDF: asked for on this card, else fit.
fn image_zoom() -> u32 {
    let (key, zoom) = IMAGE_ZOOM.get();
    if key == CONTENT_KEY.with(Cell::get) {
        zoom
    } else {
        crate::pdf::ZOOM_FIT
    }
}

/// Decide before the fit picture is placed and shown what zoom the well has and whether the
/// scroller takes over.
pub(crate) fn decide() {
    if ZOOM_CARD.get().is_none() {
        ZOOM_SHOWN.set(if SHOWS_IMAGE.with(Cell::get) {
            image_zoom()
        } else {
            crate::pdf::ZOOM_FIT
        });
    }
    ZOOM_ACTIVE.set(wanted());
}

/// What a zoom shortcut, pinch or ⌘ + scroll acts on, with its zoom now.
#[derive(Clone, Copy)]
enum Target {
    /// A PDF page: the app draws it again at the zoom.
    Pdf(u32),
    /// A picture: its bitmap is scaled (a copied one has nothing to draw again).
    Image(u32),
    /// Text and code: the font size of the well. Tables and the diff are left out.
    Text(u32),
}

fn target() -> Option<Target> {
    if let Some(zoom) = ZOOM_CARD.get() {
        Some(Target::Pdf(zoom))
    } else if SHOWS_IMAGE.with(Cell::get) {
        Some(Target::Image(image_zoom()))
    } else if TEXT_ZOOMABLE.get() {
        Some(Target::Text(crate::macos_card_text::font_zoom_setting()))
    } else {
        None
    }
}

/// The zoom for the footer of a card the app does not know the zoom of (pictures, text).
pub(crate) fn local_label() -> Option<String> {
    match target()? {
        Target::Pdf(_) => None,
        Target::Image(zoom) | Target::Text(zoom) => crate::pdf::zoom_label(zoom),
    }
}

/// Zoom the card by `step`: the app's for a PDF, here for the rest. False when the card has
/// nothing to zoom.
pub(crate) fn request(step: crate::pdf::ZoomStep) -> bool {
    let Some(target) = target() else {
        return false;
    };
    match target {
        Target::Pdf(_) => launcher::emit(UserEvent::Run(CommandId::PdfZoom(step))),
        Target::Image(zoom) => {
            let next = crate::pdf::zoom_step(zoom, step);
            if next == zoom {
                NSBeep();
            } else {
                IMAGE_ZOOM.set((CONTENT_KEY.with(Cell::get), next));
                layout(false);
            }
        }
        Target::Text(zoom) => {
            let next = crate::pdf::zoom_step(zoom, step);
            if next == zoom {
                NSBeep();
            } else {
                // Kept for the session, for every text card.
                crate::macos_card_text::set_font_zoom_setting(next);
                clear_painted();
                layout(false);
            }
        }
    }
    true
}

fn current_zoom() -> Option<u32> {
    target().map(|target| match target {
        Target::Pdf(zoom) | Target::Image(zoom) | Target::Text(zoom) => zoom,
    })
}

/// The frame of the fit picture's view at the zoom: the well's, shrunk about its centre below
/// 100 %.
pub(crate) fn fit_frame(well: NSRect) -> NSRect {
    let zoom = ZOOM_SHOWN.get();
    if zoom >= crate::pdf::ZOOM_FIT {
        return well;
    }
    let (w, h) = crate::pdf::zoomed_extent(well.size.width, well.size.height, zoom);
    NSRect::new(
        NSPoint::new(
            well.origin.x + (well.size.width - w) / 2.0,
            well.origin.y + (well.size.height - h) / 2.0,
        ),
        NSSize::new(w, h),
    )
}

/// Put the scroller over `well` (or hide it), after [`decide`].
pub(crate) fn place(well: NSRect) {
    let active = ZOOM_ACTIVE.get() && wanted();
    ZOOM_ACTIVE.set(active);
    ZOOM_VIEWS.with(|slot| {
        let slot = slot.borrow();
        let Some(views) = slot.as_ref() else {
            return;
        };
        if !active {
            hide(views);
            return;
        }
        // The page drawn again (a PDF), else the picture the fit view shows, scaled.
        let (id, decoded) = match ZOOM_PICTURE.with(|slot| slot.borrow().clone()) {
            Some(picture) => (picture.allocation_id(), Some(picture)),
            None => {
                let shown =
                    PREVIEW_IMAGE.with(|slot| slot.borrow().as_ref().and_then(|view| view.image()));
                match shown {
                    Some(image) => (Retained::as_ptr(&image) as usize, None),
                    None => {
                        hide(views);
                        return;
                    }
                }
            }
        };
        let size = ZOOM_DRAWN.with(|slot| {
            let drawn = slot.get();
            if drawn.0 == id {
                return Some(drawn.1);
            }
            let image = match decoded {
                Some(picture) => picture.with(mac_ui::image::from_bytes)?,
                None => PREVIEW_IMAGE
                    .with(|slot| slot.borrow().as_ref().and_then(|view| view.image()))?,
            };
            let size = image.size();
            views.image.setImage(Some(&image));
            slot.set((id, (size.width, size.height)));
            Some((size.width, size.height))
        });
        let Some((pic_w, pic_h)) = size.filter(|(w, h)| *w >= 1.0 && *h >= 1.0) else {
            hide(views);
            return;
        };
        // The picture fitted into the well, times the zoom.
        let fit = (well.size.width / pic_w).min(well.size.height / pic_h);
        let (w, h) = crate::pdf::zoomed_extent(pic_w * fit, pic_h * fit, ZOOM_SHOWN.get());
        let (hold_w, hold_h) = (w.max(well.size.width), h.max(well.size.height));
        let fresh = ZOOM_PLACED.with(|slot| {
            let placed = (id, ZOOM_SHOWN.get(), well.size.width, well.size.height);
            slot.replace(placed) != placed
        });
        views.scroll.setFrame(well);
        views.holder.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(hold_w, hold_h),
        ));
        views.image.setFrame(NSRect::new(
            NSPoint::new((hold_w - w) / 2.0, (hold_h - h) / 2.0),
            NSSize::new(w, h),
        ));
        if fresh {
            // A new zoom or page starts at its middle.
            let clip = views.scroll.contentView();
            clip.scrollToPoint(NSPoint::new(
                ((hold_w - well.size.width) / 2.0).max(0.0),
                ((hold_h - well.size.height) / 2.0).max(0.0),
            ));
            views.scroll.reflectScrolledClipView(&clip);
        }
        views.scroll.setHidden(false);
        views.image.setAccessibilityElement(true);
        if let Some(parent) = widgets::superview(&views.scroll) {
            parent.addSubview(&views.scroll);
        }
        // The fit picture's view stays out of the way under it.
        PREVIEW_IMAGE.with(|slot| {
            if let Some(view) = slot.borrow().as_ref() {
                view.setHidden(true);
            }
        });
        raise_content_actions();
    });
}

fn hide(views: &ZoomViews) {
    views.scroll.setHidden(true);
    views.image.setAccessibilityElement(false);
}

/// Hide the scroller at once (a masked or hidden well, another card).
pub(crate) fn hide_now() {
    ZOOM_ACTIVE.set(false);
    ZOOM_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            hide(views);
        }
    });
}

/// Drop the drawn page (Wipe, the card hidden).
pub(crate) fn wipe() {
    hide_now();
    ZOOM_PICTURE.with(|slot| slot.replace(None));
    ZOOM_DRAWN.with(|slot| slot.set((0, (0.0, 0.0))));
    ZOOM_PLACED.with(|slot| slot.set((0, 0, 0.0, 0.0)));
    ZOOM_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            views.image.setImage(None);
        }
    });
}

/// ⌘= ⌘+ ⌘− ⌘0 ([`crate::pdf::zoom_key`]): true when the key was the zoom's and the card has
/// something to zoom.
pub(crate) fn zoom_chord(event: &NSEvent) -> bool {
    if target().is_none() {
        return false;
    }
    let flags = event.modifierFlags();
    let Some(chars) = event.charactersIgnoringModifiers() else {
        return false;
    };
    let Some(step) = crate::pdf::zoom_key(
        &chars.to_string(),
        flags.contains(NSEventModifierFlags::Command),
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Control),
    ) else {
        return false;
    };
    request(step)
}

/// A pinch over the well: what it adds up to is applied when it ends ([`crate::pdf::pinch_zoom`]),
/// as drawing again takes a moment.
pub(crate) fn pinch(event: &NSEvent) -> bool {
    let Some(zoom) = current_zoom() else {
        return false;
    };
    if !pointer_over_well() {
        return false;
    }
    let phase = event.phase();
    if phase.contains(NSEventPhase::Began) {
        ZOOM_PINCH.set(0.0);
    }
    ZOOM_PINCH.set(ZOOM_PINCH.get() + event.magnification());
    if phase.contains(NSEventPhase::Ended) || phase.contains(NSEventPhase::Cancelled) {
        let target = crate::pdf::pinch_zoom(zoom, ZOOM_PINCH.replace(0.0));
        if target != zoom {
            request(crate::pdf::ZoomStep::Set(target));
        }
    }
    true
}

/// ⌘ + scroll wheel or trackpad over the well zooms ([`crate::pdf::wheel_zoom`]). True when the
/// event was the zoom's; plain scrolling is not.
pub(crate) fn wheel(event: &NSEvent) -> bool {
    if !event
        .modifierFlags()
        .contains(NSEventModifierFlags::Command)
        || target().is_none()
        || !pointer_over_well()
    {
        return false;
    }
    // Momentum after a swipe is not another step.
    if !event.momentumPhase().is_empty() {
        return true;
    }
    let (accumulated, step) = crate::pdf::wheel_zoom(
        ZOOM_WHEEL.get(),
        event.scrollingDeltaY(),
        event.hasPreciseScrollingDeltas(),
    );
    ZOOM_WHEEL.set(if event.phase().is_empty() || step.is_some() {
        0.0
    } else {
        accumulated
    });
    if let Some(step) = step {
        request(step);
    }
    true
}
