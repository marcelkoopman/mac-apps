//! Painting the table window: chrome, content, meta line and mask.

use super::*;

/// Everything from the view shown: the toolbar, the meta line and (unless blanked for a
/// missing blur) the sidebar and grid; then the mask.
pub(super) fn paint() {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        apply_table_chrome(views);
        let empty = SHOWN.with(|shown| {
            let shown = shown.borrow();
            let Some(view) = shown.as_ref() else {
                return true;
            };
            paint_chrome(views, view);
            view.grid.is_empty()
        });
        // Replace the text in place. Wiping first flashes an empty well when the window
        // repaints for appearance or a new table.
        if empty {
            wipe_content(views);
        } else {
            PAINTED.set(false);
        }
    });
    apply_mask();
}

pub(super) fn paint_chrome(views: &Views, view: &TableWindowView) {
    views
        .window
        .setTitle(&NSString::from_str(&view.window_title()));
    views
        .table_button
        .button()
        .setEnabled(!view.menu.is_empty());
    // Enabled once the version's grid is there (not while "Working on the table…").
    let ready = !view.grid.is_empty();
    views.copy_button.button().setEnabled(ready);
    views.save_button.button().setEnabled(ready);
    views.undo.button().setEnabled(view.can_undo());
    views.redo.button().setEnabled(view.can_redo());
    views
        .title
        .setTitle(&NSString::from_str(&view.version_title()));
    if !view.versions.labels.is_empty() {
        let bar = VersionBar {
            labels: view.versions.labels.clone(),
            current: view.versions.current.min(view.versions.labels.len() - 1),
        };
        let spoken = NSString::from_str(&bar.spoken());
        views.title.setAccessibilityLabel(Some(&spoken));
        views.title.setToolTip(Some(&spoken));
    }
    paint_meta(&views.meta, &view.meta);
    let columns = view.columns.len();
    views
        .sidebar_title
        .setStringValue(&NSString::from_str(&match columns {
            0 => crate::locale::t("columns_title").to_string(),
            1 => crate::locale::t("one_column").to_string(),
            n => crate::locale::tf("n_columns", &[&n]),
        }));
    views
        .placeholder
        .setStringValue(&NSString::from_str(&view.placeholder));
    views.placeholder.setHidden(view.placeholder.is_empty());
}

/// The grid (with its frozen column) and the sidebar from the view shown.
pub(super) fn paint_content(views: &Views, view: &TableWindowView) {
    paint_table_grid(views, view);
    let mut text = String::new();
    let mut kinds = Vec::new();
    for column in &view.columns {
        text.push_str(&column.shown);
        text.push_str("  ");
        let start = text.len();
        text.push_str(&column.kind);
        kinds.push(start..text.len());
        text.push('\n');
    }
    let attr = AttrText::new(
        &text,
        &NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION),
        &NSColor::labelColor(),
    );
    let dim = if crate::appearance::display().increase_contrast {
        crate::appearance::caption_color()
    } else {
        NSColor::tertiaryLabelColor()
    };
    for range in &kinds {
        attr.color(range, &dim);
    }
    if let Some(storage) = mac_ui::widgets::text_storage(&views.sidebar) {
        storage.setAttributedString(&attr.into_attributed());
    }
    zeroize::Zeroize::zeroize(&mut text);
    widgets::fit_text_view(&views.sidebar, Some(SIDEBAR_W - PAD));
    PAINTED.set(true);
}

/// The meta line, its sensitivity labels marked as on the card.
pub(super) fn paint_meta(label: &NSTextField, meta: &str) {
    let flat = meta.replace('\n', "  ·  ");
    let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    let attr = AttrText::new(&flat, &font, &crate::appearance::caption_color());
    let warn_font = NSFont::boldSystemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    for mark in crate::sensitivity::warning_marks(&flat) {
        let range = mark.start..mark.end;
        let (ink, wash) = match mark.label {
            crate::sensitivity::Label::Pii => (NSColor::blackColor(), NSColor::systemYellowColor()),
            crate::sensitivity::Label::Financial | crate::sensitivity::Label::Credential => {
                (NSColor::whiteColor(), NSColor::systemRedColor())
            }
        };
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

/// Blur (or blank) the sidebar and grid while masked, sharp once revealed.
pub(super) fn apply_mask() {
    let masked = masked();
    let filter = if masked { blur_filter() } else { None };
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        let checking = SHOWN.with(|shown| {
            let shown = shown.borrow();
            let Some(view) = shown.as_ref() else {
                return false;
            };
            // Without a blur, masked content is not drawn at all.
            let blank = masked && filter.is_none();
            if blank {
                wipe_content(views);
            } else if !PAINTED.get() && !view.grid.is_empty() {
                paint_content(views, view);
            }
            view.checking
        });
        let filters = match &filter {
            Some(filter) => NSArray::from_slice(&[&**filter]),
            None => NSArray::from_slice(&[]),
        };
        for text in [&views.grid, &views.frozen, &views.sidebar, &views.header] {
            mac_ui::blur::set_content_filters(text, &filters);
            text.setAccessibilityElement(!masked);
        }
        views.shade.setHidden(!(masked && filter.is_none()));
        views.cover.setHidden(!masked);
        let hint = if !masked {
            ""
        } else if checking {
            crate::locale::t("checking_sensitive")
        } else {
            crate::locale::t("click_table_to_reveal")
        };
        views.hint.setStringValue(&NSString::from_str(hint));
    });
}
