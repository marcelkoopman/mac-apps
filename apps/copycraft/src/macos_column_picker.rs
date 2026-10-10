// The column picker ("Choose columns…" from Table ▾), drawn over the well on the card: a filter
// field, a scrolling list of checkboxes (one per column), All / None, "Keeping 8 of 20", and
// Apply (Return) / Cancel (Esc). Part of `macos_launcher` (included there); the logic is
// `crate::column_picker`. Column names and types only, never cell values. It closes with
// focus loss, Wipe, lock, retention and another entry or version.

use crate::column_picker::{ColumnPicker, PickerColumn};

/// Height of one row in the picker's list.
const PICKER_ROW_H: f64 = 22.0;
/// Inner margin of the picker panel.
const PICKER_PAD: f64 = 10.0;
/// Width kept for the dimmed type at the right of a row.
const PICKER_TYPE_W: f64 = 92.0;
const PICKER_BUTTON_H: f64 = 24.0;
const PICKER_FIELD_H: f64 = 24.0;
const PICKER_TITLE_H: f64 = 18.0;

thread_local! {
    /// The columns of the version shown, for the picker; `None` without a table frame.
    static PICKER_SOURCE: RefCell<Option<Vec<PickerColumn>>> = const { RefCell::new(None) };
    /// The entry and table version [`PICKER_SOURCE`] is for: the picker closes when it changes.
    static PICKER_KEY: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    /// The picker while it is open.
    static PICKER: RefCell<Option<ColumnPicker>> = const { RefCell::new(None) };
    static PICKER_VIEWS: RefCell<Option<PickerViews>> = const { RefCell::new(None) };
    /// Full-card click catcher behind the picker: a click outside the panel dismisses it.
    static PICKER_BACKDROP: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
}

struct PickerViews {
    root: Retained<NSView>,
    field: Retained<NSSearchField>,
    count: Retained<NSTextField>,
    scroll: Retained<NSScrollView>,
    list: Retained<NSView>,
    all: GlassButton,
    none: GlassButton,
    cancel: GlassButton,
    apply: GlassButton,
    /// The checkbox of each visible row and its column index.
    rows: Vec<(usize, Retained<NSButton>)>,
}

fn picker_open() -> bool {
    PICKER.with(|slot| slot.borrow().is_some())
}

fn show_picker_backdrop(mtm: MainThreadMarker) {
    let window = WINDOW.with(|slot| slot.borrow().clone());
    let Some(window) = window else {
        return;
    };
    let Some(content) = window.contentView() else {
        return;
    };
    let bounds = content.bounds();
    let button = PICKER_BACKDROP.with(|slot| {
        if let Some(existing) = slot.borrow().clone() {
            existing.setFrame(bounds);
            existing.setHidden(false);
            return existing;
        }
        let button = NSButton::initWithFrame(NSButton::alloc(mtm), bounds);
        button.setBordered(false);
        button.setTitle(&NSString::from_str(""));
        button.setAccessibilityElement(false);
        // Nearly invisible; catches clicks outside the panel so Cancel/Esc are not the only exits.
        // Own selector: if z-order ever puts this above the panel, clicks on the panel must not
        // dismiss (All / None / Apply stay usable).
        button.setAlphaValue(0.01);
        wire_button(&button, sel!(pickerBackdropClicked:));
        slot.replace(Some(button.clone()));
        button
    });
    button.setFrame(bounds);
    // Backdrop above the card chrome; raise_picker puts the panel above the backdrop.
    content.addSubview(&button);
    raise_picker();
}

fn hide_picker_backdrop() {
    PICKER_BACKDROP.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setHidden(true);
        }
    });
}

/// Called from [`store_with_card`]: the columns of the version shown. Another entry or
/// version closes an open picker.
fn store_picker_source(data: &LaunchData, content_key: u64) {
    let table = data.table.as_ref();
    let columns = table.and_then(commands::picker_columns);
    let key = (content_key, table.map_or(0, |table| table.frame_id));
    if PICKER_KEY.with(Cell::get) != key {
        PICKER_KEY.set(key);
        // The card is laid out after the store.
        dismiss_picker(false);
    }
    PICKER_SOURCE.with(|slot| slot.replace(columns));
}

fn picker_views(mtm: MainThreadMarker) -> PickerViews {
    let width = WIDTH - PAD * 2.0;
    let root = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, PREVIEW_H)),
    );
    root.setHidden(true);
    root.setAccessibilityElement(true);
    root.setAccessibilityRole(Some(unsafe {
        mac_ui::objc2_app_kit::NSAccessibilityGroupRole
    }));
    root.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("picker_title"))));
    // Opaque, so nothing of the well shows through.
    let fill = filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    fill.setFrame(root.bounds());
    root.addSubview(&fill);

    let top = PREVIEW_H - PICKER_PAD - PICKER_TITLE_H;
    let title = widgets::label(mtm, 13.0, &NSColor::labelColor());
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    title.setStringValue(&NSString::from_str(crate::locale::t("picker_title")));
    title.setFrame(NSRect::new(
        NSPoint::new(PICKER_PAD, top),
        NSSize::new(width / 2.0, PICKER_TITLE_H),
    ));
    root.addSubview(&title);
    let count = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    count.setAlignment(NSTextAlignment::Right);
    count.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(12.0, 0.0)));
    count.setFrame(NSRect::new(
        NSPoint::new(width / 2.0, top),
        NSSize::new(width / 2.0 - PICKER_PAD, PICKER_TITLE_H),
    ));
    root.addSubview(&count);

    let field_y = top - 6.0 - PICKER_FIELD_H;
    let field = widgets::search_field(mtm, 13.0, crate::locale::t("filter_columns"));
    field.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("filter_columns"))));
    field.setFrame(NSRect::new(
        NSPoint::new(PICKER_PAD, field_y),
        NSSize::new(width - PICKER_PAD * 2.0, PICKER_FIELD_H),
    ));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: LauncherDelegate implements controlTextDidChange: and
            // control:textView:doCommandBySelector: with their protocol signatures; DELEGATE keeps
            // it alive for the app's lifetime.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    root.addSubview(&field);

    let buttons_y = PICKER_PAD;
    let button = |title: &str, spoken: &str, action: Sel| {
        let button = GlassButton::pill(mtm, title, ButtonSize::Small);
        button.set_accessibility_label(spoken);
        wire_button(button.button(), action);
        root.addSubview(button.view());
        button
    };
    let all = button(
        crate::locale::t("picker_all"),
        crate::locale::t("picker_keep_all"),
        sel!(pickerAllClicked:),
    );
    all.button()
        .setToolTip(Some(&NSString::from_str(crate::locale::t("picker_keep_all_tip"))));
    let none = button(
        crate::locale::t("picker_none"),
        crate::locale::t("picker_keep_none"),
        sel!(pickerNoneClicked:),
    );
    let cancel = button(
        crate::locale::t("cancel"),
        crate::locale::t("cancel"),
        sel!(pickerCancelClicked:),
    );
    cancel.button().setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let apply = button(
        crate::locale::t("picker_apply"),
        crate::locale::t("picker_apply_a11y"),
        sel!(pickerApplyClicked:),
    );
    apply.button().setKeyEquivalent(&NSString::from_str("\r"));
    apply.set_prominent(true);
    let mut x = PICKER_PAD;
    for item in [&all, &none] {
        let w = item.width_within(90.0);
        item.view().setFrame(NSRect::new(
            NSPoint::new(x, buttons_y),
            NSSize::new(w, PICKER_BUTTON_H),
        ));
        x += w + 6.0;
    }
    let mut right = width - PICKER_PAD;
    for item in [&apply, &cancel] {
        let w = item.width_within(100.0);
        right -= w;
        item.view().setFrame(NSRect::new(
            NSPoint::new(right, buttons_y),
            NSSize::new(w, PICKER_BUTTON_H),
        ));
        right -= 6.0;
    }

    let list_bottom = buttons_y + PICKER_BUTTON_H + 8.0;
    let scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(
            NSPoint::new(PICKER_PAD - 4.0, list_bottom),
            NSSize::new(width - PICKER_PAD * 2.0 + 8.0, field_y - 6.0 - list_bottom),
        ),
    );
    scroll.setDrawsBackground(false);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setAutomaticallyAdjustsContentInsets(false);
    scroll.contentView().setDrawsBackground(false);
    scroll.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("columns_title"))));
    let list = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    scroll.setDocumentView(Some(&list));
    root.addSubview(&scroll);
    PickerViews {
        root,
        field,
        count,
        scroll,
        list,
        all,
        none,
        cancel,
        apply,
        rows: Vec::new(),
    }
}

/// "Choose columns…": open the picker over the well, every column checked, the filter field
/// focused. A beep without a table frame.
fn open_picker() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(columns) = PICKER_SOURCE.with(|slot| slot.borrow().clone()) else {
        NSBeep();
        return;
    };
    if columns.len() < 2 {
        NSBeep();
        return;
    }
    PICKER.with(|slot| slot.replace(Some(ColumnPicker::new(columns))));
    let fresh = PICKER_VIEWS.with(|slot| slot.borrow().is_none());
    if fresh {
        let views = picker_views(mtm);
        let well = WELL.with(|slot| slot.borrow().clone());
        if let Some(parent) = well.and_then(|well| unsafe { well.superview() }) {
            parent.addSubview(&views.root);
        }
        PICKER_VIEWS.with(|slot| slot.replace(Some(views)));
    }
    PICKER_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            widgets::wipe_text_field(&views.field);
            views.root.setHidden(false);
        }
    });
    show_picker_backdrop(mtm);
    layout(false);
    rebuild_picker_rows(mtm);
    raise_picker();
    focus_picker_field();
}

/// Put the picker over the well at `well_y` (with the well, in [`place_well`]).
fn place_picker(well_y: f64) {
    PICKER_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            views.root.setFrameOrigin(NSPoint::new(PAD, well_y));
        }
    });
}

/// The picker on top of the well and its buttons.
fn raise_picker() {
    if !picker_open() {
        return;
    }
    PICKER_VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            raise_view(&views.root);
        }
    });
}

fn focus_picker_field() {
    let field = PICKER_VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.field.clone()));
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        window.makeFirstResponder(Some(&field));
    }
}

/// Whether the current mouse event lands inside the picker panel (window coordinates).
fn mouse_in_picker() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let root = PICKER_VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|views| !views.root.isHidden())
            .map(|views| views.root.clone())
    });
    let Some(root) = root else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(event) = app.currentEvent() else {
        return false;
    };
    let loc = event.locationInWindow();
    // convertPoint:fromView: with nil is the window's base coordinates.
    let local = root.convertPoint_fromView(loc, None);
    NSMouseInRect(local, root.bounds(), root.isFlipped())
}

/// Click on the full-card backdrop: dismiss only when it is outside the panel. A hit on the
/// panel here means the backdrop was above the picker — raise the picker and keep it open so
/// All / None / Apply work.
fn picker_backdrop_clicked() {
    if mouse_in_picker() {
        raise_picker();
        return;
    }
    close_picker();
}

/// Close the picker without a step (Cancel, Esc, focus loss) and lay the card out again.
fn close_picker() {
    dismiss_picker(true);
}

/// Close the picker and forget its filter and rows. Always lays the open card out again so
/// well buttons and focus come back — even when the caller passed `relayout: false` (a version
/// or entry change used to leave Copy/Save hidden and the card stuck).
fn dismiss_picker(_relayout: bool) {
    let was_open = PICKER.with(|slot| slot.borrow_mut().take()).is_some();
    hide_picker_backdrop();
    if MainThreadMarker::new().is_some() {
        PICKER_VIEWS.with(|slot| {
            let mut borrowed = slot.borrow_mut();
            let Some(views) = borrowed.as_mut() else {
                return;
            };
            widgets::wipe_field_editor(&views.field);
            widgets::wipe_text_field(&views.field);
            for (_, row) in views.rows.drain(..) {
                row.setTitle(&NSString::from_str(""));
            }
            let subviews = views.list.subviews();
            for view in subviews.iter() {
                view.removeFromSuperview();
            }
            views.root.setHidden(true);
        });
    }
    if was_open && is_open() {
        layout(false);
        focus_card();
    }
}

/// Apply: one step with the checked columns when the set changed; when every column is still
/// kept (Apply used to stay grey), just close — same as Cancel. Beep only when nothing is kept.
fn apply_picker() {
    let (step, kept) = PICKER.with(|slot| {
        let Some(picker) = slot.borrow().clone() else {
            return (None, 0);
        };
        (picker.step(), picker.kept_count())
    });
    if kept == 0 {
        NSBeep();
        return;
    }
    close_picker();
    if let Some(step) = step {
        launcher::emit(UserEvent::Run(CommandId::TableStep(step)));
    }
}

fn picker_update(change: impl FnOnce(&mut ColumnPicker)) {
    PICKER.with(|slot| {
        if let Some(picker) = slot.borrow_mut().as_mut() {
            change(picker);
        }
    });
    sync_picker();
}

/// Checkboxes, count and Apply from the model.
fn sync_picker() {
    let Some(picker) = PICKER.with(|slot| slot.borrow().clone()) else {
        return;
    };
    PICKER_VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        for (index, row) in &views.rows {
            row.setState(if picker.is_kept(*index) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        let count = picker.count_line();
        views.count.setStringValue(&NSString::from_str(&count));
        views.count.setAccessibilityLabel(Some(&NSString::from_str(&count)));
        views.apply.button().setEnabled(picker.can_apply());
        let visible = !picker.visible().is_empty();
        views.all.button().setEnabled(visible);
        views.none.button().setEnabled(visible);
        views.cancel.button().setEnabled(true);
    });
}

/// One checkbox per column the filter shows, top to bottom in the table's order.
fn rebuild_picker_rows(mtm: MainThreadMarker) {
    let Some(picker) = PICKER.with(|slot| slot.borrow().clone()) else {
        return;
    };
    PICKER_VIEWS.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(views) = borrowed.as_mut() else {
            return;
        };
        let subviews = views.list.subviews();
        for view in subviews.iter() {
            view.removeFromSuperview();
        }
        views.rows.clear();
        let visible = picker.visible();
        let width = views.scroll.contentSize().width;
        let height = (visible.len() as f64 * PICKER_ROW_H).max(views.scroll.contentSize().height);
        views
            .list
            .setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)));
        for (position, &index) in visible.iter().enumerate() {
            let column = &picker.columns()[index];
            let y = height - (position as f64 + 1.0) * PICKER_ROW_H;
            let check = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &NSString::from_str(&column.shown),
                    None,
                    None,
                    mtm,
                )
            };
            wire_button(&check, sel!(pickerToggled:));
            check.setTag(index as isize);
            check.setFont(Some(&NSFont::systemFontOfSize(13.0)));
            check.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
            check.setToolTip(Some(&NSString::from_str(&column.name)));
            check.setAccessibilityLabel(Some(&NSString::from_str(&picker.spoken(index))));
            check.setFrame(NSRect::new(
                NSPoint::new(4.0, y),
                NSSize::new((width - PICKER_TYPE_W - 12.0).max(40.0), PICKER_ROW_H),
            ));
            let kind = widgets::label(mtm, 12.0, &NSColor::tertiaryLabelColor());
            kind.setAlignment(NSTextAlignment::Right);
            kind.setStringValue(&NSString::from_str(&column.kind));
            // The checkbox already says it.
            kind.setAccessibilityElement(false);
            kind.setFrame(NSRect::new(
                NSPoint::new(width - PICKER_TYPE_W - 4.0, y + 3.0),
                NSSize::new(PICKER_TYPE_W, 16.0),
            ));
            views.list.addSubview(&check);
            views.list.addSubview(&kind);
            views.rows.push((index, check));
        }
        // The list starts at its top row.
        let clip = views.scroll.contentView();
        let top = (height - clip.bounds().size.height).max(0.0);
        clip.scrollToPoint(NSPoint::new(0.0, top));
        views.scroll.reflectScrolledClipView(&clip);
    });
    sync_picker();
}

fn note_is_picker_field(note: &NSNotification) -> bool {
    let Some(object) = note.object() else {
        return false;
    };
    PICKER_VIEWS.with(|slot| {
        slot.borrow().as_ref().is_some_and(|views| {
            std::ptr::eq(
                views.field.as_ref() as *const NSSearchField as *const AnyObject,
                &*object as *const AnyObject,
            )
        })
    })
}

fn control_is_picker_field(control: &NSControl) -> bool {
    PICKER_VIEWS.with(|slot| {
        slot.borrow().as_ref().is_some_and(|views| {
            std::ptr::eq(
                views.field.as_ref() as *const NSSearchField as *const NSControl,
                control as *const NSControl,
            )
        })
    })
}

/// The filter changed: the list shows the columns it lets through.
fn picker_query_changed() {
    let query = PICKER_VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|views| Zeroizing::new(views.field.stringValue().to_string()))
    });
    let Some(query) = query else {
        return;
    };
    PICKER.with(|slot| {
        if let Some(picker) = slot.borrow_mut().as_mut() {
            picker.set_query(&query);
        }
    });
    if let Some(mtm) = MainThreadMarker::new() {
        rebuild_picker_rows(mtm);
    }
}

/// Return applies, Esc cancels, ↓ goes to the list.
fn picker_field_command(command: Sel) -> bool {
    if command == sel!(insertNewline:) || command == sel!(insertNewlineIgnoringFieldEditor:) {
        apply_picker();
        true
    } else if command == sel!(cancelOperation:) {
        close_picker();
        true
    } else if command == sel!(moveDown:) {
        focus_picker_row(0);
        true
    } else {
        false
    }
}

/// Keyboard focus on the `position`-th checkbox shown, scrolled into view.
fn focus_picker_row(position: usize) {
    let row = PICKER_VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|views| views.rows.get(position).map(|(_, row)| row.clone()))
    });
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(row), Some(window)) = (row, window) {
        row.scrollRectToVisible(row.bounds());
        window.makeFirstResponder(Some(&row));
    }
}

/// ↑ ↓ between the checkboxes when one has the focus; ↑ from the first goes to the filter.
fn picker_arrow(up: bool) {
    let window = WINDOW.with(|slot| slot.borrow().clone());
    let Some(window) = window else {
        return;
    };
    let focused = window.firstResponder();
    let position = PICKER_VIEWS.with(|slot| {
        slot.borrow().as_ref().and_then(|views| {
            views.rows.iter().position(|(_, row)| {
                focused.as_ref().is_some_and(|responder| {
                    std::ptr::eq(
                        Retained::as_ptr(row).cast::<AnyObject>(),
                        Retained::as_ptr(responder).cast::<AnyObject>(),
                    )
                })
            })
        })
    });
    match (position, up) {
        (Some(0), true) => focus_picker_field(),
        (Some(at), true) => focus_picker_row(at - 1),
        (Some(at), false) => focus_picker_row(at + 1),
        (None, _) => focus_picker_row(0),
    }
}

/// Keys while the picker is open, before the card's own: ⌘A All, Return Apply, Esc Cancel,
/// ↑ ↓ through the list. Space toggles the focused checkbox (AppKit). `None`: not the
/// picker's key.
fn picker_key(event: &NSEvent) -> Option<bool> {
    if !picker_open() {
        return None;
    }
    if mac_ui::keys::is_command_chord(event, "a") {
        picker_update(ColumnPicker::keep_all);
        return Some(true);
    }
    match Key::from_key_code(event.keyCode()) {
        Some(Key::Return) => {
            apply_picker();
            Some(true)
        }
        Some(Key::Escape) => {
            close_picker();
            Some(true)
        }
        Some(Key::Up) | Some(Key::Down) => {
            let on_field = PICKER_VIEWS.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|views| views.field.currentEditor().is_some())
            });
            if on_field {
                return None;
            }
            picker_arrow(Key::from_key_code(event.keyCode()) == Some(Key::Up));
            Some(true)
        }
        _ => None,
    }
}
