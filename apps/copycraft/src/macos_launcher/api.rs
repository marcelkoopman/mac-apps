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

/// The card's window number, when it is made.
pub(crate) fn window_number() -> Option<isize> {
    WINDOW.with(|slot| slot.borrow().as_ref().map(|window| window.windowNumber()))
}

/// Give the card (when made) the sharing type "Allow Screenshots" asks for.
pub(crate) fn apply_capture() {
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.setSharingType(sharing_type());
        }
    });
}

/// Spin a busy wheel centered over the well (on top of the preview), or take it away. It
/// stays while a file is still opening ([`set_opening`]).
pub fn set_busy(busy: bool) {
    BUSY.set(busy);
    show_spinner();
}

/// "Opening file…" under the busy wheel while a dropped or chosen file is on its way, or
/// take it away (the wheel stays while other work is busy, [`set_busy`]). VoiceOver reads the
/// wheel as "Opening file…" meanwhile.
pub fn set_opening(opening: bool) {
    OPENING.set(opening);
    show_spinner();
}

/// "Extracting…" under the busy wheel while a zip is extracted, or take it away.
pub fn set_unzipping(unzipping: bool) {
    UNZIPPING.set(unzipping);
    show_spinner();
}

/// The card says "Extracting…" (so ⌘. stops it).
pub(crate) fn unzipping() -> bool {
    UNZIPPING.get()
}

/// ⌘. while the card says "Extracting…" ([`unzipping`]): stop the extraction. True when the
/// key was that.
pub(crate) fn stop_unzip_key(event: &NSEvent) -> bool {
    if !unzipping() || !mac_ui::keys::is_command_chord(event, ".") {
        return false;
    }
    launcher::emit(UserEvent::Run(CommandId::UnzipStop));
    true
}

/// The line under the wheel: "Opening file…" before "Extracting…".
fn spinner_line() -> Option<&'static str> {
    if OPENING.get() {
        Some(crate::locale::t("opening_file"))
    } else if UNZIPPING.get() {
        Some(crate::locale::t("unzipping"))
    } else {
        None
    }
}

/// The wheel (and the line under it) as [`BUSY`], [`OPENING`] and [`UNZIPPING`] ask.
fn show_spinner() {
    let (busy, line_text) = (BUSY.get(), spinner_line());
    let opening = line_text.is_some();
    if !opening {
        OPENING_LINE.with(|slot| {
            if let Some(line) = slot.borrow().as_ref() {
                line.removeFromSuperview();
            }
        });
    }
    if !busy && !opening {
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
    let area = well.frame();
    let spinner_frame = SPINNER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let spinner = slot.get_or_insert_with(|| progress::Spinner::new(mtm, SpinnerSize::Regular));
        spinner.set_accessibility_label(line_text.unwrap_or_else(|| mac_ui::lang::t("loading")));
        spinner.add_centered(&parent, area);
        spinner.start();
        spinner.view().frame()
    });
    if !opening {
        return;
    }
    OPENING_LINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let line = slot.get_or_insert_with(|| {
            let line = mac_ui::widgets::label(
                mtm,
                mac_ui::metrics::FONT_CAPTION,
                &NSColor::secondaryLabelColor(),
            );
            line.setAlignment(NSTextAlignment::Center);
            // The wheel says it already.
            line.setAccessibilityElement(false);
            line
        });
        line.setStringValue(&NSString::from_str(line_text.unwrap_or_default()));
        line.sizeToFit();
        parent.addSubview(line);
        place_opening_line(line, &parent, area, spinner_frame);
    });
}

/// "Opening file…" centered under the wheel at `spinner`, over `area` (the well's frame in
/// `parent`), whichever way the card's content view counts.
fn place_opening_line(line: &NSTextField, parent: &NSView, area: NSRect, spinner: NSRect) {
    let size = line.fittingSize();
    let gap = 6.0;
    let width = size.width.min(area.size.width);
    let x = (area.origin.x + (area.size.width - width) / 2.0).round();
    let y = if parent.isFlipped() {
        spinner.origin.y + spinner.size.height + gap
    } else {
        spinner.origin.y - gap - size.height
    };
    line.setFrame(NSRect::new(
        NSPoint::new(x, y.round()),
        NSSize::new(width, size.height),
    ));
}

thread_local! {
    /// Background work asked for the wheel ([`set_busy`]).
    static BUSY: Cell<bool> = const { Cell::new(false) };
    /// A file is opening ([`set_opening`]).
    static OPENING: Cell<bool> = const { Cell::new(false) };
    /// A zip is being extracted ([`set_unzipping`]).
    static UNZIPPING: Cell<bool> = const { Cell::new(false) };
    /// "Opening file…" under the wheel, made once.
    static OPENING_LINE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
}

thread_local! {
    /// The Preview button's small spinner and the symbol it stands in for, while Preview opens
    /// the picture ([`crate::preview_wait`]).
    static PREVIEW_WAIT: RefCell<Option<(progress::Spinner, Option<Retained<NSImage>>)>> =
        const { RefCell::new(None) };
}

/// Whether the Preview button shows its spinner.
pub(crate) fn preview_busy() -> bool {
    PREVIEW_WAIT.with(|slot| slot.borrow().is_some())
}

/// The Preview button spins (dimmed, read as "Loading preview…") until Preview has the
/// picture, then is itself again.
pub fn set_preview_busy(busy: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    PREVIEW_BUTTON.with(|slot| {
        let slot = slot.borrow();
        let Some(button) = slot.as_ref() else {
            return;
        };
        PREVIEW_WAIT.with(|wait| {
            let mut wait = wait.borrow_mut();
            if busy {
                if wait.is_some() {
                    return;
                }
                let label = crate::locale::t("preview_loading");
                let image = button.button().image();
                button.button().setImage(None);
                button.button().setEnabled(false);
                button.set_accessibility_label(label);
                let spinner = progress::Spinner::new(mtm, SpinnerSize::Small);
                spinner.set_accessibility_label(label);
                spinner.add_centered(button.view(), button.view().bounds());
                spinner.start();
                *wait = Some((spinner, image));
            } else if let Some((spinner, image)) = wait.take() {
                spinner.remove();
                button.button().setImage(image.as_deref());
                button.button().setEnabled(true);
                button.set_accessibility_label(crate::locale::t("open_preview"));
            }
        });
    });
    place_content_actions(WELL_BOTTOM.get());
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
            if spinner_line().is_some()
                && let Some(parent) = mac_ui::widgets::superview(spinner.view())
            {
                OPENING_LINE.with(|line| {
                    if let Some(line) = line.borrow().as_ref() {
                        place_opening_line(line, &parent, well_frame, spinner.view().frame());
                        raise_view(line);
                    }
                });
            }
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
    CENTRED.set(false);
    reveal_with_card(data, card);
}

/// Open the card in the middle of the main screen with `card` built from `data`.
pub fn open_centred(data: LaunchData, card: commands::WorkCard) {
    ICON_FRAME.set(None);
    CENTRED.set(true);
    reveal_with_card(data, card);
}

/// Bring the card of an AI run or preview back, in the middle of the main screen.
pub fn restore_centred() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    ICON_FRAME.set(None);
    CENTRED.set(true);
    prepare(mtm, true);
    show_stored(true);
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

/// Bring the card back for an Apple Intelligence run or preview that is going on behind it:
/// the same card (nothing is stored again, so a newer copy does not take its place), under the
/// icon.
pub fn restore_under_icon(tray: &mac_ui::tray_icon::TrayIcon) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    ICON_FRAME.set(panel::tray_icon_frame(mtm, tray));
    CENTRED.set(false);
    prepare(mtm, true);
    show_stored(true);
}

fn prepare(mtm: MainThreadMarker, fresh: bool) {
    panel::activate_app(mtm);
    ensure_window(mtm);
    if fresh {
        set_query("");
        SEARCHING.set(false);
        set_action_scope(false);
        SELECTION.set(0);
        REVEALED.set(false);
    }
}

pub(crate) fn present_with_card(data: LaunchData, card: commands::WorkCard, fresh: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    prepare(mtm, fresh);
    store_with_card(data, card);
    show_stored(fresh);
}

/// Put the card on screen from what it already holds (nothing is stored again).
fn show_stored(fresh: bool) {
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
    raise_version_row();
    raise_content_actions();
}

#[cfg(test)]
pub(crate) fn store(data: LaunchData) {
    let card = commands::work_card(&data);
    store_with_card(data, card);
}

pub(crate) fn store_with_card(data: LaunchData, card: commands::WorkCard) {
    let building = commands::is_building_card(&card);
    // A slow text card (a view such as To JSON, a Text ▾ result, line numbers) keeps the text
    // card on screen until it is ready, instead of an empty "Working…" well.
    let keep_shown = commands::keeps_shown_card(
        building,
        !SHOWS_IMAGE.with(Cell::get)
            && LINK_PAGE.with(|slot| slot.borrow().is_none())
            && CARD_EXCERPT.with(|slot| !slot.borrow().is_empty()),
        data.subject_kind == commands::SubjectKind::Text
            && data.picture.is_none()
            && data.view != commands::CardView::Diff,
    );
    PLACEHOLDER.set(building);
    // Another entry is masked again. The same one stays revealed in every view, chip, version
    // and step, and when a rebuilt card or its labels arrive ([`commands::stays_revealed`]).
    let key = commands::content_key(&data);
    store_picker_source(&data, key);
    let shown = CONTENT_KEY.with(Cell::get);
    // While the same entry's card is rebuilt (line numbers switched), the dock keeps its
    // buttons where they are, so the eye button does not slide under the pointer.
    let offers_numbers = commands::dock_offers_numbers(
        commands::offers_line_numbers(&data, &card),
        building,
        shown == key || keep_shown,
        OFFERS_NUMBERS.get(),
    );
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
    // Kept: the shown text card's own text, labels and mask stay until the new card is in.
    if !keep_shown {
        PREVIEW_NOTE.with(|slot| slot.replace(card.preview_note.clone().unwrap_or_default()));
        MASKS.set(commands::masks_content(&card, data.view) || diff_lock);
        CARD_TITLE.with(|slot| set_secret(slot, card.title));
        CARD_META.with(|slot| set_secret(slot, card.meta));
        CARD_EXCERPT.with(|slot| set_secret(slot, card.excerpt));
        CARD_PLACEHOLDER.with(|slot| set_secret(slot, card.placeholder));
        CARD_HIGHLIGHT.with(|slot| slot.replace(card.highlight));
        CARD_ERROR_LINE.set(card.error_line);
        CARD_SWATCH.set(card.swatch);
        CARD_SELECTABLE.set(card.selectable);
        LINK_PAGE.with(|slot| set_secret_opt(slot, card.link_page));
        SHOWS_IMAGE.set(card.shows_image);
    }
    PICTURE.with(|slot| slot.replace(data.picture.clone()));
    publish_search_text();
    THEME.set(data.theme);
    CLEAR_SENSITIVE.set(data.settings.clear_sensitive);
    HISTORY_MINUTES.set(data.settings.history_minutes);
    WINDOW_SIZE.set(data.settings.window_size);
    PDF_PAGES.set(
        data.pdf
            .filter(|pdf| {
                pdf.turnable() > 1
                    && data.subject_kind == commands::SubjectKind::Image
                    && data.view == commands::CardView::Original
                    && data.picture.is_some()
            })
            .map(|pdf| (data.pdf_page, pdf.turnable())),
    );
    // The zoom of a PDF page: only on its picture (the Beeld view).
    let pdf_picture = data.pdf.is_some()
        && data.subject_kind == commands::SubjectKind::Image
        && data.view == commands::CardView::Original
        && data.picture.is_some();
    ZOOM_CARD.set(pdf_picture.then_some(data.pdf_zoom));
    ZOOM_SHOWN.set(if pdf_picture {
        data.pdf_zoom
    } else {
        crate::pdf::ZOOM_FIT
    });
    ZOOM_PICTURE.with(|slot| {
        slot.replace(
            data.pdf_zoom_picture
                .clone()
                .filter(|_| pdf_picture && data.pdf_zoom > crate::pdf::ZOOM_FIT),
        )
    });
    PAUSED.set(data.paused);
    IGNORE_NEXT.set(data.ignore_next);
    // At most four chips on the card; the others lead the `…` menu (Text ▾ stays visible).
    let (shown, more) = commands::split_chips(commands::card_chips(&data));
    ACTIONS.with(|slot| set_commands(slot, shown));
    AI_ACTIONS.set(commands::ai_actions(&data));
    AI_FNS.with(|slot| slot.replace(commands::ai_functions(&data)));
    let pool = commands::search_pool(&data);
    PANEL.with(|slot| set_commands(slot, commands::card_actions(&data, &pool)));
    POOL.with(|slot| set_commands(slot, pool));
    KIND_CHIPS.with(|slot| set_commands(slot, commands::kind_chips(&data)));
    OVERFLOW.with(|slot| {
        set_commands(
            slot,
            more.into_iter().chain(commands::overflow(&data)).collect(),
        )
    });
    HISTORY_NAV.with(|slot| slot.replace(data.history_nav));
    store_strip(&data.strip, data.strip_cursor, data.can_restore_history);
    store_strip_hint(data.strip_hint);
    CONTENT_ACTIONS.set(commands::content_actions(&data));
    OFFERS_NUMBERS.set(offers_numbers);
    VERSION_BAR.with(|slot| slot.replace(commands::VersionBar::of(&data)));
    BACK_TO_SOURCE.set(data.back_to_source);
    TEXT_STEPS.with(|slot| {
        slot.replace(
            data.text_edit
                .clone()
                .filter(|_| data.subject_kind == commands::SubjectKind::Text),
        )
    });
    IMAGE_STEPS.with(|slot| {
        slot.replace(
            data.image_edit
                .clone()
                .filter(|_| data.subject_kind == commands::SubjectKind::Image),
        )
    });
    // A picture's card has Image ▾ in the table menu's place.
    let text_menu = commands::text_shown(&data);
    let items = match (data.image_edit.as_ref(), text_menu.as_ref()) {
        (Some(_), _) if data.subject_kind == commands::SubjectKind::Image => {
            commands::image_menu_items()
        }
        // The same cards that show the Text ▾ chip (`offers_text_menu`), with a fresh menu
        // when no version has been attached yet.
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

/// Send the card to the background for an Apple Intelligence run: the run and its preview stay
/// (they are shown again with the card).
pub(crate) fn minimize() {
    hide_card_with(true, true);
}

fn hide_card(animated: bool) {
    // A run or preview of Apple Intelligence goes on behind a hidden card: Cancel, Esc and Apply
    // end it, hiding does not.
    hide_card_with(animated, ai_open());
}

fn hide_card_with(animated: bool, keep_ai: bool) {
    reset_well_dock();
    if !keep_ai {
        dismiss_picker(false);
    }
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
