//! The Privacy tab: clipboard history and the settings under it, the privacy filter, blur
//! and the ignored apps.

use super::*;

/// Privacy: Keep clipboard history, the privacy filter, blur and the ignored apps (greyed out
/// while history is off). Returns the unused height.
pub(super) fn build_privacy(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = Tab::Privacy.pane_height();
    let y = take(&mut top, BUTTON_H);
    let history = checkbox(mtm, crate::locale::t("clipboard_history"), y);
    history.setState(state_of(settings::clipboard_history()));
    wire(&history, sel!(clipboardHistoryToggled:));
    body.addSubview(&history);
    HISTORY_BOX.with(|slot| *slot.borrow_mut() = Some(history));
    // Two lines: what history reads and keeps, and that macOS asks once.
    gap(&mut top, NOTE_GAP);
    let note = place_note(
        mtm,
        body,
        crate::locale::t("clipboard_history_note"),
        take(&mut top, NOTE_H * 2.0),
    );
    note.setUsesSingleLineMode(false);
    note.setLineBreakMode(mac_ui::objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    note.setMaximumNumberOfLines(2);
    note.setFrameSize(NSSize::new(WIDTH - MARGIN * 2.0, NOTE_H * 2.0));
    place_history_settings(mtm, body, &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let privacy = checkbox(mtm, crate::locale::t("privacy_filter"), y);
    privacy.setState(state_of(settings::privacy_filter()));
    wire(&privacy, sel!(privacyFilterToggled:));
    body.addSubview(&privacy);
    PRIVACY_BOX.with(|slot| *slot.borrow_mut() = Some(privacy));
    place_explanation(mtm, body, crate::locale::t("privacy_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    // Indented under the privacy filter: the blur only styles its mask.
    let blur = indented_checkbox(mtm, crate::locale::t("blur_masked"), y);
    blur.setState(state_of(settings::load().blur));
    // The blur only styles the privacy filter's mask.
    blur.setEnabled(settings::privacy_filter());
    wire(&blur, sel!(blurToggled:));
    body.addSubview(&blur);
    BLUR_BOX.with(|slot| *slot.borrow_mut() = Some(blur));
    gap(&mut top, NOTE_GAP);
    let note = place_note(
        mtm,
        body,
        crate::locale::t("blur_note"),
        take(&mut top, NOTE_H),
    );
    note.setFrame(NSRect::new(
        NSPoint::new(MARGIN + INDENT, note.frame().origin.y),
        NSSize::new(WIDTH - MARGIN * 2.0 - INDENT, NOTE_H),
    ));

    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_label(
        mtm,
        body,
        crate::locale::t("ignored_apps"),
        take(&mut top, BUTTON_H),
        WIDTH - MARGIN * 2.0,
    );
    place_explanation(mtm, body, crate::locale::t("ignored_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let ignored = text_field(mtm, take(&mut top, BUTTON_H));
    ignored.setStringValue(&NSString::from_str(&settings::ignored_apps_text()));
    ignored.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("ignored_apps"))));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: SettingsDelegate implements controlTextDidEndEditing:. DELEGATE keeps it
            // alive while the window is open; the field holds it weakly.
            unsafe { widgets::set_text_delegate(&ignored, delegate) };
        }
    });
    body.addSubview(&ignored);
    IGNORED_FIELD.with(|slot| *slot.borrow_mut() = Some(ignored));
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let add = GlassButton::pill(mtm, crate::locale::t("ignored_apps_add"), ButtonSize::Small);
    add.set_symbol("plus");
    add.set_accessibility_label(crate::locale::t("ignored_apps_add"));
    place_pill(&add, take(&mut top, BUTTON_H), 280.0);
    wire(add.button(), sel!(addFrontmostApp:));
    let add_button = add.button().retain();
    body.addSubview(add.view());
    IGNORED_ADD.with(|slot| *slot.borrow_mut() = Some(add_button));
    refresh_history_controls();
    top
}

/// The settings that only matter while history is kept, indented under it and greyed out
/// while it is off: retention, sensitive copies after 60 s and ask before clearing. Their notes
/// are tooltips, so the pane stays short.
pub(super) fn place_history_settings(mtm: MainThreadMarker, body: &NSView, top: &mut f64) {
    gap(top, mac_ui::metrics::SPACE_S);
    let y = take(top, BUTTON_H);
    let label = widgets::label(mtm, mac_ui::metrics::FONT_BODY, &NSColor::labelColor());
    label.setStringValue(&NSString::from_str(crate::locale::t(
        "keep_history_setting",
    )));
    label.setFrame(NSRect::new(
        NSPoint::new(MARGIN + INDENT, y),
        NSSize::new(LABEL_W - INDENT, BUTTON_H),
    ));
    body.addSubview(&label);
    RETENTION_LABEL.with(|slot| *slot.borrow_mut() = Some(label));
    let retention = popup(mtm, y);
    retention.removeAllItems();
    for minutes in settings::HISTORY_MINUTES {
        retention.addItemWithTitle(&NSString::from_str(&retention_title(minutes)));
    }
    retention.setToolTip(Some(&NSString::from_str(crate::locale::t(
        "detail_history_forgotten",
    ))));
    retention.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "keep_history_setting",
    ))));
    wire(retention.as_ref(), sel!(retentionChanged:));
    body.addSubview(&retention);
    RETENTION_POPUP.with(|slot| *slot.borrow_mut() = Some(retention));

    gap(top, mac_ui::style::SPACE_XS);
    let sensitive = indented_checkbox(
        mtm,
        crate::locale::t("clear_sensitive"),
        take(top, BUTTON_H),
    );
    // Points to the notification for such copies, which is under Algemeen › Meldingen.
    let tip = format!(
        "{}\n{}",
        crate::locale::t("detail_clear_sensitive"),
        crate::locale::t("clear_sensitive_notify_hint")
    );
    sensitive.setToolTip(Some(&NSString::from_str(&tip)));
    wire(&sensitive, sel!(clearSensitiveToggled:));
    body.addSubview(&sensitive);
    SENSITIVE_BOX.with(|slot| *slot.borrow_mut() = Some(sensitive));

    gap(top, mac_ui::style::SPACE_XS);
    let ask = indented_checkbox(mtm, crate::locale::t("confirm_clear"), take(top, BUTTON_H));
    ask.setToolTip(Some(&NSString::from_str(crate::locale::t(
        "confirm_clear_note",
    ))));
    wire(&ask, sel!(confirmClearToggled:));
    body.addSubview(&ask);
    ASK_BOX.with(|slot| *slot.borrow_mut() = Some(ask));
}

/// A retention choice in the Settings popup: "15 min na de laatste kopie", "Tot wissen".
pub(super) fn retention_title(minutes: u32) -> String {
    if minutes == 0 {
        crate::locale::t("keep_until_cleared").to_string()
    } else {
        crate::locale::tf("keep_after_last_copy", &[&minutes])
    }
}

pub(super) fn save_ignored_field() {
    let raw = IGNORED_FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| field.stringValue().to_string())
    });
    let Some(raw) = raw else {
        return;
    };
    settings::set_ignored_apps(&raw);
    show_ignored_apps();
}

pub(super) fn show_ignored_apps() {
    IGNORED_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            let text = settings::ignored_apps_text();
            if field.stringValue().to_string() != text {
                field.setStringValue(&NSString::from_str(&text));
            }
        }
    });
}

/// The ignore field follows the stored list, and Add follows the last other frontmost app.
pub fn refresh_ignored_field() {
    show_ignored_apps();
    refresh_ignore_button();
}

/// The history checkbox and the settings under it follow the stored values (also when the ⋯
/// menu or the clear dialog changed them); those settings and the ignored apps only apply
/// while history is on, so they are greyed out otherwise.
pub fn refresh_history_controls() {
    let on = settings::clipboard_history();
    let stored = settings::load();
    HISTORY_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(on));
        }
    });
    RETENTION_LABEL.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            let color = if on {
                NSColor::labelColor()
            } else {
                NSColor::disabledControlTextColor()
            };
            label.setTextColor(Some(&color));
        }
    });
    RETENTION_POPUP.with(|slot| {
        if let Some(popup) = slot.borrow().as_ref() {
            let index = settings::HISTORY_MINUTES
                .iter()
                .position(|minutes| *minutes == stored.history_minutes)
                .unwrap_or(0);
            popup.selectItemAtIndex(index as isize);
            popup.setEnabled(on);
        }
    });
    SENSITIVE_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(stored.clear_sensitive));
            button.setEnabled(on);
        }
    });
    ASK_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(settings::confirm_clear()));
            button.setEnabled(on);
        }
    });
    let active = crate::history_mode::capture_controls_active(on);
    IGNORED_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setEnabled(active);
        }
    });
    refresh_ignore_button();
}

pub(super) fn refresh_ignore_button() {
    let enabled = crate::history_mode::capture_controls_active(settings::clipboard_history())
        && crate::capture::last_foreign().is_some();
    IGNORED_ADD.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setEnabled(enabled);
        }
    });
}
