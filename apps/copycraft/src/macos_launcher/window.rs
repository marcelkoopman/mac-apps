//! Building the card window, labelling its controls and laying it out.

use super::*;
use mac_ui::objc2_app_kit::NSAppearanceCustomization;

pub(crate) fn ensure_window(mtm: MainThreadMarker) {
    if WINDOW.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let window = panel::borderless(LauncherWindow::alloc(mtm), NSSize::new(card_width(), 420.0));
    // Hover over the well's right edge shows the action dock (`mouseMoved:`).
    window.setAcceptsMouseMovedEvents(true);
    panel::configure_floating(&window);
    // The card stays open, floating over other apps' windows, until ✕, Esc, the hotkey or the
    // menu bar icon closes it. (NSWindow's default; said here because the card relies on it.)
    window.setHidesOnDeactivate(false);
    // Chips, header buttons and the find bar are added, removed and hidden on every layout.
    // Recalculating the key-view loop keeps Tab and Shift-Tab in on-screen order (top left to
    // bottom right) over the views that are actually shown.
    window.setAutorecalculatesKeyViewLoop(true);
    // Out of screenshots, screen recordings and screen sharing where macOS honours it, unless
    // "Allow screenshots" is on ([`sharing_type`]). Apple calls NSWindowSharingNone a legacy
    // constant not meant to hide content from capture, and ScreenCaptureKit (macOS 15.4+) is
    // reported to capture such windows anyway: the blur and the wipe stay the protection.
    window.setSharingType(sharing_type());

    let delegate = LauncherDelegate::new(mtm);
    // SAFETY: DELEGATE keeps the delegate alive for the rest of the process, like WINDOW.
    unsafe { panel::set_delegate(&window, &*delegate) };
    // SAFETY: the selector is the delegate's `applicationDidResignActive:`, which takes the
    // notification. DELEGATE keeps the observer alive for the rest of the process.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &delegate,
            sel!(applicationDidResignActive:),
            Some(NSApplicationDidResignActiveNotification),
            None,
        );
    }
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    // The content view takes drops (mac_ui::drop). Everything on the card is inside it, so a
    // drag anywhere over the card reaches it unless an editable text view takes it first.
    let drop_target = mac_ui::drop::target(mtm, DROP_ACCEPT, Some(drop_highlight()), |dropped| {
        launcher::emit(match dropped {
            mac_ui::drop::Dropped::File(path) => UserEvent::DroppedFile(path),
            mac_ui::drop::Dropped::Promised(path) => UserEvent::DroppedPromisedFile(path),
            mac_ui::drop::Dropped::Image(bytes) => {
                UserEvent::DroppedImage(zeroize::Zeroizing::new(bytes))
            }
            mac_ui::drop::Dropped::Text(text) => {
                UserEvent::DroppedText(zeroize::Zeroizing::new(text))
            }
        });
    });
    window.setContentView(Some(&drop_target));
    let window_view = window.contentView().expect("content view");
    window_view.setWantsLayer(true);
    window_view.setLayerUsesCoreImageFilters(true);

    // Rounded corners plus Liquid Glass on macOS 26+, else the frosted view. Before 26,
    // `content` is `window_view`.
    let content = panel::rounded_glass(mtm, &window_view, PANEL_RADIUS).content;

    let header = widgets::label(mtm, mac_ui::metrics::FONT_TITLE, &NSColor::labelColor());
    // A long type name is cut in the middle so the hotkey stays visible.
    header.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let meta = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    // Keep the warning at the end when a long filename is cut short.
    meta.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    meta.setAllowsEditingTextAttributes(true);
    let field = search_field(mtm);
    let item_find = item_search_field(mtm);
    let item_count = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    item_count.setAlignment(NSTextAlignment::Right);
    item_count.setHidden(true);
    let well = filled_box(mtm, WELL_RADIUS, &NSColor::textBackgroundColor());
    crate::appearance::apply_card_well(&well);
    window.setAppearance(crate::appearance::card_window_appearance().as_deref());
    let empty_line = widgets::label(
        mtm,
        mac_ui::metrics::FONT_BODY,
        &crate::appearance::caption_color(),
    );
    empty_line.setAlignment(NSTextAlignment::Center);
    empty_line.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    empty_line.setHidden(true);
    let preview_text = payload_view(mtm);
    let preview_scroll = text_scroll(mtm, &preview_text);
    let preview_clip = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    preview_clip.setClipsToBounds(true);
    preview_clip.addSubview(&preview_scroll);
    let preview_image = image_view(mtm);
    let zoom_views = zoom::make_views(mtm);
    widgets::set_command_wheel_hook(zoom::wheel);
    let pills = glass::group(mtm, GLASS_MERGE);
    let more = header_symbol(
        mtm,
        "ellipsis",
        crate::locale::t("more"),
        "⋯",
        sel!(moreClicked:),
    );
    let clear = GlassButton::pill(mtm, crate::locale::t("wipe"), ButtonSize::Regular);
    wire_button(clear.button(), sel!(clearClicked:));
    clear
        .button()
        .setToolTip(Some(&NSString::from_str(crate::locale::t("wipe_tip"))));
    let close = header_symbol(
        mtm,
        "xmark",
        crate::locale::t("close"),
        "✕",
        sel!(closeClicked:),
    );
    let symbols = GlassButton::pill(mtm, crate::locale::t("symbols"), ButtonSize::Regular);
    symbols.set_accessibility_label(crate::locale::t("symbols_a11y"));
    symbols
        .button()
        .setToolTip(Some(&NSString::from_str(crate::locale::t("symbols_tip"))));
    wire_button(symbols.button(), sel!(symbolsClicked:));
    let header_group = glass::group(mtm, GLASS_MERGE);
    header_group.content().addSubview(clear.view());
    header_group.content().addSubview(more.view());
    header_group.content().addSubview(close.view());
    let copy_button = well_action(
        mtm,
        "doc.on.doc",
        crate::locale::t("copy"),
        "⎘",
        sel!(copyClicked:),
    );
    let save_button = well_action(
        mtm,
        "square.and.arrow.down",
        crate::locale::t("save"),
        "↓",
        sel!(saveClicked:),
    );
    let hide_button = well_action(
        mtm,
        "eye.slash",
        crate::locale::t("hide_content"),
        "◌",
        sel!(hideClicked:),
    );
    let numbers_button = well_action(
        mtm,
        "list.number",
        crate::locale::t("show_line_numbers"),
        "#",
        sel!(numbersClicked:),
    );
    let preview_button = well_action(
        mtm,
        "arrow.up.left.and.arrow.down.right",
        crate::locale::t("open_preview"),
        "◉",
        sel!(previewClicked:),
    );
    let reveal = reveal_cover(mtm);
    let version_row = version_row(mtm);

    content.addSubview(&header);
    content.addSubview(&item_find);
    content.addSubview(&item_count);
    content.addSubview(header_group.view());
    content.addSubview(symbols.view());
    content.addSubview(&well);
    content.addSubview(&empty_line);
    content.addSubview(&preview_clip);
    content.addSubview(&preview_image);
    content.addSubview(zoom::scroll_view(&zoom_views));
    content.addSubview(&reveal.root);
    content.addSubview(&meta);
    content.addSubview(&field);
    content.addSubview(pills.view());
    content.addSubview(&version_row.capsule);
    content.addSubview(copy_button.view());
    content.addSubview(save_button.view());
    content.addSubview(hide_button.view());
    content.addSubview(numbers_button.view());
    content.addSubview(preview_button.view());

    HEADER.with(|slot| slot.replace(Some(header)));
    META.with(|slot| slot.replace(Some(meta)));
    FIELD.with(|slot| slot.replace(Some(field)));
    ITEM_FIELD.with(|slot| slot.replace(Some(item_find)));
    ITEM_COUNT.with(|slot| slot.replace(Some(item_count)));
    WELL.with(|slot| slot.replace(Some(well)));
    EMPTY_LINE.with(|slot| slot.replace(Some(empty_line)));
    PREVIEW_TEXT.with(|slot| slot.replace(Some(preview_text)));
    PREVIEW_SCROLL.with(|slot| slot.replace(Some(preview_scroll)));
    PREVIEW_CLIP.with(|slot| slot.replace(Some(preview_clip)));
    PREVIEW_IMAGE.with(|slot| slot.replace(Some(preview_image)));
    ZOOM_VIEWS.with(|slot| slot.replace(Some(zoom_views)));
    PILLS.with(|slot| slot.replace(Some(pills)));
    HEADER_GROUP.with(|slot| slot.replace(Some(header_group)));
    MORE.with(|slot| slot.replace(Some(more)));
    CLEAR.with(|slot| slot.replace(Some(clear)));
    CLOSE.with(|slot| slot.replace(Some(close)));
    SYMBOLS.with(|slot| slot.replace(Some(symbols)));
    COPY_BUTTON.with(|slot| slot.replace(Some(copy_button)));
    SAVE_BUTTON.with(|slot| slot.replace(Some(save_button)));
    HIDE_BUTTON.with(|slot| slot.replace(Some(hide_button)));
    NUMBERS_BUTTON.with(|slot| slot.replace(Some(numbers_button)));
    PREVIEW_BUTTON.with(|slot| slot.replace(Some(preview_button)));
    REVEAL.with(|slot| slot.replace(Some(reveal)));
    VERSION_ROW.with(|slot| slot.replace(Some(version_row)));
    WINDOW.with(|slot| slot.replace(Some(window)));
}

/// Point the card's fixed labels at the current language. The window is built once and stays
/// for the process; chips, the header and the well are filled again by the next layout. An
/// open column picker closes: its rows were built in the previous language.
pub fn relabel() {
    let t = crate::locale::t;
    CLEAR.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.set_title(t("wipe"));
            button
                .button()
                .setToolTip(Some(&NSString::from_str(t("wipe_tip"))));
        }
    });
    SYMBOLS.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.set_title(t("symbols"));
            button.set_accessibility_label(t("symbols_a11y"));
            button
                .button()
                .setToolTip(Some(&NSString::from_str(t("symbols_tip"))));
        }
    });
    MORE.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            label_symbol(button, t("more"));
        }
    });
    CLOSE.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            label_symbol(button, t("close"));
        }
    });
    COPY_BUTTON.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            label_symbol(button, t("copy"));
        }
    });
    SAVE_BUTTON.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            label_symbol(button, t("save"));
        }
    });
    PREVIEW_BUTTON.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            label_symbol(button, t("open_preview"));
        }
    });
    VERSION_ROW.with(|slot| {
        if let Some(row) = slot.borrow().as_ref() {
            label_step(&row.undo, t("undo_table_step"), "⌘Z");
            label_step(&row.redo, t("redo_table_step"), "⇧⌘Z");
        }
    });
    FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setPlaceholderString(Some(&NSString::from_str(t("search"))));
            field.setAccessibilityLabel(Some(&NSString::from_str(t("search_commands"))));
        }
    });
    ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setPlaceholderString(Some(&NSString::from_str(t("find"))));
            field.setAccessibilityLabel(Some(&NSString::from_str(t("find_in_item"))));
        }
    });
    REVEAL.with(|slot| {
        if let Some(cover) = slot.borrow().as_ref() {
            cover
                .hit
                .setAccessibilityLabel(Some(&NSString::from_str(t("reveal_hidden"))));
        }
    });
    PREVIEW_TEXT.with(|slot| {
        if let Some(text) = slot.borrow().as_ref() {
            text.setAccessibilityLabel(Some(&NSString::from_str(t("clipboard_content"))));
        }
    });
    PREVIEW_IMAGE.with(|slot| {
        if let Some(image) = slot.borrow().as_ref() {
            image.setAccessibilityLabel(Some(&NSString::from_str(t("image_preview"))));
        }
    });
    NO_MATCHES.with(|slot| {
        if let Some(empty) = slot.borrow().as_ref() {
            empty.setStringValue(&NSString::from_str(t("no_matching_commands")));
        }
    });
    SHOW_ALL.with(|slot| {
        if let Some(pill) = slot.borrow().as_ref() {
            pill.button
                .button()
                .setToolTip(Some(&NSString::from_str(t("show_all_tip"))));
        }
    });
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref() {
            spinner.set_accessibility_label(t("loading"));
        }
    });
    if picker_open() {
        close_picker();
    }
}

/// VoiceOver name of a symbol button. The title stays empty so the glass circle does not
/// draw the word on top of the symbol.
pub(crate) fn label_symbol(button: &GlassButton, label: &str) {
    button.set_accessibility_label(label);
}

/// Undo or redo in the version capsule. The drawn title stays the arrow.
pub(crate) fn label_step(button: &GlassButton, name: &str, keys: &str) {
    button.set_accessibility_label(name);
    button
        .button()
        .setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
}

/// Move `view` to `alpha`. Already there: nothing. Reduce Motion: jump. Otherwise a short fade.
pub(crate) fn fade(view: &NSView, alpha: f64) {
    if (view.alphaValue() - alpha).abs() < 0.01 {
        return;
    }
    let duration = crate::appearance::motion_secs();
    if duration == 0.0 {
        view.setAlphaValue(alpha);
        return;
    }
    NSAnimationContext::beginGrouping();
    NSAnimationContext::currentContext().setDuration(duration);
    view.animator().setAlphaValue(alpha);
    NSAnimationContext::endGrouping();
}

/// Opaque window when Reduce Transparency is on, clear glass otherwise. The well is filled
/// again here so Increase Contrast is picked up on the next layout, not only at construction.
pub(crate) fn apply_window_chrome() {
    let reduce = crate::appearance::display().reduce_transparency;
    WINDOW.with(|slot| {
        let window = slot.borrow();
        let Some(window) = window.as_ref() else {
            return;
        };
        if let Some(mtm) = MainThreadMarker::new() {
            mac_ui::glass::restyle_window(mtm, window, PANEL_RADIUS);
        }
        if reduce {
            window.setOpaque(true);
            window.setBackgroundColor(Some(&NSColor::windowBackgroundColor()));
        } else {
            window.setOpaque(false);
            window.setBackgroundColor(Some(&NSColor::clearColor()));
        }
    });
    WELL.with(|slot| {
        if let Some(well) = slot.borrow().as_ref() {
            crate::appearance::apply_card_well(well);
        }
    });
    let color = crate::appearance::caption_color();
    ITEM_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setTextColor(Some(&color));
        }
    });
    let well_caption = crate::appearance::card_caption_color(crate::appearance::card_palette());
    EMPTY_LINE.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setTextColor(Some(&well_caption));
        }
    });
    VERSION_ROW.with(|slot| {
        if let Some(row) = slot.borrow().as_ref() {
            row.title.setContentTintColor(Some(&color));
        }
    });
}

/// The card theme changed in Settings: the card window takes the theme's light or dark
/// appearance (System: the app's, which follows macOS), the well its fill, and an open card is
/// painted again in the new colours right away. No restart, nothing written but the setting.
pub(crate) fn apply_card_theme() {
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.setAppearance(crate::appearance::card_window_appearance().as_deref());
        }
    });
    apply_window_chrome();
    clear_painted();
    if window_is_visible() {
        layout(false);
    }
}

pub(crate) fn layout(fresh_place: bool) {
    let searching = SEARCHING.with(Cell::get);
    let query = if searching {
        current_query()
    } else {
        zeroize::Zeroizing::new(String::new())
    };
    let shown = if searching && !query.trim().is_empty() {
        POOL.with(|slot| commands::matching(&slot.borrow(), query.as_str()))
    } else {
        ACTIONS.with(|slot| slot.borrow().clone())
    };
    let mut meta = card_meta();
    // A zoomed picture or text says so (a PDF's footer has it from the app).
    if let Some(label) = zoom::local_label() {
        meta = if meta.is_empty() {
            label
        } else {
            format!("{meta}  ·  {label}")
        };
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // Each chip is laid out at the width its own pill measures, so titles never get cut early.
    // The buttons are reused: a search hides and shows them (see `CHIP_POOL`).
    ACTIONS.with(|actions| {
        POOL.with(|pool| {
            let (actions, pool) = (actions.borrow(), pool.borrow());
            let offered: Vec<&Command> = actions.iter().chain(pool.iter()).collect();
            prune_chip_pool(&offered);
        });
    });
    let chips: Vec<Rc<GlassButton>> = shown.iter().map(|cmd| chip_for(mtm, cmd)).collect();
    let widths: Vec<f64> = chips
        .iter()
        .map(|chip| commands::chip_width(chip.fitted_size().width))
        .collect();
    let inner = card_width() - PAD * 2.0;
    if SHOWS_IMAGE.with(Cell::get) && !meta.is_empty() {
        meta = display_image_meta(&meta, (inner - 10.0).max(40.0));
    }
    let version_bar = VERSION_BAR.with(|slot| slot.borrow().clone());
    let reserve = commands::trailing_reserve(version_bar.is_some());
    let frames = if shown.is_empty() {
        Vec::new()
    } else {
        commands::layout_chips(&widths, inner, reserve)
    };
    let show_empty = shown.is_empty() && searching && !query.trim().is_empty();
    // The field is hidden while the well is masked. The row stays, so a click to reveal does
    // not grow the card (and blurring it again does not shrink it).
    let find_shown = !well_is_masked();
    let strip_items = STRIP_ITEMS.with(|slot| slot.borrow().clone());
    let placed = place_sections(
        &meta,
        searching,
        &frames,
        show_empty,
        version_bar.is_some(),
        true,
        !strip_items.is_empty(),
    );
    place_window(mtm, placed.height, fresh_place);
    apply_window_chrome();
    let title = CARD_TITLE.with(|slot| header_title(&slot.borrow()));
    let button_h = mac_ui::metrics::BUTTON_H;
    let button_gap = mac_ui::metrics::BUTTON_GAP;
    let close_x = card_width() - PAD - button_h;
    let more_x = close_x - button_gap - button_h;
    let clear_w = clear_button_width();
    let clear_x = more_x - button_gap - clear_w;
    let symbols_w = symbols_button_width();
    let symbols_x = clear_x - button_gap - symbols_w;
    let title_w = (symbols_x - PAD - button_gap).max(40.0);
    let button_y = placed.header_y + (HEADER_H - button_h) / 2.0;
    HEADER.with(|slot| {
        set_header(slot, PAD, placed.header_y, title_w, HEADER_H, &title);
    });
    HEADER_GROUP.with(|slot| {
        if let Some(group) = slot.borrow().as_ref() {
            group.view().setFrame(NSRect::new(
                NSPoint::new(clear_x, button_y),
                NSSize::new(close_x + button_h - clear_x, button_h),
            ));
        }
    });
    CLEAR.with(|slot| place_header_button(slot, 0.0, clear_w));
    MORE.with(|slot| place_header_button(slot, more_x - clear_x, button_h));
    CLOSE.with(|slot| place_header_button(slot, close_x - clear_x, button_h));
    SYMBOLS.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.view().setFrame(NSRect::new(
                NSPoint::new(symbols_x, button_y),
                NSSize::new(symbols_w, button_h),
            ));
        }
    });
    place_item_find(placed.find_y, find_shown);
    place_well(placed.preview_y);
    apply_preview(placed.preview_y);
    place_content_actions(placed.preview_y);
    place_show_all(mtm, placed.preview_y);
    fit_text_well(placed.preview_y);
    // place_show_all / content actions may raise siblings; keep the picker on top.
    raise_picker();
    META.with(|slot| {
        let borrowed = slot.borrow();
        let Some(label) = borrowed.as_ref() else {
            return;
        };
        label.setFrame(NSRect::new(
            NSPoint::new(PAD, placed.meta_y),
            NSSize::new(inner, meta_block_height(&meta)),
        ));
        label.setHidden(meta.is_empty());
        if !meta.is_empty() {
            paint_warning_meta(label, &meta);
        }
    });
    FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            // Not setHidden: the field stays first responder while the card is open, so typing
            // (or "/") starts a search and the arrow keys move between chips. A hidden view
            // would give up focus. Out of search it has a zero frame, so it takes no clicks, and
            // alpha 0 keeps its focus ring from drawing.
            field.setHidden(false);
            let field_h = mac_ui::metrics::BUTTON_H;
            let (field_y, field_h, alpha) = if searching {
                (placed.search_y + (SEARCH_H - field_h) / 2.0, field_h, 1.0)
            } else {
                (0.0, 0.0, 0.0)
            };
            field.setFrame(NSRect::new(
                NSPoint::new(PAD, field_y),
                NSSize::new(inner, field_h),
            ));
            fade(field, alpha);
        }
    });
    PILLS.with(|slot| {
        if let Some(pills) = slot.borrow().as_ref() {
            pills.view().setFrame(NSRect::new(
                NSPoint::new(PAD, placed.chips_y),
                NSSize::new(inner, placed.chips_h),
            ));
            rebuild_pills(
                mtm,
                pills.content(),
                chips,
                &frames,
                show_empty,
                (inner - reserve).max(0.0),
            );
        }
    });
    let nav_y = placed.chips_y + placed.chips_h - commands::CHIP_PITCH
        + (commands::CHIP_PITCH - commands::CHIP_PILL_H) / 2.0;
    place_version_row(nav_y, version_bar.as_ref());
    place_strip(mtm, &placed, &strip_items);
    let selected = SELECTION.with(Cell::get);
    if shown.is_empty() {
        SELECTION.set(0);
    } else if selected >= shown.len() {
        SELECTION.set(shown.len() - 1);
    }
    SHOWN.with(|slot| set_commands(slot, shown));
    FRAMES.with(|slot| slot.replace(frames));
    paint_pills();
}

/// A fresh launcher opens just above (or below) the pointer, 8 pt inside the visible screen.
pub(crate) const NEAR_CURSOR: panel::NearCursor = panel::NearCursor {
    margin: 8.0,
    lead_x: 36.0,
    gap_y: 12.0,
};
/// Opened from the menu bar icon, it opens under the icon instead, like a menu.
pub(crate) const UNDER_ICON: panel::UnderIcon = panel::UnderIcon {
    margin: 8.0,
    gap_y: 6.0,
};

pub(crate) fn place_window(mtm: MainThreadMarker, height: f64, fresh: bool) {
    WINDOW.with(|slot| {
        let borrowed = slot.borrow();
        let Some(window) = borrowed.as_ref() else {
            return;
        };
        let placeholder = PLACEHOLDER.with(Cell::get);
        let height = commands::stable_height(placeholder, height, LAST_HEIGHT.with(Cell::get));
        if !placeholder {
            LAST_HEIGHT.set(Some(height));
        }
        let size = NSSize::new(card_width(), height);
        // A card already on screen (not fading out) stays where it is.
        let fresh = fresh && !(window.isVisible() && window.alphaValue() >= 0.99);
        let current = window.frame();
        if !fresh && current.size.width == size.width && current.size.height == size.height {
            return;
        }
        // Always under the menu bar icon (also when it grows, after the table window or a
        // copy), its height clamped to the screen; near the cursor only before the icon is known.
        let frame = match ICON_FRAME.get() {
            Some(icon) => panel::under_icon(mtm, icon, size, &UNDER_ICON),
            None if fresh => panel::near_cursor(mtm, size, &NEAR_CURSOR),
            None => panel::keep_top_left(window.frame(), size),
        };
        if !fresh && frame == current {
            return;
        }
        window.setFrame_display(frame, true);
    });
}

pub(crate) fn place_well(y: f64) {
    let frame = NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(card_width() - PAD * 2.0, PREVIEW_H),
    );
    WELL.with(|slot| {
        if let Some(well) = slot.borrow().as_ref() {
            well.setFrame(frame);
        }
    });
    place_spinner(frame);
    place_picker(y);
}
