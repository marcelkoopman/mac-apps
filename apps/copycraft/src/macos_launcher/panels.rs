//! The content well: reveal cover, blur, preview text and images.

use super::*;

pub(crate) struct RevealCover {
    pub(crate) root: Retained<NSView>,
    /// Opaque stand-in when the gaussian blur cannot be installed.
    pub(crate) shade: Retained<NSBox>,
    pub(crate) hit: Retained<NSButton>,
}

pub(crate) fn reveal_cover(mtm: MainThreadMarker) -> RevealCover {
    let width = card_width() - PAD * 2.0;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, PREVIEW_H));
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let shade = filled_box(mtm, WELL_RADIUS, &NSColor::textBackgroundColor());
    crate::appearance::apply_well(&shade, true, false);
    shade.setFrame(frame);
    shade.setHidden(true);
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setTransparent(true);
    hit.setTitle(&NSString::from_str(""));
    // Keyboard users can Tab to the cover and press Space to reveal, so keep the focus ring.
    hit.setFocusRingType(NSFocusRingType::Default);
    hit.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("reveal_hidden"))));
    wire_button(&hit, sel!(revealClicked:));
    root.addSubview(&shade);
    root.addSubview(&hit);
    root.setHidden(true);
    RevealCover { root, shade, hit }
}

pub(crate) fn place_reveal_cover(y: f64) {
    let width = card_width() - PAD * 2.0;
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.root.setFrame(NSRect::new(
            NSPoint::new(PAD, y),
            NSSize::new(width, PREVIEW_H),
        ));
        cover.hit.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, PREVIEW_H),
        ));
        cover.shade.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, PREVIEW_H),
        ));
    });
}

pub(crate) fn show_mask_shade(shown: bool) {
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.shade.setHidden(!shown);
    });
}

pub(crate) fn show_reveal_cover(shown: bool) {
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.root.setHidden(!shown);
        if shown {
            raise_view(&cover.root);
        }
    });
    raise_picker();
}

/// The open panel is choosing a file. A revealed card stays revealed across it.
pub fn holding_reveal<T>(body: impl FnOnce() -> T) -> T {
    let keep = is_open() && REVEALED.with(Cell::get);
    if keep {
        FILE_PANEL.set(true);
    }
    let result = body();
    if keep {
        REVEALED.set(true);
        if is_open() {
            layout(false);
        }
    }
    FILE_PANEL.set(false);
    result
}

/// Blur revealed content again (the app is no longer active). The next click reveals it.
pub(crate) fn mask_again() {
    // The open panel resigns the app; that is not a real focus loss.
    if FILE_PANEL.with(Cell::get) {
        return;
    }
    // Focus loss closes the column picker.
    close_picker();
    if !is_open() || !MASKS.with(Cell::get) || !REVEALED.with(Cell::get) {
        return;
    }
    REVEALED.set(false);
    layout(false);
    // As in `present`: the blur, then `<` `>` and the well icons, on top.
    show_reveal_cover(true);
    raise_history_nav();
    raise_content_actions();
}

/// The one place the card decides the well is masked: the privacy filter setting (read live, so
/// a change in Settings counts at once), whether this content masks, and whether it was revealed.
pub(crate) fn well_is_masked() -> bool {
    if DIFF_LOCK.with(Cell::get) && !REVEALED.with(Cell::get) {
        return true;
    }
    commands::well_masked(
        crate::settings::privacy_filter(),
        MASKS.with(Cell::get),
        REVEALED.with(Cell::get),
        HIDDEN.with(Cell::get),
    )
}

/// The privacy filter was switched in Settings. Any reveal is forgotten, so switching on masks
/// what is shown at once (a click reveals it again); switching off shows it unmasked. Nothing is
/// stored again: the open card is only laid out anew.
pub fn privacy_filter_changed() {
    REVEALED.set(false);
    if !is_open() {
        return;
    }
    layout(false);
    if well_is_masked() {
        // As in `present`: the blur, then `<` `>` and the well icons, on top.
        show_reveal_cover(true);
    }
    raise_history_nav();
    raise_content_actions();
}

/// The blurs for the well's text and its picture, or `None` when Core Image cannot build them.
/// Built once and reused: every card update (each step through history) masks the well again.
pub(crate) fn gaussian_blurs() -> Option<(Retained<AnyObject>, Retained<AnyObject>)> {
    #[cfg(test)]
    if FAIL_BLUR.with(Cell::get) {
        return None;
    }
    if let Some(built) = BLUR_FILTERS.with(|slot| slot.borrow().clone()) {
        return Some(built);
    }
    let built = (
        mac_ui::blur::gaussian(TEXT_BLUR_RADIUS)?,
        mac_ui::blur::gaussian(IMAGE_BLUR_RADIUS)?,
    );
    BLUR_FILTERS.with(|slot| slot.replace(Some(built.clone())));
    Some(built)
}

pub(crate) fn set_well_blur(on: bool) {
    set_well_accessible(!on);
    // The cover and the find bar follow the same mask. Revealed text can be searched.
    set_item_find(!on);
    if !on {
        SHADE_ON.set(false);
        show_mask_shade(false);
        clear_blur();
        return;
    }
    let filters = if crate::settings::load().blur {
        gaussian_blurs()
    } else {
        None
    };
    let plan = commands::well_mask(true, filters.is_some());
    SHADE_ON.set(plan.shade);
    show_mask_shade(plan.shade);
    if let Some((text, image)) = filters {
        if !BLUR_ON.get() {
            blur_both(
                &NSArray::from_slice(&[&*text]),
                &NSArray::from_slice(&[&*image]),
            );
            BLUR_ON.set(true);
        }
        return;
    }
    // The filter is missing. Leave the well blank under an opaque cover.
    clear_blur();
    if !plan.show_body {
        blank_masked_well();
    }
}

/// While the well is masked, its blurred text and picture are hidden from accessibility, so
/// VoiceOver (or any accessibility client) cannot read what the blur keeps from the eye. The
/// reveal cover button stays reachable.
pub(crate) fn set_well_accessible(accessible: bool) {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setAccessibilityElement(accessible);
        }
    });
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setAccessibilityElement(accessible);
        }
    });
}

pub(crate) fn clear_blur() {
    if !BLUR_ON.get() {
        return;
    }
    blur_both(&NSArray::from_slice(&[]), &NSArray::from_slice(&[]));
    BLUR_ON.set(false);
}

/// Install `text` on the well's text and `image` on its picture (empty arrays clear them).
pub(crate) fn blur_both(text: &NSArray<AnyObject>, image: &NSArray<AnyObject>) {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            mac_ui::blur::set_content_filters(view, text);
        }
    });
    blur_frozen_column(text);
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            mac_ui::blur::set_content_filters(view, image);
        }
    });
}

pub(crate) fn blank_masked_well() {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setString(&NSString::from_str(""));
            view.setSelectable(false);
        }
    });
    hide_frozen_column();
    set_preview_image_hidden(true);
    clear_painted();
}

pub(crate) fn card_meta() -> String {
    CARD_META.with(|slot| slot.borrow().clone())
}

pub(crate) fn apply_preview(y: f64) {
    if !well_is_masked() {
        SHADE_ON.set(false);
        show_mask_shade(false);
    }
    let image_frame = NSRect::new(
        NSPoint::new(PAD + WELL_INSET, y + WELL_INSET),
        NSSize::new(
            card_width() - PAD * 2.0 - WELL_INSET * 2.0,
            PREVIEW_H - WELL_INSET * 2.0,
        ),
    );
    let text_frame = NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(card_width() - PAD * 2.0, PREVIEW_H),
    );
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setFrame(image_frame);
        }
    });
    PREVIEW_CLIP.with(|slot| {
        if let Some(clip) = slot.borrow().as_ref() {
            clip.setFrame(text_frame);
        }
    });
    PREVIEW_SCROLL.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), text_frame.size));
        }
    });
    place_reveal_cover(y);
    if SHOWS_IMAGE.with(Cell::get) {
        hide_empty_line();
        clear_preview_text();
        set_preview_text_hidden(true);
        show_preview_image();
        load_thumbnail();
        finish_preview();
        return;
    }
    THUMB_TOKEN.set(-1);
    // A link is plain text in the well, like any copy (copycraft never goes online).
    set_preview_image_hidden(true);
    let excerpt = CARD_EXCERPT.with(|slot| slot.borrow().clone());
    let placeholder = CARD_PLACEHOLDER.with(|slot| slot.borrow().clone());
    let payload = !excerpt.is_empty();
    let body = if !excerpt.is_empty() {
        page_body(&excerpt, text_frame.size.height).unwrap_or(excerpt)
    } else {
        placeholder
    };
    // Empty history is one short line, centred. A placeholder with a line break (paste ask,
    // denied, private, no text) stays in the text view. A mask hides the line; the cover
    // sits above it.
    if !payload && !body.contains('\n') && !well_is_masked() {
        clear_preview_text();
        set_item_text(body.clone());
        set_preview_text_hidden(true);
        show_empty_line(&body, text_frame);
    } else {
        hide_empty_line();
        paint_preview_text(&body, payload);
    }
    finish_preview();
}

pub(crate) fn show_empty_line(text: &str, well: NSRect) {
    EMPTY_LINE.with(|slot| {
        let borrowed = slot.borrow();
        let Some(label) = borrowed.as_ref() else {
            return;
        };
        let line_h = mac_ui::metrics::FONT_BODY + 6.0;
        label.setStringValue(&NSString::from_str(text));
        label.setTextColor(Some(&crate::appearance::caption_color()));
        label.setFrame(NSRect::new(
            NSPoint::new(
                well.origin.x + WELL_INSET,
                well.origin.y + (well.size.height - line_h) / 2.0,
            ),
            NSSize::new((well.size.width - WELL_INSET * 2.0).max(0.0), line_h),
        ));
        label.setHidden(false);
    });
}

pub(crate) fn hide_empty_line() {
    EMPTY_LINE.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(true);
            label.setStringValue(&NSString::from_str(""));
        }
    });
}

pub(crate) fn finish_preview() {
    let masked = well_is_masked();
    set_well_blur(masked);
    if masked {
        PREVIEW_TEXT.with(|slot| {
            if let Some(view) = slot.borrow().as_ref() {
                view.setSelectable(false);
            }
        });
    }
    show_reveal_cover(masked);
    refresh_item_marks(false);
}

pub(crate) fn wipe_item_field() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ITEM_FIELD.with(|slot| {
        let borrowed = slot.borrow();
        let Some(field) = borrowed.as_ref() else {
            return;
        };
        widgets::wipe_field_editor(field);
        widgets::wipe_text_field(field);
    });
}

pub(crate) fn clear_preview_text() {
    set_item_text(String::new());
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            widgets::wipe_text_view(view);
        }
    });
    hide_frozen_column();
    clear_painted();
}

pub(crate) fn set_preview_image_hidden(hidden: bool) {
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setHidden(hidden);
        }
    });
}

pub(crate) fn set_preview_text_hidden(hidden: bool) {
    PREVIEW_SCROLL.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            // setHidden keeps clicks, scrolling and focus off the hidden text. The alpha only
            // backs it up: a layer-backed scroll view can keep painting after setHidden.
            view.setHidden(hidden);
            fade(view, if hidden { 0.0 } else { 1.0 });
        }
    });
    // Reorder the clip host (not the scroller): it is the card sibling that must sit behind
    // the well for a picture, and in front when the text is shown.
    PREVIEW_CLIP.with(|slot| {
        if let Some(clip) = slot.borrow().as_ref() {
            clip.setHidden(hidden);
            if let Some(parent) = mac_ui::widgets::superview(clip) {
                if hidden {
                    parent.addSubview_positioned_relativeTo(
                        clip,
                        NSWindowOrderingMode::Below,
                        None,
                    );
                } else {
                    parent.addSubview(clip);
                    raise_content_actions();
                }
            }
        }
    });
}

pub(crate) fn show_preview_image() {
    if SHADE_ON.with(Cell::get) {
        set_preview_image_hidden(true);
        return;
    }
    let image = PREVIEW_IMAGE.with(|slot| slot.borrow().clone());
    let Some(view) = image else {
        return;
    };
    view.setHidden(false);
    if let Some(parent) = mac_ui::widgets::superview(&view) {
        parent.addSubview(&view);
    }
    raise_content_actions();
}

pub(crate) fn set_preview_image(image: &NSImage) {
    image.setTemplate(false);
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setImage(Some(image));
            view.setNeedsDisplayInRect(view.bounds());
        }
    });
}

pub(crate) fn preview_wrap_width() -> f64 {
    PREVIEW_SCROLL.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| view.contentSize().width)
            .filter(|width| *width > 1.0)
            .unwrap_or(card_width() - PAD * 2.0)
    })
}

pub(crate) fn paint_preview_text(body: &str, payload: bool) {
    set_item_text(body.to_string());
    let highlight = CARD_HIGHLIGHT.with(|slot| *slot.borrow());
    let error_line = CARD_ERROR_LINE.get();
    let selectable = CARD_SELECTABLE.with(Cell::get);
    set_preview_text_hidden(false);
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setSelectable(selectable);
        }
    });
    let diff_lines = CARD_DIFF.with(Cell::get) && payload;
    let unchanged = PAINTED.with(|slot| {
        slot.borrow().as_ref().is_some_and(|painted| {
            painted.text == body
                && painted.highlight == highlight
                && painted.payload == payload
                && painted.diff_lines == diff_lines
        })
    }) && PAINTED_ERROR_LINE.get() == error_line;
    if unchanged {
        PAINTED_ITEM.set(Some(ITEM_GEN.get()));
        return;
    }
    PREVIEW_TEXT.with(|slot| {
        let borrowed = slot.borrow();
        let Some(view) = borrowed.as_ref() else {
            return;
        };
        if diff_lines {
            crate::macos_card_text::paint_diff(view, body);
        } else {
            crate::macos_card_text::paint(view, body, highlight, payload, error_line);
        }
        // A side-by-side row stays on one line and scrolls sideways. Wrapping would
        // break the two columns.
        let wrap = if diff_lines {
            None
        } else {
            (highlight.is_none()
                || matches!(highlight, Some(FormatKind::Markdown | FormatKind::Jwt)))
            .then(preview_wrap_width)
        };
        widgets::fit_text_view(view, wrap);
        view.scrollRangeToVisible(NSRange {
            location: 0,
            length: 0,
        });
    });
    refresh_frozen_column(body, highlight, payload);
    PAINTED.with(|slot| {
        if let Some(painted) = slot.borrow_mut().as_mut() {
            painted.text.zeroize();
        }
        slot.replace(Some(Painted {
            text: body.to_string(),
            highlight,
            payload,
            diff_lines,
        }));
    });
    PAINTED_ERROR_LINE.set(error_line);
    // `set_item_text(body)` above: the view shows the item.
    PAINTED_ITEM.set(Some(ITEM_GEN.get()));
}

pub(crate) fn load_thumbnail() {
    let already = PREVIEW_IMAGE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|view| !view.isHidden() && view.image().is_some())
    });
    if let Some(picture) = PICTURE.with(|slot| slot.borrow().clone()) {
        load_dropped_picture(&picture, already);
        return;
    }
    PICTURE_SHOWN.set(0);
    let change = crate::macos_pasteboard::change_count();
    if THUMB_TOKEN.with(Cell::get) == change && already {
        return;
    }
    let Some(image) = crate::macos_pasteboard::clipboard_picture() else {
        return;
    };
    // A picture set before the window is visible sticks as an empty layer.
    if window_is_visible() {
        THUMB_TOKEN.set(change);
    }
    set_preview_image(&image);
}

/// Draw a dropped picture in the well, decoded once per picture like the clipboard's.
pub(crate) fn load_dropped_picture(picture: &SecretBytes, already: bool) {
    // The clipboard's picture is drawn again when the card goes back to it.
    THUMB_TOKEN.set(-1);
    let id = picture.allocation_id();
    if PICTURE_SHOWN.get() == id && already {
        return;
    }
    let Some(image) = picture.with(mac_ui::image::from_bytes) else {
        return;
    };
    if window_is_visible() {
        PICTURE_SHOWN.set(id);
    }
    set_preview_image(&image);
}

/// The well's hide/show button: blur (or shade) the entry on screen, or show it again.
pub(crate) fn toggle_hidden() {
    let masked = well_is_masked();
    if masked
        && crate::sensitivity::meta_status(&card_meta()) == Some(crate::sensitivity::checking())
    {
        // Not before the labels are known, as for a click on the cover.
        mac_ui::objc2_app_kit::NSBeep();
        return;
    }
    let (hidden, revealed) = commands::toggle_hidden(masked);
    HIDDEN.set(hidden);
    REVEALED.set(revealed);
    layout(false);
    if well_is_masked() {
        show_reveal_cover(true);
    }
    raise_history_nav();
    raise_content_actions();
}
