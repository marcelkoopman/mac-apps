//! The table window's column picker, its keys and commands.

use super::*;

/// Modal Choose Columns… for the table window: All / None, checkboxes, Apply / Cancel.
/// Uses the same [`ColumnPicker`] model as the card; Apply emits one select step. The table
/// stays revealed (the alert steals key; we do not re-blur on Apply or Cancel).
pub(super) fn run_column_picker() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let columns = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| view.columns.clone())
            .unwrap_or_default()
    });
    if columns.len() < 2 {
        NSBeep();
        return;
    }
    let mut picker = ColumnPicker::new(columns);
    let row_h = 22.0;
    // Dialog buttons at the regular size (28 pt), like the alert's own.
    let btn_h = mac_ui::metrics::BUTTON_H;
    let width = 320.0;
    let pad = 4.0;
    let visible_rows = 12.0_f64.min(picker.columns().len() as f64).max(3.0);
    let scroll_h = visible_rows * row_h + 8.0;
    let list_h = picker.columns().len() as f64 * row_h;
    let accessory_h = btn_h + 8.0 + scroll_h;
    let accessory = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, accessory_h)),
    );

    let all = GlassButton::pill(mtm, crate::locale::t("picker_all"), ButtonSize::Regular);
    all.set_accessibility_label(crate::locale::t("picker_keep_all"));
    wire(all.button(), sel!(tableWindowPickerAll:));
    let none = GlassButton::pill(mtm, crate::locale::t("picker_none"), ButtonSize::Regular);
    none.set_accessibility_label(crate::locale::t("picker_keep_none"));
    wire(none.button(), sel!(tableWindowPickerNone:));
    let mut x = pad;
    for item in [&all, &none] {
        // Room for "Selecteer alles" / "Selecteer niets" at the regular size.
        let w = item.width_within(150.0);
        item.view().setFrame(NSRect::new(
            NSPoint::new(x, accessory_h - pad - btn_h),
            NSSize::new(w, btn_h),
        ));
        accessory.addSubview(item.view());
        x += w + 6.0;
    }

    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, scroll_h)),
    );
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setBorderType(NSBorderType::NoBorder);
    let list = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width - 16.0, list_h)),
    );
    let mut checks: Vec<Retained<NSButton>> = Vec::new();
    for (index, column) in picker.columns().iter().enumerate() {
        let y = list_h - (index as f64 + 1.0) * row_h;
        let check = mac_ui::actions::checkbox(mtm, &column.shown);
        check.setState(NSControlStateValueOn);
        check.setToolTip(Some(&NSString::from_str(&column.name)));
        check.setFrame(NSRect::new(
            NSPoint::new(4.0, y),
            NSSize::new(width - 24.0, row_h),
        ));
        list.addSubview(&check);
        checks.push(check);
    }
    scroll.setDocumentView(Some(&list));
    accessory.addSubview(&scroll);
    PICKER_CHECKS.with(|slot| slot.replace(checks.clone()));

    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(commands::choose_columns_title()));
    alert.setInformativeText(&NSString::from_str(crate::locale::t("picker_alert_info")));
    alert.addButtonWithTitle(&NSString::from_str(crate::locale::t("picker_apply")));
    alert.addButtonWithTitle(&NSString::from_str(crate::locale::t("cancel")));
    // AppKit binds Escape only to the English title "Cancel". "Annuleer" needs the key set.
    for button in alert.buttons().iter() {
        if mac_ui::lang::is_cancel(&button.title().to_string()) {
            button.setKeyEquivalent(&NSString::from_str("\u{1b}"));
        }
    }
    alert.setAccessoryView(Some(&accessory));

    COLUMN_PICKER_OPEN.set(true);
    // Stay revealed under the alert (mask_again is suppressed while open).
    REVEALED.set(true);
    panel::activate_app(mtm);
    alert.layout();
    let window = alert.window();
    window.setHidesOnDeactivate(false);
    window.setLevel(NSModalPanelWindowLevel);
    window.center();
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
    let response = alert.runModal();
    COLUMN_PICKER_OPEN.set(false);
    PICKER_CHECKS.with(|slot| slot.borrow_mut().clear());

    let applied = response == NSAlertFirstButtonReturn;
    if applied {
        for (index, check) in checks.iter().enumerate() {
            picker.set_kept(index, check.state() == NSControlStateValueOn);
        }
        if !picker.can_apply() {
            NSBeep();
        } else if let Some(step) = picker.step() {
            run(CommandId::TableStep(step));
        }
        // Nothing kept or nothing changed: still keep the grid revealed.
    }
    // Alert stole key; always leave the table window revealed after Choose Columns….
    reveal();
}

pub(super) fn set_picker_checks(kept: bool) {
    let state = if kept {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    };
    PICKER_CHECKS.with(|slot| {
        for check in slot.borrow().iter() {
            check.setState(state);
        }
    });
}

pub(super) fn run(id: CommandId) {
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::Run(id)));
}

/// ⌘O opens a file, ⌘S saves this table, ⌘Z / ⇧⌘Z step through the versions. ⌘W and ⌘M are
/// the hidden Window menu's (src/macos_main_menu.rs).
pub(super) fn window_key(event: &NSEvent) -> bool {
    if mac_ui::keys::is_command_chord(event, "o") {
        launcher::emit(UserEvent::Run(CommandId::ChooseFile));
        return true;
    }
    if mac_ui::keys::is_command_chord(event, "s") {
        run(CommandId::Save);
        return true;
    }
    let flags = event.modifierFlags();
    let Some(chars) = event.charactersIgnoringModifiers() else {
        return false;
    };
    let chars = chars.to_string();
    let id = SHOWN.with(|slot| {
        let borrowed = slot.borrow();
        commands::undo_key(
            &chars,
            flags.contains(NSEventModifierFlags::Command),
            flags.contains(NSEventModifierFlags::Shift),
            flags.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option),
            borrowed.as_ref().map(|view| &view.versions),
        )
    });
    match id {
        Some(id) => {
            run(id);
            true
        }
        None => false,
    }
}
