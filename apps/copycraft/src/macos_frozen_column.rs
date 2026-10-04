// The frozen first column of the card's table view: a copy of the grid's first column (its
// characters and colours, taken from the well's own text) in a text view that floats over the
// well's left edge. It scrolls up and down with the grid but not sideways, so a row's first
// value stays in view while the other columns scroll past. Part of `macos_launcher` (included
// there): it follows the well's blur, accessibility and wipe.

thread_local! {
    /// Made with the first grid; kept as a floating subview of the well's scroll view.
    static FROZEN: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
}

/// The frozen column's text view, made and floated over the well the first time.
fn frozen_view(mtm: MainThreadMarker) -> Option<Retained<NSTextView>> {
    if let Some(view) = FROZEN.with(|slot| slot.borrow().clone()) {
        return Some(view);
    }
    let scroll = PREVIEW_SCROLL.with(|slot| slot.borrow().clone())?;
    let view = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    // Opaque, in the well's colour, so the columns scrolling under it do not show through.
    view.setDrawsBackground(true);
    view.setBackgroundColor(&NSColor::controlBackgroundColor());
    // A copy of what the well says: VoiceOver reads the well itself.
    view.setAccessibilityElement(false);
    view.setHidden(true);
    widgets::scroll_text_both_ways(&view);
    scroll.addFloatingSubview_forAxis(&view, mac_ui::objc2_app_kit::NSEventGestureAxis::Horizontal);
    // The column is as tall as the whole grid. Floating subviews sit beside the clip view, not
    // in it, and since macOS 14 a view does not clip its subviews by default: without this the
    // column draws past the well, over the meta line and the chips.
    scroll.setClipsToBounds(true);
    FROZEN.with(|slot| slot.replace(Some(view.clone())));
    Some(view)
}

/// After the well was painted with `body`: show the grid's first column frozen when `body` is
/// a table grid of two or more columns, else hide (and wipe) it.
fn refresh_frozen_column(body: &str, highlight: Option<FormatKind>, payload: bool) {
    let lines = (payload && highlight == Some(FormatKind::Dataframe))
        .then(|| crate::dataframe::frozen_column(body))
        .flatten();
    let Some(lines) = lines else {
        hide_frozen_column();
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(text) = PREVIEW_TEXT.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let Some(frozen) = frozen_view(mtm) else {
        return;
    };
    crate::macos_card_text::copy_frozen_column(&text, &frozen, &lines);
    frozen.setHidden(false);
    // The well's blur covers the frozen column too.
    if BLUR_ON.get()
        && let Some((filter, _)) = gaussian_blurs()
    {
        mac_ui::blur::set_content_filters(&frozen, &NSArray::from_slice(&[&*filter]));
    }
}

/// No grid in the well (another view, the overview, a picture, Wipe): the frozen column goes,
/// its characters overwritten first.
fn hide_frozen_column() {
    FROZEN.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            widgets::wipe_text_view(view);
            view.setHidden(true);
        }
    });
}

/// The well's blur (`filters`) on the frozen column too; empty clears it.
fn blur_frozen_column(filters: &NSArray<AnyObject>) {
    FROZEN.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            mac_ui::blur::set_content_filters(view, filters);
        }
    });
}
