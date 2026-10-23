//! Chips, keyboard handling, the overflow menu, search and wiping shown content.

use super::*;

/// One chip button kept in the chip row, and the command and title it was made for.
pub(crate) struct PooledChip {
    pub(crate) id: CommandId,
    pub(crate) title: String,
    pub(crate) chip: Rc<GlassButton>,
}

/// The chip for `cmd`: the pooled button with the same command and title, or a new one.
pub(crate) fn chip_for(mtm: MainThreadMarker, cmd: &Command) -> Rc<GlassButton> {
    let pooled = CHIP_POOL.with(|pool| {
        pool.borrow()
            .iter()
            .find(|pooled| pooled.id == cmd.id && pooled.title == cmd.title)
            .map(|pooled| Rc::clone(&pooled.chip))
    });
    let chip = pooled.unwrap_or_else(|| {
        let chip = Rc::new(GlassButton::pill(mtm, &cmd.title, ButtonSize::Regular));
        if let Some(symbol) = commands::symbol_for(&cmd.id) {
            chip.set_symbol(symbol);
        }
        wire_button(chip.button(), sel!(chipClicked:));
        // A chip can take the keyboard focus (with Full Keyboard Access), so arrow-key
        // selection shows the system focus ring on it.
        chip.button().setRefusesFirstResponder(false);
        CHIP_POOL.with(|pool| {
            pool.borrow_mut().push(PooledChip {
                id: cmd.id.clone(),
                title: cmd.title.clone(),
                chip: Rc::clone(&chip),
            });
        });
        chip
    });
    // The title is the label; a menu chip is a menu button named without its "▾". VoiceOver
    // reads the detail as the hint.
    if let Some(label) = commands::menu_chip_label(cmd) {
        chip.set_menu_button(label);
    }
    let help = (!cmd.detail.is_empty()).then(|| NSString::from_str(&cmd.detail));
    chip.button().setAccessibilityHelp(help.as_deref());
    // A disabled chip (Image ▾ on the recognized text) says why in its tooltip.
    chip.button().setEnabled(cmd.enabled);
    let tip = (!cmd.enabled && !cmd.detail.is_empty()).then(|| NSString::from_str(&cmd.detail));
    chip.button().setToolTip(tip.as_deref());
    chip
}

/// Drop pooled chips for commands the card no longer offers (`offered`), and zeroize their
/// titles: a history row's title is part of a copy.
pub(crate) fn prune_chip_pool(offered: &[&Command]) {
    CHIP_POOL.with(|pool| {
        pool.borrow_mut().retain_mut(|pooled| {
            let keep = offered
                .iter()
                .any(|cmd| cmd.id == pooled.id && cmd.title == pooled.title);
            if !keep {
                pooled.chip.view().removeFromSuperview();
                pooled.title.zeroize();
            }
            keep
        });
    });
}

/// Every pooled chip and the "no matches" note go; for Wipe.
pub(crate) fn drop_chip_pool() {
    prune_chip_pool(&[]);
    CHIPS.with(|slot| slot.borrow_mut().clear());
}

/// Show `chips` (in order, at `frames`) and hide the other pooled chips.
pub(crate) fn rebuild_pills(
    mtm: MainThreadMarker,
    list: &NSView,
    chips: Vec<Rc<GlassButton>>,
    frames: &[ChipFrame],
    show_empty: bool,
    text_width: f64,
) {
    NO_MATCHES.with(|slot| {
        let mut slot = slot.borrow_mut();
        if show_empty && slot.is_none() {
            let empty = widgets::label(
                mtm,
                mac_ui::metrics::FONT_BODY,
                &crate::appearance::caption_color(),
            );
            empty.setStringValue(&NSString::from_str(crate::locale::t(
                "no_matching_commands",
            )));
            list.addSubview(&empty);
            *slot = Some(empty);
        }
        if let Some(empty) = slot.as_ref() {
            empty.setFrame(NSRect::new(
                NSPoint::new(2.0, 4.0),
                NSSize::new(text_width, 20.0),
            ));
            empty.setHidden(!(show_empty && chips.is_empty()));
        }
    });
    let area_h = commands::chips_height(frames);
    let mut placed = Vec::with_capacity(chips.len());
    for (index, chip) in chips.into_iter().enumerate() {
        let Some(frame) = frames.get(index) else {
            continue;
        };
        let y = area_h - (frame.row as f64 + 1.0) * commands::CHIP_PITCH
            + (commands::CHIP_PITCH - commands::CHIP_PILL_H) / 2.0;
        chip.view().setFrame(NSRect::new(
            NSPoint::new(frame.x, y),
            NSSize::new(frame.width, commands::CHIP_PILL_H),
        ));
        chip.button().setTag(index as isize);
        let parent = mac_ui::widgets::superview(chip.view());
        if parent.is_none_or(|parent| !std::ptr::eq(&*parent, list)) {
            list.addSubview(chip.view());
        }
        chip.view().setHidden(false);
        placed.push(chip);
    }
    CHIP_POOL.with(|pool| {
        for pooled in pool.borrow().iter() {
            if !placed.iter().any(|chip| Rc::ptr_eq(chip, &pooled.chip)) {
                pooled.chip.view().setHidden(true);
                pooled.chip.button().setTag(-1);
            }
        }
    });
    CHIPS.with(|slot| *slot.borrow_mut() = placed);
}

/// Show the selected chip as the prominent one: the accent marks the chip Return runs. The
/// chip index is the `SELECTION` index.
pub(crate) fn paint_pills() {
    let selected = SELECTION.with(Cell::get);
    CHIPS.with(|slot| {
        for (index, chip) in slot.borrow().iter().enumerate() {
            chip.set_prominent(index == selected);
        }
    });
}

pub(crate) fn nudge(dx: isize, dy: isize) {
    let frames = FRAMES.with(|slot| slot.borrow().clone());
    let next = commands::step_chip(&frames, SELECTION.with(Cell::get), dx, dy);
    if next == SELECTION.with(Cell::get) {
        return;
    }
    SELECTION.set(next);
    paint_pills();
    focus_selected_chip();
}

/// Give the selected chip the keyboard focus, so the system draws its focus ring. AppKit only
/// lets a button take it with Full Keyboard Access on; otherwise the search field keeps it and
/// the accent alone shows the selection. Keys typed on a focused chip reach the window's
/// `keyDown:` ([`on_key`]): Return runs the selection, Space clicks the chip, text searches.
fn focus_selected_chip() {
    let chip = CHIPS.with(|slot| slot.borrow().get(SELECTION.with(Cell::get)).cloned());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(chip), Some(window)) = (chip, window) {
        let _ = window.makeFirstResponder(Some(chip.button()));
    }
}

/// The arrow key of a field editor's move command (`moveLeft:` …).
pub(crate) fn arrow_command(command: Sel) -> Option<Key> {
    if command == sel!(moveLeft:) {
        Some(Key::Left)
    } else if command == sel!(moveRight:) {
        Some(Key::Right)
    } else if command == sel!(moveUp:) {
        Some(Key::Up)
    } else if command == sel!(moveDown:) {
        Some(Key::Down)
    } else {
        None
    }
}

/// ⌘← or ⌘→ (Shift, Control and Option up: ⇧⌘← still selects to the line start in a field).
pub(crate) fn command_arrow(event: &NSEvent) -> Option<Key> {
    let key = Key::from_key_code(event.keyCode())?;
    let flags = event.modifierFlags();
    let command = flags.contains(NSEventModifierFlags::Command)
        && !flags.intersects(
            NSEventModifierFlags::Shift
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option,
        );
    (command && matches!(key, Key::Left | Key::Right)).then_some(key)
}

/// Do what an arrow key does on the card ([`commands::arrow_action`]). Returns `false` when
/// the key is left to the text field, so its caret moves.
/// The mouse is over the well (the PDF's page, when that is what it shows).
pub(crate) fn pointer_over_well() -> bool {
    let point = WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|window| window.mouseLocationOutsideOfEventStream())
    });
    point.is_some_and(|point| {
        point.x >= PAD
            && point.x <= card_width() - PAD
            && point.y >= WELL_BOTTOM.get()
            && point.y <= WELL_TOP.get()
    })
}

/// Turn the PDF's pages by `delta`: a beep at the first or last page. False when the card is no
/// PDF with pages to turn.
pub(crate) fn turn_pdf_pages(delta: isize) -> bool {
    let Some((page, count)) = PDF_PAGES.get() else {
        return false;
    };
    if crate::pdf::step_page(page, delta, count).is_some() {
        launcher::emit(UserEvent::Run(CommandId::PdfPage(delta)));
    } else {
        NSBeep();
    }
    true
}

/// ← → ⌥← ⌥→ and Page Up / Page Down on a PDF card ([`crate::pdf::route_key`]): true when the
/// key turned (or tried to turn) a page.
pub(crate) fn pdf_page_key(event: &NSEvent) -> bool {
    use crate::pdf::{KeyRoute, PageKey};
    if PDF_PAGES.get().is_none() || SEARCHING.with(Cell::get) {
        return false;
    }
    // Page Up / Page Down key codes.
    let key = match (event.keyCode(), Key::from_key_code(event.keyCode())) {
        (116, _) => PageKey::PageUp,
        (121, _) => PageKey::PageDown,
        (_, Some(Key::Left)) => PageKey::Left,
        (_, Some(Key::Right)) => PageKey::Right,
        _ => return false,
    };
    let flags = event.modifierFlags();
    let route = crate::pdf::route_key(
        key,
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Command),
        pointer_over_well(),
    );
    match route {
        KeyRoute::Turn(delta) => turn_pdf_pages(delta),
        KeyRoute::Card => false,
    }
}

/// A scroll wheel or trackpad swipe over the PDF's page turns it ([`crate::pdf::scroll_step`]).
/// True when the event was the PDF's.
pub(crate) fn pdf_scroll(event: &NSEvent) -> bool {
    if PDF_PAGES.get().is_none() || !pointer_over_well() {
        return false;
    }
    // Momentum after a swipe is not another swipe.
    if !event.momentumPhase().is_empty() {
        return true;
    }
    let (accumulated, step) = crate::pdf::scroll_step(
        PDF_SWIPE.get(),
        event.scrollingDeltaY(),
        event.hasPreciseScrollingDeltas(),
    );
    PDF_SWIPE.set(if event.phase().is_empty() || step.is_some() {
        0.0
    } else {
        accumulated
    });
    if let Some(delta) = step {
        turn_pdf_pages(delta);
    }
    true
}

pub(crate) fn on_arrow(key: Key, command: bool, focus: commands::ArrowFocus) -> bool {
    let nav = HISTORY_NAV.with(|slot| *slot.borrow());
    match commands::arrow_action(key, command, focus, nav.is_some()) {
        None | Some(commands::ArrowAction::Text) => false,
        Some(commands::ArrowAction::Chip { dx, dy }) => {
            nudge(dx, dy);
            true
        }
        Some(commands::ArrowAction::History(chevron)) => {
            // Same as clicking the chevron. At 1 (‹) or N (›), or without history: a beep.
            if nav.is_some_and(|nav| chevron.enabled(&nav)) {
                launcher::emit(UserEvent::Run(chevron.command()));
            } else {
                NSBeep();
            }
            true
        }
    }
}

pub(crate) fn activate_selected() {
    let cmd = SHOWN.with(|slot| slot.borrow().get(SELECTION.with(Cell::get)).cloned());
    let Some(cmd) = cmd else {
        return;
    };
    run_command(cmd);
}

pub(crate) fn overflow_item(
    mtm: MainThreadMarker,
    title: &str,
    index: usize,
) -> Retained<NSMenuItem> {
    let item = mac_ui::actions::menu_item_with_action(mtm, title, sel!(overflowClicked:));
    item.setTag(index as isize);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::set_menu_target(&item, delegate);
        }
    });
    item
}

/// The shortcut of `id` drawn on its row ([`commands::menu_key`]); the card runs the chord.
pub(crate) fn show_menu_key(item: &NSMenuItem, id: &CommandId) {
    if let Some((key, shift)) = commands::menu_key(id) {
        item.setKeyEquivalent(&NSString::from_str(key));
        let mut mask = NSEventModifierFlags::Command;
        if shift {
            mask |= NSEventModifierFlags::Shift;
        }
        item.setKeyEquivalentModifierMask(mask);
    }
}

pub(crate) fn activate_overflow(index: usize) {
    let cmd = OVERFLOW.with(|slot| slot.borrow().get(index).cloned());
    let Some(cmd) = cmd else {
        return;
    };
    run_command(cmd);
}

pub(crate) fn run_command(cmd: Command) {
    if cmd.id == CommandId::AiMenu {
        pop_ai_menu();
        return;
    }
    if cmd.id == CommandId::ImageMenu || cmd.id == CommandId::TextMenu {
        pop_table_menu();
        return;
    }
    // Table ▾ is no longer a chip; Choose columns stays for command search / legacy.
    if cmd.id == CommandId::TableMenu {
        pop_table_menu();
        return;
    }
    if cmd.id == CommandId::TableChooseColumns {
        open_picker();
        return;
    }
    let formatting_link =
        cmd.id == CommandId::Format && LINK_PAGE.with(|slot| slot.borrow().is_some());
    if !commands::keeps_card_open(&cmd.id)
        && !commands::closes_card_itself(&cmd.id)
        && !formatting_link
    {
        hide();
    }
    launcher::emit(UserEvent::Run(cmd.id));
}

pub(crate) fn pop_overflow() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let items = OVERFLOW.with(|slot| slot.borrow().clone());
    let theme = THEME.with(Cell::get);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let mut section: Option<commands::MenuSection> = None;
    // Each submenu with the item last put in it, for its separators and ⌥ alternates.
    let mut submenus: Vec<(&'static str, Retained<NSMenu>, Option<usize>)> = Vec::new();
    for (index, cmd) in items.iter().enumerate() {
        let this_section = commands::menu_section(&cmd.id);
        if section != Some(this_section) {
            if section.is_some() || this_section != commands::MenuSection::More {
                add_section_start(mtm, &menu, this_section, section.is_some());
            }
            section = Some(this_section);
        }
        let mut previous: Option<usize> = None;
        let target = match commands::menu_submenu(&cmd.id) {
            None => menu.clone(),
            Some(name) => match submenus.iter_mut().find(|(shown, _, _)| *shown == name) {
                Some((_, submenu, last)) => {
                    previous = last.replace(index);
                    submenu.clone()
                }
                None => {
                    let parent = mac_ui::widgets::menu_item(mtm, name);
                    if let Some(symbol) = commands::submenu_symbol(&cmd.id)
                        && let Some(image) = mac_ui::icon::system_symbol(symbol, name)
                    {
                        parent.setImage(Some(&image));
                    }
                    let submenu =
                        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(name));
                    submenu.setAutoenablesItems(false);
                    parent.setSubmenu(Some(&submenu));
                    menu.addItem(&parent);
                    submenus.push((name, submenu.clone(), Some(index)));
                    submenu
                }
            },
        };
        let previous_cmd = previous.and_then(|at| items.get(at));
        let previous_id = previous_cmd.map(|cmd| &cmd.id);
        if commands::separator_before(previous_id, &cmd.id) {
            target.addItem(&NSMenuItem::separatorItem(mtm));
        }
        let alternate = commands::is_alternate(previous_id, &cmd.id);
        let title = match cmd.id {
            CommandId::KeepHistory(minutes) => commands::keep_history_short(minutes),
            _ if alternate => commands::alternate_title(
                &cmd.title,
                previous_cmd.map(|cmd| cmd.title.as_str()).unwrap_or(""),
            ),
            _ => cmd.title.clone(),
        };
        let item = overflow_item(mtm, &title, index);
        show_menu_key(&item, &cmd.id);
        if alternate {
            // Held ⌥ swaps the row above for this one; both have no key, so the masks differ.
            if let Some(primary) = target.itemArray().lastObject() {
                primary.setKeyEquivalentModifierMask(NSEventModifierFlags::empty());
            }
            item.setKeyEquivalentModifierMask(NSEventModifierFlags::Option);
            item.setAlternate(true);
        }
        item.setEnabled(cmd.enabled);
        item.setAccessibilityLabel(Some(&NSString::from_str(&cmd.title)));
        if commands::menu_submenu(&cmd.id).is_none()
            && let Some(image) = commands::symbol_for(&cmd.id)
                .and_then(|symbol| mac_ui::icon::system_symbol(symbol, &cmd.title))
        {
            item.setImage(Some(&image));
        }
        if let CommandId::CardTheme(item_theme) = cmd.id
            && item_theme == crate::settings::card_theme()
        {
            item.setState(NSControlStateValueOn);
        }
        if let CommandId::Appearance(item_theme) = cmd.id {
            let state = if item_theme == theme {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            };
            item.setState(state);
        }
        if let CommandId::WindowSize(size) = cmd.id {
            item.setState(if WINDOW_SIZE.with(Cell::get) == size {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if let CommandId::KeepHistory(minutes) = cmd.id {
            item.setState(if HISTORY_MINUTES.with(Cell::get) == minutes {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if cmd.id == CommandId::ClearSensitive {
            item.setState(if CLEAR_SENSITIVE.with(Cell::get) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if cmd.id == CommandId::Pause {
            item.setState(if PAUSED.with(Cell::get) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if cmd.id == CommandId::IgnoreNext {
            item.setState(if IGNORE_NEXT.with(Cell::get) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::set_menu_target(&item, delegate);
            }
        });
        target.addItem(&item);
    }
    let button = MORE.with(|slot| slot.borrow().as_ref().map(|more| more.view().retain()));
    let Some(button) = button else {
        return;
    };
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, 0.0), Some(&*button));
    focus_card();
}

/// A section header (macOS 14+) or, before that, a separator before `section`. `after_other`:
/// another section came before, so older macOS needs the separator.
fn add_section_start(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    section: commands::MenuSection,
    after_other: bool,
) {
    add_section_header(mtm, menu, section.title(), after_other);
}

/// A section header titled `title` (macOS 14+) or, before that or without a title, a
/// separator when `after_other`.
pub(crate) fn add_section_header(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    title: Option<&str>,
    after_other: bool,
) {
    use mac_ui::objc2::ClassType;
    let headers = NSMenuItem::class()
        .metaclass()
        .responds_to(sel!(sectionHeaderWithTitle:));
    match title {
        Some(title) if headers => {
            if after_other {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            menu.addItem(&NSMenuItem::sectionHeaderWithTitle(
                &NSString::from_str(title),
                mtm,
            ));
        }
        _ if after_other => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        _ => {}
    }
}

pub(crate) fn on_key(event: &NSEvent) {
    // While the column picker is open, the card's keys (search, history) wait.
    if picker_open() {
        let _ = picker_key(event);
        return;
    }
    if is_item_find_chord(event) {
        if item_find_on() {
            focus_item_field();
        }
        return;
    }
    if pager_key(event) || pdf_page_key(event) {
        return;
    }
    match Key::from_key_code(event.keyCode()) {
        // A button or the reveal cover has the focus: no text caret to keep.
        Some(key @ (Key::Left | Key::Right | Key::Up | Key::Down)) => {
            let command = event
                .modifierFlags()
                .contains(NSEventModifierFlags::Command);
            on_arrow(key, command, commands::ArrowFocus::Card);
        }
        Some(Key::Return) => activate_selected(),
        // ⌫ takes the copy the strip marks out of history; the clipboard keeps it.
        Some(Key::Delete) => {
            let (strip_shown, cursor) = strip_delete_target();
            if let Some(id) = commands::delete_key(SEARCHING.with(Cell::get), strip_shown, cursor) {
                launcher::emit(UserEvent::Run(id));
            } else {
                begin_search_from_key(event);
            }
        }
        Some(Key::Escape) => {
            // Esc empties the in-item field, then ends a comparison of two strip cards. Then
            // it closes the popup.
            match commands::escape_step(
                item_query().is_empty(),
                SEARCHING.with(Cell::get),
                strip_comparing(),
            ) {
                commands::EscapeStep::ClearQuery => clear_item_query(),
                commands::EscapeStep::CloseSearch => close_search(),
                commands::EscapeStep::EndCompare => {
                    launcher::emit(UserEvent::Run(CommandId::EndCompare));
                }
                commands::EscapeStep::Close => hide(),
            }
        }
        _ => begin_search_from_key(event),
    }
}

pub(crate) fn begin_search_from_key(event: &NSEvent) {
    let flags = event.modifierFlags();
    if flags.contains(NSEventModifierFlags::Command)
        || flags.contains(NSEventModifierFlags::Control)
    {
        return;
    }
    let Some(text) = event.characters() else {
        return;
    };
    let text = text.to_string();
    if SEARCHING.with(Cell::get) {
        // A chip had the focus during a search: the typed text goes on in the search field.
        if is_search_text(&text) {
            let mut query = current_query();
            query.push_str(&text);
            set_query(&query);
            SELECTION.set(0);
            layout(false);
            focus_field();
        }
        return;
    }
    if text == "/" {
        open_search("");
    } else if is_search_text(&text) {
        open_search(&text);
    }
}

pub(crate) fn is_search_text(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| !ch.is_control())
}

pub(crate) fn open_search(initial: &str) {
    SEARCHING.set(true);
    set_query(initial);
    SELECTION.set(0);
    layout(false);
    focus_field();
}

pub(crate) fn close_search() {
    if !SEARCHING.with(Cell::get) {
        hide();
        return;
    }
    SEARCHING.set(false);
    set_query("");
    SELECTION.set(0);
    layout(false);
    focus_card();
}

pub fn wipe_shown() {
    // Wipe forgets which items were shown or hidden with the eye button too.
    BLUR_CHOICES.with(|choices| choices.borrow_mut().clear());
    dismiss_picker(false);
    set_item_find(false);
    PREVIEW_NOTE.with(|slot| slot.borrow_mut().clear());
    CARD_TITLE.with(wipe_string_slot);
    CARD_META.with(wipe_string_slot);
    CARD_EXCERPT.with(wipe_string_slot);
    CARD_PLACEHOLDER.with(wipe_string_slot);
    LINK_PAGE.with(|slot| set_secret_opt(slot, None));
    for slot in [&ACTIONS, &POOL, &OVERFLOW, &SHOWN] {
        slot.with(|slot| set_commands(slot, Vec::new()));
    }
    clear_painted();
    clear_strip();
    DIFF_LOCK.set(false);
    CARD_DIFF.set(false);
    DIFF_WIDE.set(false);
    wipe_shown_views();
    SEARCHING.set(false);
}

pub(crate) fn wipe_string_slot(slot: &RefCell<String>) {
    slot.borrow_mut().zeroize();
}

/// Drop the remembered well paint and zero the text it held.
pub(crate) fn clear_painted() {
    PAINTED.with(|slot| {
        if let Some(painted) = slot.borrow_mut().as_mut() {
            painted.text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
    PAINTED_ITEM.set(None);
}

pub(crate) fn set_secret(slot: &RefCell<String>, next: String) {
    wipe_string_slot(slot);
    *slot.borrow_mut() = next;
}

pub(crate) fn set_secret_opt(slot: &RefCell<Option<String>>, next: Option<String>) {
    if let Some(text) = slot.borrow_mut().as_mut() {
        text.zeroize();
    }
    *slot.borrow_mut() = next;
}

pub(crate) fn set_commands(slot: &RefCell<Vec<Command>>, next: Vec<Command>) {
    commands::wipe_commands(&mut slot.borrow_mut());
    slot.replace(next);
}

pub(crate) fn wipe_shown_views() {
    FIND_CACHE.with(|slot| slot.borrow_mut().take());
    drop_chip_pool();
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            widgets::wipe_text_view(view);
        }
    });
    set_item_text(String::new());
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setImage(None);
        }
    });
    zoom::wipe();
    THUMB_TOKEN.set(-1);
    PICTURE.with(|slot| slot.replace(None));
    PICTURE_SHOWN.set(0);
    for slot in [&HEADER, &META, &FIELD, &ITEM_COUNT] {
        slot.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                widgets::wipe_text_field(field);
            }
        });
    }
    wipe_item_field();
}

pub(crate) fn current_query() -> zeroize::Zeroizing<String> {
    FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| zeroize::Zeroizing::new(field.stringValue().to_string()))
            .unwrap_or_default()
    })
}

pub(crate) fn set_query(text: &str) {
    FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(text));
        }
    });
}

pub(crate) fn focus_card() {
    if picker_open() {
        focus_picker_field();
    } else {
        focus_field();
    }
}

/// ⌘V belongs to the field with the caret (search, find, column filter), not the card
/// ([`commands::paste_target`]). The hidden Edit menu's *Paste* then pastes there
/// ([`crate::macos_main_menu`]): at the caret, replacing the selection, one undoable edit.
pub(crate) fn caret_in_field(window: &NSWindow) -> bool {
    let Some(editor) = window
        .firstResponder()
        .and_then(|responder| responder.downcast::<NSTextView>().ok())
    else {
        return false;
    };
    let hidden_search = !SEARCHING.with(Cell::get)
        && FIELD.with(|slot| {
            slot.borrow().as_ref().is_some_and(|field| {
                field.currentEditor().is_some_and(|current| {
                    std::ptr::eq(
                        Retained::as_ptr(&current).cast::<AnyObject>(),
                        Retained::as_ptr(&editor).cast::<AnyObject>(),
                    )
                })
            })
        });
    commands::paste_target(editor.isEditable(), hidden_search) == commands::PasteTarget::Field
}

pub(crate) fn focus_field() {
    let field = FIELD.with(|slot| slot.borrow().clone());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        window.makeFirstResponder(Some(&field));
    }
}
