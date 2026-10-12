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
        wire_button(chip.button(), sel!(chipClicked:));
        CHIP_POOL.with(|pool| {
            pool.borrow_mut().push(PooledChip {
                id: cmd.id.clone(),
                title: cmd.title.clone(),
                chip: Rc::clone(&chip),
            });
        });
        chip
    });
    // The title is the label. VoiceOver reads the detail as the hint.
    let help = (!cmd.detail.is_empty()).then(|| NSString::from_str(&cmd.detail));
    chip.button().setAccessibilityHelp(help.as_deref());
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
        // SAFETY: reading the view's parent on the main thread, only to compare it with `list`.
        let parent = unsafe { chip.view().superview() };
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

/// Show the selected chip as the prominent one. The chip index is the `SELECTION` index.
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
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel!(overflowClicked:)),
            &NSString::from_str(""),
        )
    };
    item.setTag(index as isize);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            unsafe {
                item.setTarget(Some(delegate));
            }
        }
    });
    item
}

pub(crate) fn activate_overflow(index: usize) {
    let cmd = OVERFLOW.with(|slot| slot.borrow().get(index).cloned());
    let Some(cmd) = cmd else {
        return;
    };
    run_command(cmd);
}

pub(crate) fn run_command(cmd: Command) {
    if cmd.id == CommandId::ImageMenu {
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
    if !commands::keeps_card_open(&cmd.id) && !formatting_link {
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
    let mut saw_appearance = false;
    let mut saw_keep_history = false;
    let mut saw_quit = false;
    let mut history_menu: Option<Retained<NSMenu>> = None;
    for (index, cmd) in items.iter().enumerate() {
        let in_history = matches!(
            cmd.id,
            CommandId::History(_)
                | CommandId::ClearHistory
                | CommandId::ClearPins
                | CommandId::DiffToggle(_)
        );
        if in_history {
            let submenu = history_menu.get_or_insert_with(|| {
                let parent = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(crate::locale::t("history")),
                        None,
                        &NSString::from_str(""),
                    )
                };
                let submenu = NSMenu::initWithTitle(
                    NSMenu::alloc(mtm),
                    &NSString::from_str(crate::locale::t("history")),
                );
                submenu.setAutoenablesItems(false);
                parent.setSubmenu(Some(&submenu));
                menu.addItem(&parent);
                submenu
            });
            let item = overflow_item(mtm, &cmd.title, index);
            item.setEnabled(cmd.enabled);
            submenu.addItem(&item);
            continue;
        }
        if matches!(cmd.id, CommandId::ClearSensitive) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        if !saw_keep_history && matches!(cmd.id, CommandId::KeepHistory(_)) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_keep_history = true;
        }
        if !saw_appearance && matches!(cmd.id, CommandId::Appearance(_)) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_appearance = true;
        }
        if matches!(cmd.id, CommandId::Settings) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        if !saw_quit && matches!(cmd.id, CommandId::Quit) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_quit = true;
        }
        let item = overflow_item(mtm, &cmd.title, index);
        item.setEnabled(cmd.enabled);
        if let CommandId::Appearance(item_theme) = cmd.id {
            let state = if item_theme == theme {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            };
            item.setState(state);
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
                unsafe {
                    item.setTarget(Some(delegate));
                }
            }
        });
        menu.addItem(&item);
    }
    let button = MORE.with(|slot| slot.borrow().as_ref().map(|more| more.view().retain()));
    let Some(button) = button else {
        return;
    };
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, 0.0), Some(&*button));
    focus_card();
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
    match Key::from_key_code(event.keyCode()) {
        // A button or the reveal cover has the focus: no text caret to keep.
        Some(key @ (Key::Left | Key::Right | Key::Up | Key::Down)) => {
            let command = event
                .modifierFlags()
                .contains(NSEventModifierFlags::Command);
            on_arrow(key, command, commands::ArrowFocus::Card);
        }
        Some(Key::Return) => activate_selected(),
        Some(Key::Escape) => {
            // Esc empties the in-item field. A second Esc closes the popup.
            if !item_query().is_empty() {
                clear_item_query();
            } else if SEARCHING.with(Cell::get) {
                close_search();
            } else {
                hide();
            }
        }
        _ => begin_search_from_key(event),
    }
}

pub(crate) fn begin_search_from_key(event: &NSEvent) {
    if SEARCHING.with(Cell::get) {
        return;
    }
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

pub(crate) fn focus_field() {
    let field = FIELD.with(|slot| slot.borrow().clone());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        window.makeFirstResponder(Some(&field));
    }
}
