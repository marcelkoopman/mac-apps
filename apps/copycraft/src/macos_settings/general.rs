//! The General tab: the hotkey, language, date order, Open at Login and the Meldingen block.

use super::*;

/// The notification switches in the order of their checkboxes (row by row), with the title key.
/// The checkbox's tag is the index.
pub(super) const NOTIFY_SWITCHES: [crate::notifications::Switch; 4] = [
    crate::notifications::Switch::Ai,
    crate::notifications::Switch::Ocr,
    crate::notifications::Switch::Errors,
    crate::notifications::Switch::Secret,
];

/// The note under the notification switches: while macOS refuses notifications, that they
/// are off (the switches then do nothing); otherwise that a notification never holds text.
pub(super) fn notify_note(refused: bool) -> &'static str {
    if refused {
        crate::locale::t("notify_off")
    } else {
        crate::locale::t("notify_note")
    }
}

/// An open Settings window follows the stored notification permission: while refused the
/// switches are dimmed, the note says notifications are off and the heading row offers the
/// Notifications pane of System Settings.
pub fn refresh_notifications() {
    let refused = settings::notify_permission() == crate::notifications::Permission::Refused;
    NOTIFY_BOXES.with(|slot| {
        for button in slot.borrow().iter() {
            button.setEnabled(!refused);
        }
    });
    NOTIFY_NOTE.with(|slot| {
        if let Some(note) = slot.borrow().as_ref() {
            note.setStringValue(&NSString::from_str(notify_note(refused)));
        }
    });
    NOTIFY_OPEN.with(|slot| {
        if let Some(open) = slot.borrow().as_ref() {
            open.setHidden(!refused);
        }
    });
}

/// The checkbox title of a notification switch (literal keys, for the locale scan test).
pub(super) fn notify_title(switch: crate::notifications::Switch) -> &'static str {
    use crate::notifications::Switch;
    match switch {
        Switch::Ai => crate::locale::t("notify_switch_ai"),
        Switch::Ocr => crate::locale::t("notify_switch_ocr"),
        Switch::Errors => crate::locale::t("notify_switch_errors"),
        Switch::Secret => crate::locale::t("notify_switch_secret"),
    }
}

pub(super) fn set_hotkey_text(text: &str) {
    HOTKEY_LABEL.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(text));
        }
    });
}

pub(super) fn handle_record_key(event: &NSEvent) {
    let key_code = event.keyCode();
    if key_code == mac_ui::keys::code::ESCAPE {
        RECORDING.set(false);
        set_hotkey_text(&settings::hotkey_label());
        return;
    }
    let flags = event.modifierFlags();
    let mut mods = Modifiers::empty();
    if flags.contains(NSEventModifierFlags::Control) {
        mods |= Modifiers::CONTROL;
    }
    if flags.contains(NSEventModifierFlags::Option) {
        mods |= Modifiers::ALT;
    }
    if flags.contains(NSEventModifierFlags::Shift) {
        mods |= Modifiers::SHIFT;
    }
    if flags.contains(NSEventModifierFlags::Command) {
        mods |= Modifiers::META;
    }
    let Some(code) = hotkey::code_from_key_code(key_code) else {
        return;
    };
    let chord = HotKey::new(Some(mods), code);
    if !hotkey::is_safe(&chord) {
        set_hotkey_text(crate::locale::t("hotkey_rejected"));
        return;
    }
    if settings::set_hotkey(&chord) {
        RECORDING.set(false);
        set_hotkey_text(&settings::hotkey_label());
        launcher::emit(UserEvent::HotkeyChanged);
    }
}

/// Algemeen: hotkey, language, date order, Open at Login and the Meldingen block. Returns the
/// unused height.
pub(super) fn build_general(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = Tab::General.pane_height();
    let value_x = MARGIN + LABEL_W + mac_ui::metrics::BUTTON_GAP;

    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("hotkey"), y);
    let value = widgets::label(
        mtm,
        mac_ui::metrics::FONT_BODY,
        &crate::appearance::caption_color(),
    );
    value.setStringValue(&NSString::from_str(&settings::hotkey_label()));
    let change = GlassButton::pill(mtm, crate::locale::t("change_hotkey"), ButtonSize::Small);
    change.set_symbol("keyboard");
    change.set_accessibility_label(crate::locale::t("change_hotkey_a11y"));
    let change_w = change.width_within(140.0).max(BUTTON_H);
    let change_x = WIDTH - MARGIN - change_w;
    value.setFrame(NSRect::new(
        NSPoint::new(value_x, y),
        NSSize::new(
            (change_x - mac_ui::metrics::BUTTON_GAP - value_x).max(40.0),
            BUTTON_H,
        ),
    ));
    body.addSubview(&value);
    HOTKEY_LABEL.with(|slot| *slot.borrow_mut() = Some(value));
    change.view().setFrame(NSRect::new(
        NSPoint::new(change_x, y),
        NSSize::new(change_w, BUTTON_H),
    ));
    wire(change.button(), sel!(changeHotkey:));
    body.addSubview(change.view());
    place_explanation(mtm, body, crate::locale::t("hotkey_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("language"), y);
    let language = popup(mtm, y);
    language.removeAllItems();
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_system")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_nl")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_en")));
    language.selectItemAtIndex(language_index(settings::language()));
    wire(language.as_ref(), sel!(languageChanged:));
    body.addSubview(&language);
    place_explanation(mtm, body, crate::locale::t("language_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("date_order"), y);
    let date_order = popup(mtm, y);
    date_order.removeAllItems();
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_dmy")));
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_mdy")));
    date_order.selectItemAtIndex(isize::from(settings::load().date_month_first));
    wire(date_order.as_ref(), sel!(dateOrderChanged:));
    body.addSubview(&date_order);
    DATE_POPUP.with(|slot| *slot.borrow_mut() = Some(date_order));
    place_explanation(mtm, body, crate::locale::t("date_order_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let login = checkbox(mtm, crate::locale::t("open_at_login"), y);
    wire(&login, sel!(loginToggled:));
    body.addSubview(&login);
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login));
    gap(&mut top, NOTE_GAP);
    let note = place_note(
        mtm,
        body,
        crate::locale::t("login_note"),
        take(&mut top, NOTE_H),
    );
    LOGIN_NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    refresh_login_row();
    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_notifications(mtm, body, &mut top);
    top
}

/// The Meldingen block at the bottom of Algemeen: a heading, the four notification switches
/// and their note, then Announce confirmations, from `top` down. While macOS refuses
/// notifications the note says so, the switches are dimmed and the heading row has *Open
/// System Settings › Notifications…* ([`refresh_notifications`]).
pub(super) fn place_notifications(mtm: MainThreadMarker, body: &NSView, top: &mut f64) {
    let y = take(top, BUTTON_H);
    let open = GlassButton::pill(
        mtm,
        crate::locale::t("notify_open_settings"),
        ButtonSize::Small,
    );
    open.set_symbol("bell.badge");
    open.set_accessibility_label(crate::locale::t("notify_open_settings"));
    let open_w = open
        .width_within(WIDTH - MARGIN * 2.0 - LABEL_W)
        .max(BUTTON_H);
    open.view().setFrame(NSRect::new(
        NSPoint::new(WIDTH - MARGIN - open_w, y),
        NSSize::new(open_w, BUTTON_H),
    ));
    wire(open.button(), sel!(openNotificationSettings:));
    body.addSubview(open.view());
    NOTIFY_OPEN.with(|slot| *slot.borrow_mut() = Some(open.view().retain()));
    place_label(
        mtm,
        body,
        crate::locale::t("group_notifications"),
        y,
        WIDTH - MARGIN * 2.0 - open_w - mac_ui::metrics::BUTTON_GAP,
    );
    let switches = settings::notify_switches();
    let column = (WIDTH - MARGIN * 2.0) / 2.0;
    for (row_index, pair) in NOTIFY_SWITCHES.chunks(2).enumerate() {
        let y = take(top, BUTTON_H);
        for (column_index, switch) in pair.iter().enumerate() {
            let at = row_index * 2 + column_index;
            let box_ = checkbox(mtm, notify_title(*switch), y);
            box_.setFrame(NSRect::new(
                NSPoint::new(MARGIN + column * column_index as f64, y),
                NSSize::new(column, BUTTON_H),
            ));
            box_.setTag(at as isize);
            box_.setState(state_of(switches.is_on(*switch)));
            wire(&box_, sel!(notifyToggled:));
            body.addSubview(&box_);
            NOTIFY_BOXES.with(|slot| slot.borrow_mut().push(box_));
        }
        if row_index == 0 {
            gap(top, mac_ui::metrics::SPACE_S);
        }
    }
    gap(top, NOTE_GAP);
    let note = place_note(
        mtm,
        body,
        crate::locale::t("notify_note"),
        take(top, NOTE_H),
    );
    NOTIFY_NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    refresh_notifications();
    // VoiceOver confirmations belong with the other ways CopyCraft tells you something.
    gap(top, mac_ui::metrics::SPACE_S);
    let y = take(top, BUTTON_H);
    let announce = checkbox(mtm, crate::locale::t("announce_confirmations"), y);
    announce.setState(state_of(settings::announce_confirmations()));
    wire(&announce, sel!(announceToggled:));
    body.addSubview(&announce);
    place_explanation(
        mtm,
        body,
        crate::locale::t("announce_confirmations_note"),
        top,
    );
}

/// Ask macOS (it never prompts) whether notifications are allowed; the answer is stored and
/// [`refresh_notifications`] shows it. A refusal undone in System Settings is picked up here.
pub(super) fn check_notifications() {
    crate::macos_notify::probe_permission();
}

pub(super) fn language_index(language: settings::Language) -> isize {
    match language {
        settings::Language::System => 0,
        settings::Language::Nl => 1,
        settings::Language::En => 2,
    }
}

pub(super) fn refresh_login_row() {
    let status = macos_login::status();
    LOGIN_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(if status.is_on() {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            button.setEnabled(status != macos_login::LoginStatus::Unavailable);
        }
    });
    let note = match status {
        macos_login::LoginStatus::RequiresApproval => crate::locale::t("login_approval"),
        macos_login::LoginStatus::Unavailable => crate::locale::t("login_unavailable"),
        macos_login::LoginStatus::NotFound => crate::locale::t("login_not_found"),
        _ => crate::locale::t("login_note"),
    };
    LOGIN_NOTE.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(note));
        }
    });
}
