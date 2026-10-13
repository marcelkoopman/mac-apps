//! The launcher's public entry points: opening, syncing and hiding the card, and window levels.

use super::*;

pub fn is_open() -> bool {
    OPEN.with(Cell::get)
}

/// One above the card. The card stays at [`NSFloatingWindowLevel`] (above other apps). A window
/// at the same level loses the next time the card calls `orderFront`.
pub(crate) fn level_above_card() -> NSWindowLevel {
    WINDOW
        .with(|slot| slot.borrow().as_ref().map(|window| window.level()))
        .unwrap_or(NSFloatingWindowLevel)
        .saturating_add(1)
}

/// Raise `window` above the floating card. The first call stores `window`'s level in `previous`.
pub(crate) fn raise_above_card(window: &NSWindow, previous: &Cell<Option<NSWindowLevel>>) {
    if previous.get().is_none() {
        previous.set(Some(window.level()));
    }
    let above = level_above_card();
    if window.level() < above {
        window.setLevel(above);
    }
}

/// Put back the level saved by [`raise_above_card`]. A second call does nothing.
pub(crate) fn restore_above_card(window: &NSWindow, previous: &Cell<Option<NSWindowLevel>>) {
    let Some(level) = previous.take() else {
        return;
    };
    window.setLevel(level);
}

/// The sharing type for the card and the table window: none, or read-only while "Allow
/// screenshots" is on ([`crate::launcher::capture`]).
pub(crate) fn sharing_type() -> NSWindowSharingType {
    match crate::launcher::capture() {
        crate::launcher::Capture::Excluded => NSWindowSharingType::None,
        crate::launcher::Capture::ReadOnly => NSWindowSharingType::ReadOnly,
    }
}

/// Give the card (when made) the sharing type "Allow screenshots" asks for.
pub(crate) fn apply_capture() {
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.setSharingType(sharing_type());
        }
    });
}

/// Spin a busy wheel centered over the well (on top of the preview), or take it away.
pub fn set_busy(busy: bool) {
    if !busy {
        SPINNER.with(|slot| {
            if let Some(spinner) = slot.borrow().as_ref() {
                spinner.remove();
            }
        });
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(well) = WELL.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let Some(parent) = mac_ui::widgets::superview(&well) else {
        return;
    };
    SPINNER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let spinner = slot.get_or_insert_with(|| progress::Spinner::new(mtm, SpinnerSize::Regular));
        spinner.add_centered(&parent, well.frame());
        spinner.start();
    });
}

thread_local! {
    /// Busy wheel shown while a modal open or save panel is starting. The panel blocks the
    /// main thread, so the wheel is committed before that call.
    static PANEL_WAIT: RefCell<Option<progress::Spinner>> = const { RefCell::new(None) };
}

/// The busy wheel on the key window, for the wait before an open or save panel appears.
pub struct PanelWait;

impl PanelWait {
    /// Show the wheel and commit its animation. Dropping it removes the wheel.
    pub fn show() -> Self {
        show_panel_wait();
        Self
    }
}

impl Drop for PanelWait {
    fn drop(&mut self) {
        hide_panel_wait();
    }
}

pub(crate) fn show_panel_wait() {
    if PANEL_WAIT.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = NSApplication::sharedApplication(mtm).keyWindow() else {
        return;
    };
    let Some(content) = window.contentView() else {
        return;
    };
    let spinner = progress::Spinner::new(mtm, SpinnerSize::Regular);
    let indicator = spinner.view();
    indicator.setWantsLayer(true);
    // SAFETY: `setUsesThreadedAnimation:` keeps an indeterminate indicator spinning while
    // the main thread is inside a modal panel and cannot service the run loop.
    unsafe { indicator.setUsesThreadedAnimation(true) };
    spinner.add_centered(&content, content.bounds());
    // The view has to be in the window before its layer exists. Draw once, then start,
    // then commit, so the wheel is already running when the panel blocks this thread.
    window.displayIfNeeded();
    spinner.start();
    window.displayIfNeeded();
    commit_animations();
    PANEL_WAIT.with(|slot| slot.replace(Some(spinner)));
}

/// Put the panel-wait wheel back above whatever layout just raised.
pub fn raise_panel_wait() {
    let window = PANEL_WAIT.with(|slot| {
        let borrowed = slot.borrow();
        let spinner = borrowed.as_ref()?;
        let parent = mac_ui::widgets::superview(spinner.view());
        let parent = parent?;
        parent.addSubview(spinner.view());
        spinner.view().window()
    });
    if let Some(window) = window.as_ref() {
        window.displayIfNeeded();
        commit_animations();
    }
}

pub(crate) fn commit_animations() {
    if let Some(transaction) = AnyClass::get(c"CATransaction") {
        // SAFETY: +[CATransaction flush] takes no arguments. It commits animations already
        // added to a layer.
        let _: () = unsafe { msg_send![transaction, flush] };
    }
}

pub(crate) fn hide_panel_wait() {
    PANEL_WAIT.with(|slot| {
        if let Some(spinner) = slot.borrow_mut().take() {
            spinner.remove();
        }
    });
}

/// Keep a running spinner centered over the well and above the views raised after it.
pub(crate) fn place_spinner(well_frame: NSRect) {
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref()
            && spinner.is_added()
        {
            spinner.center_in(well_frame);
            raise_view(spinner.view());
        }
    });
}

pub fn order_front() {
    if !is_open() {
        return;
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            panel::bring_to_front(window);
        }
    });
}

/// Open the card under the menu bar icon of `tray` with `card` built (off the main thread)
/// from `data`.
pub fn open_under_icon(
    data: LaunchData,
    card: commands::WorkCard,
    tray: &mac_ui::tray_icon::TrayIcon,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // Measured again on every show: the card is placed from the icon, never from an old frame.
    // Kept afterwards, so a card that grows or shrinks stays anchored under the icon too.
    ICON_FRAME.set(panel::tray_icon_frame(mtm, tray));
    reveal_with_card(data, card);
}

/// Open the card with `card` already built (off the main thread) from `data`.
pub fn reveal_with_card(data: LaunchData, card: commands::WorkCard) {
    let fresh = !is_open();
    present_with_card(data, card, fresh);
}

/// Update the open card with `card` already built from `data`: the "Show all" card (built off the main
/// thread), or a history entry's card built earlier.
pub fn sync_with_card(data: LaunchData, card: commands::WorkCard) {
    if !is_open() {
        return;
    }
    store_with_card(data, card);
    layout(false);
}

pub(crate) fn present_with_card(data: LaunchData, card: commands::WorkCard, fresh: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    panel::activate_app(mtm);
    ensure_window(mtm);
    if fresh {
        set_query("");
        SEARCHING.set(false);
        SELECTION.set(0);
    }
    if fresh {
        REVEALED.set(false);
    }
    store_with_card(data, card);
    OPEN.set(true);
    layout(fresh);
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            let animate_in = mac_ui::motion::window_needs_in(window);
            panel::bring_to_front(window);
            if animate_in {
                mac_ui::motion::window_in(window);
            }
        }
    });
    focus_card();
    // Layout runs before the window is on screen. Setting the picture only then
    // leaves the layer empty, and Original stays on that blank well.
    if SHOWS_IMAGE.with(Cell::get) {
        THUMB_TOKEN.set(-1);
        PICTURE_SHOWN.set(0);
        show_preview_image();
        load_thumbnail();
    }
    // The picture and the text scroller are brought forward after the arrows
    // are placed. Put the blur, then `<` `>` and the well icons, back on top.
    if well_is_masked() {
        show_reveal_cover(true);
    }
    raise_history_nav();
    raise_content_actions();
}

#[cfg(test)]
pub(crate) fn store(data: LaunchData) {
    let card = commands::work_card(&data);
    store_with_card(data, card);
}

pub(crate) fn store_with_card(data: LaunchData, card: commands::WorkCard) {
    PLACEHOLDER.set(commands::is_building_card(&card));
    // Another entry is masked again. The same one stays revealed in every view, chip, version
    // and step, and when a rebuilt card or its labels arrive ([`commands::stays_revealed`]).
    let key = commands::content_key(&data);
    store_picker_source(&data, key);
    let shown = CONTENT_KEY.with(Cell::get);
    REVEALED.set(commands::stays_revealed(
        REVEALED.with(Cell::get),
        shown,
        key,
    ));
    if shown != key {
        CONTENT_KEY.set(key);
        HIDDEN.set(false);
        set_item_find(false);
    }
    // An item the eye button was used on keeps that choice (reopened, browsed back to).
    let (hidden, revealed) = BLUR_CHOICES.with(|choices| {
        choices
            .borrow()
            .apply(key, HIDDEN.with(Cell::get), REVEALED.with(Cell::get))
    });
    HIDDEN.set(hidden);
    REVEALED.set(revealed);
    PREVIEW_NOTE.with(|slot| slot.replace(card.preview_note.clone().unwrap_or_default()));
    let diff_lock = data.view == commands::CardView::Diff
        && data.diff.as_ref().is_some_and(|diff| diff.sensitive);
    DIFF_LOCK.set(diff_lock);
    let columns = data.view == commands::CardView::Diff
        && matches!(
            data.diff.as_ref().map(|diff| &diff.phase),
            Some(crate::diff::Phase::Same(_) | crate::diff::Phase::Lines(_))
        );
    CARD_DIFF.set(columns);
    DIFF_WIDE.set(columns);
    MASKS.set(commands::masks_content(&card, data.view) || diff_lock);
    CARD_TITLE.with(|slot| set_secret(slot, card.title));
    CARD_META.with(|slot| set_secret(slot, card.meta));
    CARD_EXCERPT.with(|slot| set_secret(slot, card.excerpt));
    CARD_PLACEHOLDER.with(|slot| set_secret(slot, card.placeholder));
    CARD_HIGHLIGHT.with(|slot| slot.replace(card.highlight));
    CARD_ERROR_LINE.set(card.error_line);
    CARD_SELECTABLE.set(card.selectable);
    LINK_PAGE.with(|slot| set_secret_opt(slot, card.link_page));
    SHOWS_IMAGE.set(card.shows_image);
    PICTURE.with(|slot| slot.replace(data.picture.clone()));
    publish_search_text();
    THEME.set(data.theme);
    CLEAR_SENSITIVE.set(data.settings.clear_sensitive);
    HISTORY_MINUTES.set(data.settings.history_minutes);
    PAUSED.set(data.paused);
    IGNORE_NEXT.set(data.ignore_next);
    // At most four chips on the card; the others lead the `…` menu (Lines… stays visible).
    let (shown, more) = commands::split_chips(commands::chips(&data));
    ACTIONS.with(|slot| set_commands(slot, shown));
    POOL.with(|slot| set_commands(slot, commands::search_pool(&data)));
    OVERFLOW.with(|slot| {
        set_commands(
            slot,
            more.into_iter().chain(commands::overflow(&data)).collect(),
        )
    });
    HISTORY_NAV.with(|slot| slot.replace(data.history_nav));
    CONTENT_ACTIONS.set(commands::content_actions(&data));
    VERSION_BAR.with(|slot| slot.replace(commands::VersionBar::of(&data)));
    // A picture's card has Image ▾ in the table menu's place.
    let items = match (data.image_edit.as_ref(), data.text_edit.as_ref()) {
        (Some(edit), _) if data.subject_kind == commands::SubjectKind::Image => {
            commands::image_menu_items(edit)
        }
        // (`text_edit` is only there when the card offers Lines…, see `offers_text_menu`).
        (_, Some(text)) => commands::text_menu_items(text),
        _ => commands::table_menu_items(data.table.as_ref()),
    };
    let (menu, checks): (Vec<Command>, Vec<bool>) = items.into_iter().unzip();
    TABLE_MENU.with(|slot| set_commands(slot, menu));
    TABLE_MENU_CHECKS.with(|slot| slot.replace(checks));
}

pub(crate) fn hide() {
    hide_card(true);
}

/// Close the card without the fade, for a window that must come to the front right after
/// (the table window): a card still fading would be ordered out after it and take focus.
pub(crate) fn hide_at_once() {
    hide_card(false);
}

fn hide_card(animated: bool) {
    dismiss_picker(false);
    crate::macos_symbols::close();
    set_item_find(false);
    if !is_open() && !window_is_visible() {
        return;
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            if animated {
                let closing = window.clone();
                mac_ui::motion::window_out(window, move || closing.orderOut(None));
            } else {
                mac_ui::motion::cancel_window_out();
                window.orderOut(None);
            }
        }
    });
    OPEN.set(false);
}

pub(crate) fn window_is_visible() -> bool {
    WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|window| window.isVisible())
    })
}
