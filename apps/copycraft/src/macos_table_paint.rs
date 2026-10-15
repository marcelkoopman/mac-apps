// The table window's grid, once `grid_sheet` can read it: proportional figures, right-aligned
// numbers, a header strip that stays put while the body scrolls, and system row stripes.
// A grid that does not parse keeps the boxed text and its frozen first column.

thread_local! {
    /// The body is a column sheet, so row stripes are drawn. Off for the boxed fallback.
    static TABLE_STRIPES: Cell<bool> = const { Cell::new(false) };
    /// Line height the stripes use, the same height the paragraph style locks in.
    static ROW_LINE_H: Cell<f64> = const { Cell::new(0.0) };
    /// Column separators of the sheet, x in the grid view (inset included).
    static COLUMN_LINES: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
    /// The separators in the header strip, thin boxes on the header text.
    static HEADER_LINES: RefCell<Vec<Retained<NSBox>>> = const { RefCell::new(Vec::new()) };
}

const HEADER_STRIP: f64 = 28.0;
const TABLE_FONT: f64 = 13.0;
/// Medium weight. The bindings have no `NSFontWeightMedium` constant (regular is 0.0).
const HEADER_WEIGHT: f64 = 0.23;
const BODY_INSET_X: f64 = 12.0;

fn table_grid_view(mtm: MainThreadMarker) -> Retained<NSTextView> {
    let allocated = TableGridView::alloc(mtm);
    // SAFETY: initWithFrame: is NSTextView's initializer. The caller retains the view.
    let view: Retained<TableGridView> =
        unsafe { msg_send![allocated, initWithFrame: NSRect::ZERO] };
    let text = view.into_super();
    text.setEditable(false);
    text.setSelectable(true);
    text.setUsesFindPanel(false);
    text.setDrawsBackground(false);
    text.setRichText(true);
    text.setTextContainerInset(NSSize::new(BODY_INSET_X, 10.0));
    text
}

fn draw_table_background(view: &NSTextView, dirty: NSRect) {
    draw_stripes(view, dirty);
    if TABLE_STRIPES.with(Cell::get) {
        draw_column_lines(dirty);
    }
}

/// Thin 1 pt lines between the columns, in the adaptive separator colour, over the stripes.
fn draw_column_lines(dirty: NSRect) {
    COLUMN_LINES.with(|lines| {
        let lines = lines.borrow();
        if lines.is_empty() {
            return;
        }
        NSColor::separatorColor().setFill();
        for &x in lines.iter() {
            if x + 1.0 < dirty.origin.x || x > dirty.origin.x + dirty.size.width {
                continue;
            }
            NSBezierPath::fillRect(NSRect::new(
                NSPoint::new(x, dirty.origin.y),
                NSSize::new(1.0, dirty.size.height),
            ));
        }
    });
}

/// The same separators in the header strip, as thin boxes on the header text view (they
/// scroll sideways with it).
fn place_header_lines(views: &Views, xs: &[f64]) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    HEADER_LINES.with(|slot| {
        let mut boxes = slot.borrow_mut();
        for line in boxes.drain(..) {
            line.removeFromSuperview();
        }
        for &x in xs {
            let line = widgets::filled_box(mtm, 0.0, &NSColor::separatorColor());
            line.setFrame(NSRect::new(
                NSPoint::new(x, 0.0),
                NSSize::new(1.0, HEADER_STRIP),
            ));
            views.header.addSubview(&line);
            boxes.push(line);
        }
    });
}

fn draw_stripes(view: &NSTextView, dirty: NSRect) {
    if !TABLE_STRIPES.with(Cell::get) {
        NSColor::controlBackgroundColor().setFill();
        NSBezierPath::fillRect(dirty);
        return;
    }
    let line_h = ROW_LINE_H.with(Cell::get);
    let colors = NSColor::alternatingContentBackgroundColors();
    if colors.count() == 0 || line_h <= 0.0 {
        NSColor::controlBackgroundColor().setFill();
        NSBezierPath::fillRect(dirty);
        return;
    }
    let even = colors.objectAtIndex(0);
    even.setFill();
    NSBezierPath::fillRect(dirty);
    if colors.count() < 2 {
        return;
    }
    let odd = colors.objectAtIndex(1);
    odd.setFill();
    let bounds = view.bounds();
    let inset = view.textContainerInset().height;
    let flipped = view.isFlipped();
    let (start, end) = if flipped {
        (dirty.origin.y, dirty.origin.y + dirty.size.height)
    } else {
        let top = bounds.size.height - (dirty.origin.y + dirty.size.height);
        (top, top + dirty.size.height)
    };
    let first = ((start - inset) / line_h).floor().max(0.0) as usize;
    let last = (((end - inset) / line_h).ceil() - 1.0).max(0.0) as usize;
    if first > last {
        return;
    }
    for row in first..=last {
        if row % 2 == 0 {
            continue;
        }
        let top = inset + row as f64 * line_h;
        let y = if flipped {
            top
        } else {
            bounds.size.height - top - line_h
        };
        NSBezierPath::fillRect(NSRect::new(
            NSPoint::new(bounds.origin.x, y),
            NSSize::new(bounds.size.width, line_h),
        ));
    }
}

fn on_grid_scrolled() {
    VIEWS.with(|slot| {
        // Paint already holds `VIEWS` when a bounds notification fires from laying out the
        // text. Skip here; the paint path syncs the header after it lets go of the cells.
        let Ok(borrowed) = slot.try_borrow() else {
            return;
        };
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        sync_header_from_views(views);
    });
}

fn sync_header_from_views(views: &Views) {
    let Some(scroll) = views.grid.enclosingScrollView() else {
        return;
    };
    let x = scroll.contentView().bounds().origin.x;
    let frame = views.header.frame();
    if (frame.origin.x + x).abs() < 0.5 {
        return;
    }
    views
        .header
        .setFrameOrigin(NSPoint::new(-x, frame.origin.y));
}

fn place_table_header(views: &Views, shown: bool) {
    let Some(scroll) = views.grid.enclosingScrollView() else {
        return;
    };
    let Some(clip) = mac_ui::widgets::superview(&scroll) else {
        return;
    };
    let size = clip.bounds().size;
    let want_h = if shown {
        (size.height - HEADER_STRIP).max(0.0)
    } else {
        size.height
    };
    let scroll_size = scroll.frame().size;
    let same = views.header_clip.isHidden() == !shown
        && (scroll_size.height - want_h).abs() < 0.5
        && (scroll_size.width - size.width).abs() < 0.5;
    if same {
        return;
    }
    views.header_clip.setHidden(!shown);
    if !shown {
        scroll.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
        return;
    }
    views.header_clip.setFrame(NSRect::new(
        NSPoint::new(0.0, (size.height - HEADER_STRIP).max(0.0)),
        NSSize::new(size.width, HEADER_STRIP),
    ));
    views.header_back.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(size.width, HEADER_STRIP),
    ));
    views.header_rule.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(size.width, 1.0),
    ));
    scroll.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(size.width, want_h),
    ));
}

fn paint_table_grid(views: &Views, view: &TableWindowView) {
    let mut previous = views.grid.string().to_string();
    if let Some(mut sheet) = crate::dataframe::grid_sheet(&view.grid) {
        paint_aligned_table(views, &sheet, &previous);
        for header in &mut sheet.headers {
            header.zeroize();
        }
        for row in &mut sheet.rows {
            for cell in row {
                cell.zeroize();
            }
        }
    } else {
        paint_boxed_grid(views, view, &previous);
    }
    previous.zeroize();
}

fn paint_aligned_table(views: &Views, sheet: &crate::dataframe::GridSheet, previous: &str) {
    let was_sheet = TABLE_STRIPES.with(Cell::get);
    let (body_font, header_font) = table_fonts();
    let em = text_width("M", &header_font);
    let digit = text_width("0", &body_font);
    let mut layout = crate::dataframe::layout_table(sheet, em, digit);
    let body_changed = previous != layout.body;
    let mut header_now = views.header.string().to_string();
    let header_changed = header_now != layout.header;
    header_now.zeroize();
    let line_h = line_height_for(&views.grid, &body_font);
    ROW_LINE_H.set(line_h);
    // Separators sit mid-gap; the text starts after the container inset in both views.
    let xs: Vec<f64> = layout
        .separators()
        .into_iter()
        .map(|x| x + BODY_INSET_X)
        .collect();
    place_header_lines(views, &xs);
    COLUMN_LINES.with(|slot| slot.replace(xs));
    TABLE_STRIPES.set(true);
    views.grid.setDrawsBackground(true);
    views.frozen.setHidden(true);
    place_table_header(views, true);
    // A new header can move the tab stops while the body text stays the same. Reapply both,
    // and only jump back to the top when the body itself changed.
    if body_changed || header_changed || !was_sheet {
        views.grid.setFont(Some(&body_font));
        views.header.setFont(Some(&header_font));
        let pad_y = ((HEADER_STRIP - line_h) / 2.0).max(0.0);
        set_line_padding(&views.grid, NSSize::new(BODY_INSET_X, 4.0), 0.0);
        set_line_padding(&views.header, NSSize::new(BODY_INSET_X, pad_y), 0.0);
        let style = table_paragraph(&layout.stops, line_h);
        set_styled_text(&views.grid, &layout.body, &body_font, &style, &NSColor::labelColor());
        // Column names take the app accent (the icon blue under Multicolour), like CSV headers.
        set_styled_text(
            &views.header,
            &layout.header,
            &header_font,
            &style,
            &NSColor::controlAccentColor(),
        );
        widgets::fit_text_view(&views.grid, None);
        size_header(views, layout.content_width());
        if body_changed {
            views.grid.scrollRangeToVisible(NSRange {
                location: 0,
                length: 0,
            });
        }
    }
    widgets::wipe_text_view(&views.frozen);
    sync_header_from_views(views);
    views.grid.setNeedsDisplay(true);
    views.header.setNeedsDisplay(true);
    layout.header.zeroize();
    layout.body.zeroize();
}

fn paint_boxed_grid(views: &Views, view: &TableWindowView, previous: &str) {
    // Light and dark repaint the same grid. Leave the scroll position and the frozen column.
    if !TABLE_STRIPES.with(Cell::get) && previous == view.grid {
        return;
    }
    TABLE_STRIPES.set(false);
    COLUMN_LINES.with(|slot| slot.borrow_mut().clear());
    place_header_lines(views, &[]);
    views.grid.setDrawsBackground(false);
    set_line_padding(&views.grid, NSSize::new(12.0, 10.0), 5.0);
    place_table_header(views, false);
    widgets::wipe_text_view(&views.header);
    crate::macos_card_text::paint(
        &views.grid,
        &view.grid,
        Some(FormatKind::Dataframe),
        true,
        None,
        false,
    );
    widgets::fit_text_view(&views.grid, None);
    views.grid.scrollRangeToVisible(NSRange {
        location: 0,
        length: 0,
    });
    match crate::dataframe::frozen_column(&view.grid) {
        Some(lines) => {
            crate::macos_card_text::copy_frozen_column(&views.grid, &views.frozen, &lines);
            views.frozen.setHidden(false);
        }
        None => {
            widgets::wipe_text_view(&views.frozen);
            views.frozen.setHidden(true);
        }
    }
}

fn table_fonts() -> (Retained<NSFont>, Retained<NSFont>) {
    (
        NSFont::monospacedSystemFontOfSize_weight(TABLE_FONT, 0.0),
        NSFont::monospacedSystemFontOfSize_weight(TABLE_FONT, HEADER_WEIGHT),
    )
}

fn text_width(sample: &str, font: &NSFont) -> f64 {
    let attr = AttrText::new(sample, font, &NSColor::labelColor()).into_attributed();
    attr.size().width.max(1.0)
}

fn line_height_for(text: &NSTextView, font: &NSFont) -> f64 {
    let height = mac_ui::widgets::layout_manager(text)
        .map(|manager| manager.defaultLineHeightForFont(font))
        .unwrap_or(0.0);
    if height > 0.0 { height } else { 16.0 }
}

fn set_line_padding(text: &NSTextView, inset: NSSize, padding: f64) {
    text.setTextContainerInset(inset);
    if let Some(container) = mac_ui::widgets::text_container(text) {
        container.setLineFragmentPadding(padding);
    }
}

fn table_paragraph(stops: &[(f64, bool)], line_h: f64) -> Retained<NSMutableParagraphStyle> {
    let style = NSMutableParagraphStyle::new();
    style.setAlignment(NSTextAlignment::Left);
    style.setLineBreakMode(NSLineBreakMode::ByClipping);
    style.setMinimumLineHeight(line_h);
    style.setMaximumLineHeight(line_h);
    style.setDefaultTabInterval(10_000.0);
    let options = NSDictionary::<NSString, AnyObject>::new();
    let mut tabs = Vec::with_capacity(stops.len());
    for &(location, right) in stops {
        let align = if right {
            NSTextAlignment::Right
        } else {
            NSTextAlignment::Left
        };
        // SAFETY: `options` is an empty dictionary; this initializer accepts any tab-option map.
        let tab = unsafe {
            NSTextTab::initWithTextAlignment_location_options(
                NSTextTab::alloc(),
                align,
                location,
                &options,
            )
        };
        tabs.push(tab);
    }
    let refs: Vec<&NSTextTab> = tabs.iter().map(|tab| tab.as_ref()).collect();
    style.setTabStops(Some(&NSArray::from_slice(&refs)));
    style
}

fn set_styled_text(
    view: &NSTextView,
    text: &str,
    font: &NSFont,
    style: &NSMutableParagraphStyle,
    color: &NSColor,
) {
    let attr = AttrText::new(text, font, color).into_attributed();
    let range = NSRange {
        location: 0,
        length: attr.length(),
    };
    mac_ui::text::set_paragraph_style(&attr, style, range);
    if let Some(storage) = mac_ui::widgets::text_storage(view) {
        storage.setAttributedString(&attr);
    }
}

fn size_header(views: &Views, content_width: f64) {
    widgets::fit_text_view(&views.header, None);
    let frame = views.header.frame();
    let clip_w = views.header_clip.bounds().size.width;
    // Insets sit outside the tab stops, so the view has to be wider than the columns.
    let fitted = frame
        .size
        .width
        .max(content_width + BODY_INSET_X * 2.0);
    views.header.setFrame(NSRect::new(
        NSPoint::new(frame.origin.x, 0.0),
        NSSize::new(fitted.max(clip_w), HEADER_STRIP),
    ));
}

/// Hide the sheet header and turn stripes off. Used when the window is wiped.
fn clear_sheet_chrome(views: &Views) {
    widgets::wipe_text_view(&views.header);
    place_table_header(views, false);
    TABLE_STRIPES.set(false);
    COLUMN_LINES.with(|slot| slot.borrow_mut().clear());
    place_header_lines(views, &[]);
    views.grid.setDrawsBackground(false);
}
