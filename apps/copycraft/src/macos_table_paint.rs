// The table window's grid, once `grid_sheet` can read it: proportional figures, right-aligned
// numbers, a header strip that stays put while the body scrolls, and system row stripes.
// A grid that does not parse keeps the boxed text and its frozen first column.

thread_local! {
    /// The body is a column sheet, so row stripes are drawn. Off for the boxed fallback.
    static TABLE_STRIPES: Cell<bool> = const { Cell::new(false) };
    /// The sheet's first column is numeric: each body line starts with a tab (its right stop).
    static FIRST_NUMERIC: Cell<bool> = const { Cell::new(false) };
    /// Line height the stripes use, the same height the paragraph style locks in.
    static ROW_LINE_H: Cell<f64> = const { Cell::new(0.0) };
    /// Column separators of the sheet, x in the grid view (inset included).
    static COLUMN_LINES: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
    /// The separators in the header strip, thin boxes on the header text.
    static HEADER_LINES: RefCell<Vec<Retained<NSBox>>> = const { RefCell::new(Vec::new()) };
    /// While a search filters the sheet: the table row of each drawn row. `None`: all rows.
    static SHOWN_ROWS: RefCell<Option<Vec<usize>>> = const { RefCell::new(None) };
    /// The shown view's grid read into columns and laid out (on the worker that built the
    /// view, or here once); kept for the search. Zeroized when dropped.
    static PREPARED: RefCell<Option<std::sync::Arc<crate::dataframe::PreparedSheet>>> =
        const { RefCell::new(None) };
    /// Numbers the searches of a large sheet on a thread ([`commands::SearchGeneration`]).
    static SEARCH_GENERATION: commands::SearchGeneration = commands::SearchGeneration::default();
    /// The search on its way on a thread: its generation, the sheet's body key and the query.
    static SEARCH_PENDING: RefCell<Option<(u64, u64, zeroize::Zeroizing<String>)>> =
        const { RefCell::new(None) };
    /// The search a thread finished, for the next paint (see [`SEARCH_PENDING`]).
    static SEARCH_DONE: RefCell<Option<ThreadSearch>> = const { RefCell::new(None) };
    /// [`crate::dataframe::body_key`] of the sheet body in the grid; 0 when it holds none.
    static BODY_KEY: Cell<u64> = const { Cell::new(0) };
    /// The font widths the sheet is laid out with (a wide letter, a figure), measured once.
    static GRID_METRICS: Cell<Option<(f64, f64)>> = const { Cell::new(None) };
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
    // A large table is laid out as it scrolls into view, not all at once when it is set: its
    // rows all have the same height, so the sheet sizes the view itself ([`size_sheet_grid`]).
    if let Some(manager) = mac_ui::widgets::layout_manager(&text) {
        manager.setAllowsNonContiguousLayout(true);
    }
    text
}

/// The sheet's font widths (a wide letter in the header font, a figure in the body font), for
/// laying a grid out on a worker ([`crate::dataframe::prepare_sheet`]). Main thread only.
pub(crate) fn grid_metrics() -> Option<(f64, f64)> {
    MainThreadMarker::new()?;
    if let Some(metrics) = GRID_METRICS.with(Cell::get) {
        return Some(metrics);
    }
    let (body_font, header_font) = table_fonts();
    let metrics = (
        text_width("M", &header_font),
        text_width("0", &body_font),
    );
    GRID_METRICS.set(Some(metrics));
    Some(metrics)
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
    let palette = crate::appearance::card_palette();
    if !TABLE_STRIPES.with(Cell::get) {
        crate::appearance::table_grid_colours().2.setFill();
        NSBezierPath::fillRect(dirty);
        return;
    }
    let line_h = ROW_LINE_H.with(Cell::get);
    // A fixed card theme stripes in its background and a step toward its text (tested).
    if let Some(p) = palette
        && line_h > 0.0
    {
        crate::appearance::srgb(p.background).setFill();
        NSBezierPath::fillRect(dirty);
        crate::appearance::srgb(p.table_stripe()).setFill();
        fill_odd_rows(view, dirty, line_h);
        return;
    }
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
    fill_odd_rows(view, dirty, line_h);
}

/// Fill the odd rows inside `dirty` with the colour set.
fn fill_odd_rows(view: &NSTextView, dirty: NSRect, line_h: f64) {
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

/// A search of a large sheet done on a thread: for the sheet with `body_key` and `query`, the
/// matching rows and the body with only their lines. Zeroized when dropped.
struct ThreadSearch {
    body_key: u64,
    query: zeroize::Zeroizing<String>,
    matches: Vec<usize>,
    kept: zeroize::Zeroizing<String>,
}

/// What a search leaves to draw: the matching rows and their lines (with the body key).
type Filtered = (Vec<usize>, zeroize::Zeroizing<String>);

/// The search's rows for `prepared`: done here for a small sheet, on a thread for a large one
/// ([`commands::search_on_thread`]). `Err(())` while that thread is still at it: the grid on
/// screen stays until its result comes.
fn search_sheet(
    prepared: &std::sync::Arc<crate::dataframe::PreparedSheet>,
    query: &str,
) -> Result<Option<Filtered>, ()> {
    let sheet = &prepared.sheet;
    let key = prepared.body_key;
    if commands::table_search_needle(query).is_none()
        || !commands::search_on_thread(sheet.rows.len(), sheet.headers.len())
    {
        // A search here makes any on a thread stale.
        bump_search_generation();
        SEARCH_PENDING.with(|slot| slot.replace(None));
        return Ok(commands::filter_sheet(
            &sheet.rows,
            &prepared.layout.body,
            query,
            || true,
        ));
    }
    let done = SEARCH_DONE.with(|slot| slot.borrow_mut().take());
    if let Some(done) = done.filter(|done| done.body_key == key && *done.query == query) {
        // Kept for repaints of the same search (a resize, the privacy filter).
        let filtered = (done.matches.clone(), done.kept.clone());
        SEARCH_DONE.with(|slot| slot.replace(Some(done)));
        return Ok(Some(filtered));
    }
    let running = SEARCH_PENDING.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|(_, pending_key, pending)| *pending_key == key && **pending == query)
    });
    if !running && !start_thread_search(prepared.clone(), query) {
        // No thread to be had: search here, slow but right.
        return Ok(commands::filter_sheet(
            &sheet.rows,
            &prepared.layout.body,
            query,
            || true,
        ));
    }
    Err(())
}

fn bump_search_generation() -> u64 {
    SEARCH_GENERATION.with(commands::SearchGeneration::start)
}

/// Search `prepared` for `query` on a thread; the window repaints with the result when it is
/// still the newest search ([`commands::SearchGeneration`]). A newer search, closing the window
/// or Clear stops it within [`commands::SEARCH_CHECK_ROWS`] rows, and the sheet it holds goes.
/// Nothing is logged.
fn start_thread_search(
    prepared: std::sync::Arc<crate::dataframe::PreparedSheet>,
    query: &str,
) -> bool {
    let generation = bump_search_generation();
    let counter = SEARCH_GENERATION.with(Clone::clone);
    let key = prepared.body_key;
    let query = zeroize::Zeroizing::new(query.to_string());
    SEARCH_PENDING.with(|slot| slot.replace(Some((generation, key, query.clone()))));
    let spawned = std::thread::Builder::new()
        .name("copycraft-table-search".into())
        .spawn(move || {
            let found = commands::filter_sheet(
                &prepared.sheet.rows,
                &prepared.layout.body,
                &query,
                || counter.current(generation),
            );
            // The sheet goes before the main thread is asked: the window may have dropped it.
            drop(prepared);
            let done = found.map(|(matches, kept)| ThreadSearch {
                body_key: key,
                query,
                matches,
                kept,
            });
            mac_ui::actions::run_on_main(move || thread_search_done(generation, done));
        });
    match spawned {
        Ok(_) => true,
        Err(e) => {
            SEARCH_PENDING.with(|slot| slot.replace(None));
            crate::diag::log(crate::diag::LogEvent::WorkNotStarted(
                crate::diag::Work::Lane,
                crate::diag::ErrorKind::io(&e),
            ));
            false
        }
    }
}

/// A search on a thread is done: drawn when it is still the newest, else dropped (wiped).
fn thread_search_done(generation: u64, done: Option<ThreadSearch>) {
    if !SEARCH_GENERATION.with(|counter| counter.current(generation)) {
        return;
    }
    SEARCH_PENDING.with(|slot| slot.replace(None));
    let Some(done) = done else {
        return;
    };
    SEARCH_DONE.with(|slot| slot.replace(Some(done)));
    PAINTED.set(false);
    apply_mask();
}

/// Forget the searches on a thread (the window is wiped): a result still on its way is dropped.
fn forget_thread_search() {
    bump_search_generation();
    switching::search_pending(false);
    SEARCH_PENDING.with(|slot| slot.replace(None));
    SEARCH_DONE.with(|slot| slot.replace(None));
}

fn paint_table_grid(views: &Views, view: &TableWindowView) {
    // Read into columns on the worker that built the view; only a view that came without
    // (or a repaint after the window was wiped) is read here.
    let prepared = PREPARED.with(|slot| slot.borrow_mut().take()).or_else(|| {
        let (em, digit) = grid_metrics()?;
        crate::dataframe::prepare_sheet(&view.grid, em, digit).map(std::sync::Arc::new)
    });
    if let Some(prepared) = prepared {
        let sheet = &prepared.sheet;
        // The toolbar's search: only the rows with a cell that matches are drawn.
        let query = SEARCH.with(|query| zeroize::Zeroizing::new(query.borrow().clone()));
        let Ok(filtered) = search_sheet(&prepared, &query) else {
            // A large sheet's search is on a thread: the grid drawn (perhaps of the version
            // before) stays, dimmed, until it is done. Its rows no longer map to this sheet.
            SHOWN_ROWS.with(|slot| slot.replace(None));
            switching::search_pending(true);
            PREPARED.with(|slot| slot.replace(Some(prepared)));
            return;
        };
        switching::search_pending(false);
        paint_aligned_table(views, &prepared, filtered.as_ref());
        let mut meta = match &filtered {
            Some((found, _)) => {
                let columns = match view.columns.len() {
                    0 => sheet.headers.len(),
                    n => n,
                };
                commands::filtered_meta(&view.meta, found.len(), view.rows, columns)
            }
            None => view.meta.clone(),
        };
        paint_meta(&views.meta, &meta);
        meta.zeroize();
        SHOWN_ROWS.with(|slot| slot.replace(filtered.map(|(found, _)| found)));
        PREPARED.with(|slot| slot.replace(Some(prepared)));
    } else {
        SHOWN_ROWS.with(|slot| slot.replace(None));
        BODY_KEY.set(0);
        let mut previous = views.grid.string().to_string();
        paint_boxed_grid(views, view, &previous);
        previous.zeroize();
    }
}

fn paint_aligned_table(
    views: &Views,
    prepared: &crate::dataframe::PreparedSheet,
    filtered: Option<&Filtered>,
) {
    let was_sheet = TABLE_STRIPES.with(Cell::get);
    let sheet = &prepared.sheet;
    let layout = &prepared.layout;
    let (body_font, header_font) = table_fonts();
    // Column widths come from every row, so they hold still while a search narrows the rows.
    let (body, key) = match filtered {
        Some((_, kept)) => (kept.as_str(), crate::dataframe::body_key(kept)),
        None => (layout.body.as_str(), prepared.body_key),
    };
    let rows = filtered.map_or(sheet.rows.len(), |(found, _)| found.len());
    // Compared by fingerprint: no copy of a large grid's text back out of the view.
    let body_changed = BODY_KEY.get() != key;
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
    FIRST_NUMERIC.set(sheet.numeric.first().copied().unwrap_or(false));
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
        // Body in the card theme's text colour; column names take its accent (the app
        // accent, the icon blue under Multicolour, with System), like CSV headers.
        let (text_colour, header_colour, _) = crate::appearance::table_grid_colours();
        set_styled_text(&views.grid, body, &body_font, &style, &text_colour);
        set_styled_text(
            &views.header,
            &layout.header,
            &header_font,
            &style,
            &header_colour,
        );
        size_sheet_grid(&views.grid, rows, layout.content_width(), line_h);
        size_header(views, layout.content_width());
        BODY_KEY.set(key);
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
}

/// Size the grid to a sheet of `rows` rows without laying all of its text out (a large table
/// took the main thread for that): every row is `line_h` high (the paragraph style locks it,
/// as the stripes assume) and the columns end at `content_width`. The text view does not
/// resize itself to its (estimated) layout.
fn size_sheet_grid(grid: &NSTextView, rows: usize, content_width: f64, line_h: f64) {
    widgets::scroll_text_both_ways(grid);
    grid.setHorizontallyResizable(false);
    grid.setVerticallyResizable(false);
    let inset = grid.textContainerInset();
    let width = content_width + inset.width * 2.0;
    let height = rows.max(1) as f64 * line_h + inset.height * 2.0;
    grid.setFrameSize(NSSize::new(width.max(1.0), height.max(1.0)));
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
    SHOWN_ROWS.with(|slot| slot.replace(None));
    BODY_KEY.set(0);
    TABLE_STRIPES.set(false);
    COLUMN_LINES.with(|slot| slot.borrow_mut().clear());
    place_header_lines(views, &[]);
    views.grid.setDrawsBackground(false);
}
