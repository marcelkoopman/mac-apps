//! Building the table window's views: sidebar, grid, meta line and cover (the toolbar is
//! `toolbar`).

use super::*;

pub(super) fn wire(control: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(control, delegate, action);
        }
    });
}

pub(super) fn mask(options: &[NSAutoresizingMaskOptions]) -> NSAutoresizingMaskOptions {
    options
        .iter()
        .fold(NSAutoresizingMaskOptions::empty(), |all, one| all | *one)
}

pub(super) fn ensure_views(mtm: MainThreadMarker) {
    if VIEWS.with(|slot| slot.borrow().is_some()) {
        return;
    }
    use NSAutoresizingMaskOptions as A;
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), INITIAL_SIZE);
    // SAFETY: initWithContentRect:styleMask:backing:defer: on a freshly allocated window, with
    // plain value arguments.
    let window: Retained<TableWindowPanel> = unsafe {
        msg_send![
            TableWindowPanel::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    mac_ui::panel::keep_when_closed(&window);
    window.setTitle(&NSString::from_str(crate::locale::t("table_window_title")));
    window.setContentMinSize(MIN_SIZE);
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    // Like the card: out of screenshots, recordings and screen sharing where macOS honours it
    // (not ScreenCaptureKit on macOS 15.4+, by reports; see `macos_launcher::ensure_window`),
    // unless "Allow Screenshots" is on.
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();
    let delegate = TableWindowDelegate::new(mtm);
    // SAFETY: DELEGATE keeps the delegate alive for the rest of the process, like VIEWS.
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let (width, height) = (INITIAL_SIZE.width, INITIAL_SIZE.height);
    // The toolbar sits in the title bar, outside the content view.
    let body_h = height - META_H;

    // Shown in the toolbar while the grid is masked ("Click the table to reveal it").
    let hint = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    hint.setAlignment(NSTextAlignment::Center);
    hint.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    toolbar::install(mtm, &window, &hint);

    // The meta line along the bottom.
    let meta = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    meta.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    meta.setUsesSingleLineMode(true);
    meta.setMaximumNumberOfLines(1);
    meta.setFrame(NSRect::new(
        NSPoint::new(PAD, (META_H - 18.0) / 2.0),
        NSSize::new(width - PAD * 2.0, 18.0),
    ));
    meta.setAutoresizingMask(mask(&[A::ViewMaxYMargin, A::ViewWidthSizable]));
    content.addSubview(&meta);

    // The sidebar: "Columns" and one line per column (name, type).
    let sidebar_title = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    sidebar_title.setFrame(NSRect::new(
        NSPoint::new(PAD, META_H + body_h - SIDEBAR_TITLE_H),
        NSSize::new(SIDEBAR_W - PAD * 2.0, 18.0),
    ));
    sidebar_title.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(&sidebar_title);
    let sidebar = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    sidebar.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("columns_title"))));
    let sidebar_scroll = widgets::passive_text_scroll(mtm, &sidebar);
    sidebar_scroll.setFrame(NSRect::new(
        NSPoint::new(PAD - 4.0, META_H),
        NSSize::new(SIDEBAR_W - PAD, body_h - SIDEBAR_TITLE_H - 4.0),
    ));
    sidebar_scroll.setAutoresizingMask(mask(&[A::ViewHeightSizable, A::ViewMaxXMargin]));
    content.addSubview(&sidebar_scroll);

    // The grid in a well, as on the card, with its first column frozen.
    let grid_frame = NSRect::new(
        NSPoint::new(SIDEBAR_W, META_H),
        NSSize::new(width - SIDEBAR_W - PAD, body_h),
    );
    let well = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::textBackgroundColor());
    crate::appearance::apply_well(&well, false, true);
    well.setFrame(grid_frame);
    well.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&well);
    let grid = table_grid_view(mtm);
    grid.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("table_view"))));
    widgets::scroll_text_both_ways(&grid);
    // Clip host around the scroller: clips the tall floating first column without
    // setClipsToBounds on the NSScrollView (which breaks scrolling).
    let grid_clip = NSView::initWithFrame(NSView::alloc(mtm), grid_frame);
    grid_clip.setClipsToBounds(true);
    grid_clip.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&grid_clip);
    let grid_scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), grid_frame.size),
    );
    widgets::configure_text_scroll(&grid_scroll, &grid);
    grid_scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    grid_clip.addSubview(&grid_scroll);
    let clip_view = grid_scroll.contentView();
    clip_view.setPostsBoundsChangedNotifications(true);
    if let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) {
        let observer: &AnyObject = &delegate;
        let object: &AnyObject = &clip_view;
        // SAFETY: tableWindowGridScrolled: is a TableWindowDelegate method taking the
        // notification. DELEGATE keeps the observer alive. The clip view is the object and
        // stays in this window.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                observer,
                sel!(tableWindowGridScrolled:),
                Some(NSViewBoundsDidChangeNotification),
                Some(object),
            );
        }
    }
    let frozen = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    frozen.setDrawsBackground(true);
    // Opaque plate: the same colour the grid paints, so the scrolling body does not show
    // through the frozen column and the two stay one surface.
    frozen.setBackgroundColor(&NSColor::controlBackgroundColor());
    // A copy of what the grid says: VoiceOver reads the grid itself.
    frozen.setAccessibilityElement(false);
    frozen.setHidden(true);
    widgets::scroll_text_both_ways(&frozen);
    grid_scroll.addFloatingSubview_forAxis(&frozen, NSEventGestureAxis::Horizontal);
    // Header strip above the scroller (non-flipped: pinned to the top of the clip). The body
    // scroll view keeps a top margin of the same height while the sheet is showing.
    let header_clip = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    header_clip.setClipsToBounds(true);
    header_clip.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMinYMargin]));
    header_clip.setHidden(true);
    let header_back = widgets::filled_box(mtm, 0.0, &NSColor::controlBackgroundColor());
    header_back.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    header_clip.addSubview(&header_back);
    let header = widgets::read_only_text_view(mtm, NSSize::new(12.0, 4.0));
    header.setDrawsBackground(true);
    header.setBackgroundColor(&NSColor::controlBackgroundColor());
    widgets::scroll_text_both_ways(&header);
    header_clip.addSubview(&header);
    let header_rule = widgets::filled_box(mtm, 0.0, &NSColor::separatorColor());
    header_rule.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMaxYMargin]));
    header_rule.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 1.0)));
    header_clip.addSubview(&header_rule);
    grid_clip.addSubview(&header_clip);
    let placeholder = widgets::label(
        mtm,
        mac_ui::metrics::FONT_BODY,
        &crate::appearance::caption_color(),
    );
    placeholder.setAlignment(NSTextAlignment::Center);
    placeholder.setFrame(NSRect::new(
        NSPoint::new(SIDEBAR_W, META_H + body_h / 2.0 - 9.0),
        NSSize::new(width - SIDEBAR_W - PAD, 18.0),
    ));
    placeholder.setAutoresizingMask(mask(&[
        A::ViewWidthSizable,
        A::ViewMinYMargin,
        A::ViewMaxYMargin,
    ]));
    placeholder.setHidden(true);
    content.addSubview(&placeholder);

    // The cover over the sidebar and the grid: a click reveals them. Its shade stands in for
    // a blur that cannot be installed.
    let cover_frame = NSRect::new(NSPoint::new(0.0, META_H), NSSize::new(width, body_h));
    let shade = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    shade.setFrame(cover_frame);
    shade.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    shade.setHidden(true);
    content.addSubview(&shade);
    let cover = NSButton::initWithFrame(NSButton::alloc(mtm), cover_frame);
    cover.setBordered(false);
    cover.setTransparent(true);
    cover.setTitle(&NSString::from_str(""));
    cover.setFocusRingType(NSFocusRingType::Default);
    cover.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("reveal_table"))));
    cover.setToolTip(Some(&NSString::from_str(crate::locale::t(
        "click_to_reveal",
    ))));
    cover.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    wire(&cover, sel!(tableWindowRevealClicked:));
    content.addSubview(&cover);

    VIEWS.with(|slot| {
        slot.replace(Some(Views {
            window,
            grid,
            frozen,
            header,
            header_clip,
            header_back,
            header_rule,
            well,
            sidebar,
            sidebar_title,
            meta,
            placeholder,
            hint,
            cover,
            shade,
        }))
    });
}

/// Well border and caption colours follow Reduce Transparency and Increase Contrast.
/// The frozen column and the header strip stay an opaque plate in the grid's own colour,
/// so scrolling cells do not show through and the plate does not become a second surface.
pub(super) fn apply_table_chrome(views: &Views) {
    crate::appearance::apply_well(&views.well, false, true);
    // A fixed card theme paints the grid: its background on the plates and the well, its
    // light or dark appearance on the window so separators and controls match.
    mac_ui::objc2_app_kit::NSAppearanceCustomization::setAppearance(
        &**views.window as &mac_ui::objc2_app_kit::NSWindow,
        crate::appearance::card_window_appearance().as_deref(),
    );
    let plate = crate::appearance::table_grid_colours().2;
    if crate::appearance::card_palette().is_some() {
        views.well.setFillColor(&plate);
    }
    views.header_back.setFillColor(&plate);
    views.frozen.setBackgroundColor(&plate);
    views.header.setBackgroundColor(&plate);
    let caption = crate::appearance::caption_color();
    views.hint.setTextColor(Some(&caption));
    views.sidebar_title.setTextColor(Some(&caption));
    views.placeholder.setTextColor(Some(&caption));
}
