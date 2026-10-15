// The preview of an Apple Intelligence action ("Lines… › Apple Intelligence"), drawn over the well
// on the card like the column picker: a sparkles icon and the action's name, the result in a
// scrolling, selectable text, a notice line (text cut at 20,000 characters, or why it failed),
// and Cancel (Esc) / "Apply to clipboard" (Return). While the model works a spinner shows and
// Apply is off. Part of `macos_launcher` (included there); the logic is `menubar::ai` and the
// model is `mac_ai`.
//
// Privacy: the result lives only in this text view and in `App::ai`. Closing the preview (Cancel,
// Esc, the card closing or losing focus, a new entry, Wipe) wipes the text view and, through
// `UserEvent::Run(CommandId::AiCancel)`, stops the model. Nothing is logged or stored here.

/// Inner margin of the preview.
const AI_PAD: f64 = 10.0;
const AI_BUTTON_H: f64 = 24.0;
const AI_TITLE_H: f64 = 18.0;
const AI_NOTE_H: f64 = 16.0;

thread_local! {
    static AI_VIEWS: RefCell<Option<AiViews>> = const { RefCell::new(None) };
    /// Whether the preview is open (shown over the well).
    static AI_SHOWN: Cell<bool> = const { Cell::new(false) };
}

struct AiViews {
    root: Retained<NSView>,
    title: Retained<NSTextField>,
    note: Retained<NSTextField>,
    scroll: Retained<NSScrollView>,
    text: Retained<NSTextView>,
    spinner: progress::Spinner,
    cancel: GlassButton,
    apply: GlassButton,
}

fn ai_open() -> bool {
    AI_SHOWN.with(Cell::get)
}

fn ai_views(mtm: MainThreadMarker) -> AiViews {
    let width = WIDTH - PAD * 2.0;
    let root = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, PREVIEW_H)),
    );
    root.setHidden(true);
    root.setAccessibilityElement(true);
    root.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("ai_group"))));
    // Opaque, so nothing of the well shows through.
    let fill = filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    fill.setFrame(root.bounds());
    root.addSubview(&fill);

    let top = PREVIEW_H - AI_PAD - AI_TITLE_H;
    let icon = widgets::image_view(mtm);
    if let Some(image) = mac_ui::icon::system_symbol(
        commands::AI_SYMBOL,
        crate::locale::t("ai_group"),
    ) {
        icon.setImage(Some(&image));
    }
    icon.setContentTintColor(Some(&NSColor::controlAccentColor()));
    icon.setAccessibilityElement(false);
    icon.setFrame(NSRect::new(
        NSPoint::new(AI_PAD, top),
        NSSize::new(AI_TITLE_H, AI_TITLE_H),
    ));
    root.addSubview(&icon);
    let title = widgets::label(mtm, 13.0, &NSColor::labelColor());
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    title.setFrame(NSRect::new(
        NSPoint::new(AI_PAD + AI_TITLE_H + 6.0, top),
        NSSize::new(width - AI_PAD * 2.0 - AI_TITLE_H - 6.0, AI_TITLE_H),
    ));
    root.addSubview(&title);

    let note_y = top - 2.0 - AI_NOTE_H;
    let note = widgets::label(mtm, 11.0, &NSColor::secondaryLabelColor());
    note.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    note.setFrame(NSRect::new(
        NSPoint::new(AI_PAD, note_y),
        NSSize::new(width - AI_PAD * 2.0, AI_NOTE_H),
    ));
    root.addSubview(&note);

    let button = |title: &str, spoken: &str, action: Sel| {
        let button = GlassButton::pill(mtm, title, ButtonSize::Small);
        button.set_accessibility_label(spoken);
        wire_button(button.button(), action);
        root.addSubview(button.view());
        button
    };
    let cancel = button(
        crate::locale::t("cancel"),
        crate::locale::t("cancel"),
        sel!(aiCancelClicked:),
    );
    cancel
        .button()
        .setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let apply = button(
        crate::locale::t("ai_apply"),
        crate::locale::t("ai_apply_a11y"),
        sel!(aiApplyClicked:),
    );
    apply.button().setKeyEquivalent(&NSString::from_str("\r"));
    apply.set_prominent(true);
    let mut right = width - AI_PAD;
    for item in [&apply, &cancel] {
        let w = item.width_within(220.0);
        right -= w;
        item.view().setFrame(NSRect::new(
            NSPoint::new(right, AI_PAD),
            NSSize::new(w, AI_BUTTON_H),
        ));
        right -= 6.0;
    }

    let list_bottom = AI_PAD + AI_BUTTON_H + 8.0;
    let area = NSRect::new(
        NSPoint::new(AI_PAD - 4.0, list_bottom),
        NSSize::new(width - AI_PAD * 2.0 + 8.0, note_y - 6.0 - list_bottom),
    );
    let text = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    text.setSelectable(true);
    text.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    text.setTextColor(Some(&NSColor::labelColor()));
    text.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "ai_preview_a11y",
    ))));
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), area);
    widgets::configure_text_scroll(&scroll, &text);
    scroll.setHasHorizontalScroller(false);
    root.addSubview(&scroll);
    let spinner = progress::Spinner::new(mtm, SpinnerSize::Regular);
    spinner.set_accessibility_label(crate::locale::t("ai_working"));
    AiViews {
        root,
        title,
        note,
        scroll,
        text,
        spinner,
        cancel,
        apply,
    }
}

/// An action was clicked: open the preview over the well in its working state (spinner, Apply
/// off). `notice` says that the text was cut.
pub(crate) fn show_ai_working(title: &str, notice: Option<&str>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let fresh = AI_VIEWS.with(|slot| slot.borrow().is_none());
    if fresh {
        let views = ai_views(mtm);
        let well = WELL.with(|slot| slot.borrow().clone());
        if let Some(parent) = well.and_then(|well| mac_ui::widgets::superview(&well)) {
            parent.addSubview(&views.root);
        }
        AI_VIEWS.with(|slot| slot.replace(Some(views)));
    }
    AI_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            widgets::wipe_text_view(&views.text);
            views.title.setStringValue(&NSString::from_str(title));
            set_ai_note(views, notice.unwrap_or(""), false);
            views.apply.button().setEnabled(false);
            views.cancel.button().setEnabled(true);
            // One indicator only: out of its superview first, so a second run cannot stack it.
            views.spinner.remove();
            views
                .spinner
                .add_centered(&views.root, views.scroll.frame());
            views.spinner.start();
            views.root.setHidden(false);
        }
    });
    AI_SHOWN.set(true);
    if is_open() {
        layout(false);
    }
    raise_picker();
    focus_picker_field();
}

fn set_ai_note(views: &AiViews, note: &str, failed: bool) {
    views.note.setStringValue(&NSString::from_str(note));
    let color = if failed {
        NSColor::systemRedColor()
    } else {
        NSColor::secondaryLabelColor()
    };
    views.note.setTextColor(Some(&color));
    views.note.setAccessibilityLabel(Some(&NSString::from_str(note)));
}

/// The model's answer: show it and turn Apply on. Does nothing when the preview was closed.
pub(crate) fn show_ai_result(result: &str) {
    if !ai_open() {
        return;
    }
    AI_VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        views.spinner.remove();
        views.text.setString(&NSString::from_str(result));
        widgets::fit_text_view(&views.text, Some(views.scroll.contentSize().width));
        views.text.scrollRangeToVisible(NSRange {
            location: 0,
            length: 0,
        });
        views.apply.button().setEnabled(true);
    });
    focus_picker_field();
}

/// The run failed: say why on the notice line, Apply stays off.
pub(crate) fn show_ai_error(message: &str) {
    if !ai_open() {
        return;
    }
    AI_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            views.spinner.remove();
            set_ai_note(views, message, true);
        }
    });
}

/// Close the preview and wipe its text. `stop_run`: also tell the app to stop the model (the
/// user closed it); not when the app itself closes it (Apply, Cancel finished). Whether it was
/// open.
fn dismiss_ai(stop_run: bool) -> bool {
    if !AI_SHOWN.replace(false) {
        return false;
    }
    AI_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            views.spinner.remove();
            widgets::wipe_text_view(&views.text);
            views.note.setStringValue(&NSString::from_str(""));
            views.root.setHidden(true);
        }
    });
    if stop_run {
        launcher::emit(UserEvent::Run(CommandId::AiCancel));
    }
    true
}

/// The app closes the preview (Apply, a stopped run): lay the card out again.
pub(crate) fn close_ai_preview() {
    if dismiss_ai(false) && is_open() {
        layout(false);
        focus_card();
    }
}

fn place_ai(well_y: f64) {
    AI_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            views.root.setFrameOrigin(NSPoint::new(PAD, well_y));
        }
    });
}

fn raise_ai() {
    if !ai_open() {
        return;
    }
    AI_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            raise_view(&views.root);
        }
    });
}

fn focus_ai() {
    let text = AI_VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.text.clone()));
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(text), Some(window)) = (text, window) {
        window.makeFirstResponder(Some(&text));
    }
}

/// Return applies (once there is a result), Esc closes. `None`: not the preview's key.
fn ai_key(event: &NSEvent) -> Option<bool> {
    if !ai_open() {
        return None;
    }
    match Key::from_key_code(event.keyCode()) {
        Some(Key::Return) => {
            let ready = AI_VIEWS.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|views| views.apply.button().isEnabled())
            });
            if ready {
                launcher::emit(UserEvent::Run(CommandId::AiApply));
            }
            Some(true)
        }
        Some(Key::Escape) => {
            close_picker();
            Some(true)
        }
        _ => None,
    }
}
