#![cfg(target_os = "macos")]
// The table window ("Open table" on the card): the table of one entry in its own resizable
// window. A toolbar with Table ▾, Copy, Save and the version capsule (↶ v2/3 ↷), a sidebar
// with the columns (names and types), the grid (every row, soft-capped only for huge
// tables) and the meta line. A parsed grid is drawn as aligned columns with a fixed header
// (`macos_table_paint`); a grid that does not parse keeps the boxed text and a frozen first
// column. What it shows comes from `commands::table_window_view`; what it asks for goes to
// the app as `UserEvent::TableWindow`.
//
// Opens with the grid and sidebar revealed (Open table from the card). Blurs again when
// the window stops being key (another window or app). Choose columns… and our save and open
// panels steal key; a grid that was sharp stays sharp. The app closes the window and wipes
// it on Wipe, Clear history, screen lock, sleep and when its entry is gone. Without a blur
// filter they stay blank under an opaque cover when masked. No extra entitlements.

use std::cell::{Cell, RefCell};

use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::AnyThread;
use mac_ui::objc2::runtime::{AnyObject, NSObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSAlert, NSAlertFirstButtonReturn, NSAttributedStringNSStringDrawing,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSBeep, NSBezierPath, NSBorderType, NSBox,
    NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSEvent, NSEventGestureAxis, NSEventModifierFlags, NSFocusRingType, NSFont, NSLineBreakMode,
    NSMenu, NSMenuItem, NSModalPanelWindowLevel, NSMutableParagraphStyle,
    NSParagraphStyleAttributeName, NSScrollView, NSTextAlignment, NSTextField, NSTextTab,
    NSTextView, NSView, NSViewBoundsDidChangeNotification, NSWindow, NSWindowStyleMask,
    NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSObjectNSDelayedPerforming,
    NSPoint, NSRange, NSRect, NSSize, NSString,
};
use mac_ui::panel;
use mac_ui::text::AttrText;
use mac_ui::widgets;

use crate::column_picker::ColumnPicker;
use crate::commands::{self, Command, CommandId, TableWindowView, VersionBar};
use crate::format::FormatKind;
use crate::launcher::{self, TableWindowEvent, UserEvent};
use zeroize::Zeroize;

/// The window's first size; it is resizable down to [`MIN_SIZE`].
const INITIAL_SIZE: NSSize = NSSize::new(900.0, 560.0);
const MIN_SIZE: NSSize = NSSize::new(560.0, 320.0);
const TOOLBAR_H: f64 = 44.0;
const META_H: f64 = 28.0;
const SIDEBAR_W: f64 = 220.0;
const SIDEBAR_TITLE_H: f64 = 22.0;
const PAD: f64 = mac_ui::metrics::MARGIN;
const WELL_RADIUS: f64 = mac_ui::metrics::WELL_RADIUS;
const NAV_SYMBOL: f64 = mac_ui::metrics::FONT_CAPTION;
const NAV_FONT: f64 = mac_ui::metrics::FONT_CAPTION;
/// The card's text blur (`TEXT_BLUR_RADIUS` in `macos_launcher`): lines and their shape show,
/// the characters do not.
const BLUR_RADIUS: f64 = 6.0;

thread_local! {
    static VIEWS: RefCell<Option<Views>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<TableWindowDelegate>>> = const { RefCell::new(None) };
    /// What the window shows; wiped when it is replaced or the window closes.
    static SHOWN: RefCell<Option<TableWindowView>> = const { RefCell::new(None) };
    /// The grid and sidebar were clicked since the window last became key.
    static REVEALED: Cell<bool> = const { Cell::new(false) };
    /// The grid and sidebar hold the shown view's text (not blanked for a missing blur).
    static PAINTED: Cell<bool> = const { Cell::new(false) };
    /// The commands of the menu popped last, by item tag.
    static POPPED: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// The blur filter, built once; `None` inside when Core Image cannot build it.
    static BLUR: RefCell<Option<Option<Retained<AnyObject>>>> = const { RefCell::new(None) };
    /// Choose columns… modal is up: do not blur on resign-key (the alert takes key).
    static COLUMN_PICKER_OPEN: Cell<bool> = const { Cell::new(false) };
    /// Our save or open panel is up. It takes key; a revealed grid stays sharp.
    static OWN_PANEL: Cell<bool> = const { Cell::new(false) };
    /// Checkboxes of the open Choose columns… alert (All / None toggle these).
    static PICKER_CHECKS: RefCell<Vec<Retained<NSButton>>> = const { RefCell::new(Vec::new()) };
}

struct Views {
    window: Retained<TableWindowPanel>,
    grid: Retained<NSTextView>,
    frozen: Retained<NSTextView>,
    /// Column names, kept at the top of the grid while the body scrolls.
    header: Retained<NSTextView>,
    header_clip: Retained<NSView>,
    header_back: Retained<NSBox>,
    header_rule: Retained<NSBox>,
    /// Rounded fill behind the grid. The grid paints its own opaque background on top.
    well: Retained<NSBox>,
    sidebar: Retained<NSTextView>,
    sidebar_title: Retained<NSTextField>,
    meta: Retained<NSTextField>,
    placeholder: Retained<NSTextField>,
    hint: Retained<NSTextField>,
    table_button: GlassButton,
    copy_button: GlassButton,
    save_button: GlassButton,
    undo: GlassButton,
    title: Retained<NSButton>,
    redo: GlassButton,
    cover: Retained<NSButton>,
    shade: Retained<NSBox>,
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftTableWindow"]
    struct TableWindowPanel;

    impl TableWindowPanel {
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if window_key(event) {
                true
            } else {
                let handled: bool =
                    unsafe { msg_send![super(self), performKeyEquivalent: event] };
                handled
            }
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftTableWindowDelegate"]
    struct TableWindowDelegate;

    impl TableWindowDelegate {
        /// Another window or app took the focus: blurred again.
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _note: &NSNotification) {
            mask_again();
        }

        /// Closed by its close button or ⌘W: wiped, and the app forgets it. (The app's own
        /// [`close`] orders it out instead, so this is the user's close only.)
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _note: &NSNotification) {
            forget();
            launcher::emit(UserEvent::TableWindow(TableWindowEvent::Closed));
        }

        #[unsafe(method(tableWindowRevealClicked:))]
        fn reveal_clicked(&self, _sender: Option<&NSButton>) {
            reveal();
        }

        #[unsafe(method(tableWindowMenuClicked:))]
        fn menu_clicked(&self, _sender: Option<&NSButton>) {
            pop_table_menu();
        }

        #[unsafe(method(tableWindowUndoClicked:))]
        fn undo_clicked(&self, _sender: Option<&NSButton>) {
            run(CommandId::TableUndo);
        }

        #[unsafe(method(tableWindowRedoClicked:))]
        fn redo_clicked(&self, _sender: Option<&NSButton>) {
            run(CommandId::TableRedo);
        }

        #[unsafe(method(tableWindowCopyClicked:))]
        fn copy_clicked(&self, _sender: Option<&NSButton>) {
            run(CommandId::Copy);
        }

        #[unsafe(method(tableWindowSaveClicked:))]
        fn save_clicked(&self, _sender: Option<&NSButton>) {
            run(CommandId::Save);
        }

        #[unsafe(method(tableWindowVersionsClicked:))]
        fn versions_clicked(&self, _sender: Option<&NSButton>) {
            pop_versions_menu();
        }

        #[unsafe(method(tableWindowPoppedClicked:))]
        fn popped_clicked(&self, sender: Option<&NSMenuItem>) {
            let Some(item) = sender else {
                return;
            };
            let Ok(index) = usize::try_from(item.tag()) else {
                return;
            };
            let id = POPPED.with(|slot| slot.borrow().get(index).map(|cmd| cmd.id.clone()));
            if let Some(id) = id {
                run(id);
            }
        }

        /// After the Table menu closes: show Choose columns… (modal).
        #[unsafe(method(tableWindowOpenColumnPicker:))]
        fn open_column_picker_clicked(&self, _sender: Option<&AnyObject>) {
            run_column_picker();
        }

        #[unsafe(method(tableWindowPickerAll:))]
        fn picker_all_clicked(&self, _sender: Option<&NSButton>) {
            set_picker_checks(true);
        }

        #[unsafe(method(tableWindowPickerNone:))]
        fn picker_none_clicked(&self, _sender: Option<&NSButton>) {
            set_picker_checks(false);
        }

        /// The grid scrolled sideways: slide the header row with it.
        #[unsafe(method(tableWindowGridScrolled:))]
        fn grid_scrolled(&self, _note: &NSNotification) {
            on_grid_scrolled();
        }
    }
);

define_class!(
    /// Grid text view that paints full-width row stripes behind the glyphs.
    #[unsafe(super(NSTextView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftTableGrid"]
    struct TableGridView;

    impl TableGridView {
        #[unsafe(method(drawViewBackgroundInRect:))]
        fn draw_view_background(&self, rect: NSRect) {
            draw_table_background(self, rect);
        }
    }
);

impl TableWindowDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm);
        unsafe { msg_send![this, init] }
    }
}

/// Show `view` in the table window, opening (and focusing) it when it is not on screen.
pub fn show(view: TableWindowView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    ensure_views(mtm);
    SHOWN.with(|slot| slot.replace(Some(view)));
    paint();
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window
        && !window.isVisible()
    {
        // Opened (or opened again): content visible — user came from the card's table.
        REVEALED.set(true);
        apply_mask();
        panel::activate_app(mtm);
        panel::bring_to_front(&window);
    }
}

/// Point the window's fixed labels at the current language and lay the toolbar out again.
/// Pill titles change width (`Kopieer` is wider than `Copy`); the hint moves with them.
/// Nothing when the window has not been built. The meta line, sidebar title and hint text
/// are filled by the next paint.
pub fn relabel() {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        let t = crate::locale::t;
        views
            .window
            .setTitle(&NSString::from_str(t("table_window_title")));
        views.table_button.set_title(t("table_menu"));
        views
            .table_button
            .set_accessibility_label(t("table_steps_a11y"));
        views.copy_button.set_title(t("copy"));
        views
            .copy_button
            .set_accessibility_label(t("copy_table_a11y"));
        views
            .copy_button
            .button()
            .setToolTip(Some(&NSString::from_str(t("copy_table_tip"))));
        views.save_button.set_title(t("save"));
        views
            .save_button
            .set_accessibility_label(t("save_table_a11y"));
        views
            .save_button
            .button()
            .setToolTip(Some(&NSString::from_str(t("save_table_tip"))));

        let width = views
            .window
            .contentView()
            .map(|view| view.frame().size.width)
            .unwrap_or(INITIAL_SIZE.width);
        let toolbar_y = views.table_button.view().frame().origin.y;
        let height = commands::CHIP_PILL_H;
        let table_w = views.table_button.width_within(120.0);
        views.table_button.view().setFrame(NSRect::new(
            NSPoint::new(PAD, toolbar_y),
            NSSize::new(table_w, height),
        ));
        let mut x = PAD + table_w + mac_ui::metrics::BUTTON_GAP;
        let copy_w = views.copy_button.width_within(100.0);
        views.copy_button.view().setFrame(NSRect::new(
            NSPoint::new(x, toolbar_y),
            NSSize::new(copy_w, height),
        ));
        x += copy_w + mac_ui::metrics::BUTTON_GAP;
        let save_w = views.save_button.width_within(100.0);
        views.save_button.view().setFrame(NSRect::new(
            NSPoint::new(x, toolbar_y),
            NSSize::new(save_w, height),
        ));
        x += save_w + PAD;
        let hint = views.hint.frame();
        let hint_w = (width - PAD - commands::VERSION_SPAN - PAD - x).max(10.0);
        views.hint.setFrame(NSRect::new(
            NSPoint::new(x, hint.origin.y),
            NSSize::new(hint_w, hint.size.height),
        ));

        label_step(&views.undo, t("undo_table_step"), "⌘Z");
        label_step(&views.redo, t("redo_table_step"), "⇧⌘Z");
        views
            .sidebar
            .setAccessibilityLabel(Some(&NSString::from_str(t("columns_title"))));
        views
            .grid
            .setAccessibilityLabel(Some(&NSString::from_str(t("table_view"))));
        views
            .cover
            .setAccessibilityLabel(Some(&NSString::from_str(t("reveal_table"))));
        views
            .cover
            .setToolTip(Some(&NSString::from_str(t("click_to_reveal"))));
    });
}

/// Undo or redo. The drawn title stays the arrow; VoiceOver and the tooltip follow the language.
fn label_step(button: &GlassButton, name: &str, keys: &str) {
    button.set_accessibility_label(name);
    button
        .button()
        .setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
}

/// Give the window (when made) the sharing type "Allow screenshots" asks for.
pub fn apply_capture() {
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window {
        window.setSharingType(crate::macos_launcher::sharing_type());
    }
}

/// Close the window and wipe what it shows (Wipe, Clear history, lock, sleep, its entry
/// gone). Nothing when it is not open.
pub fn close() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    forget();
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window
        && window.isVisible()
    {
        window.orderOut(None);
    }
}

/// Wipe the views and what they were made from.
fn forget() {
    REVEALED.set(false);
    PAINTED.set(false);
    // Dropping the view wipes it.
    SHOWN.with(|slot| slot.replace(None));
    POPPED.with(|slot| {
        for cmd in slot.borrow_mut().iter_mut() {
            cmd.wipe();
        }
        slot.borrow_mut().clear();
    });
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        wipe_content(views);
        widgets::wipe_text_field(&views.meta);
        widgets::wipe_text_field(&views.placeholder);
        let empty = NSString::from_str("");
        views.title.setAccessibilityLabel(Some(&empty));
        views.title.setToolTip(Some(&empty));
    });
}

/// Overwrite the grid, its frozen column and the sidebar.
fn wipe_content(views: &Views) {
    widgets::wipe_text_view(&views.grid);
    widgets::wipe_text_view(&views.frozen);
    views.frozen.setHidden(true);
    widgets::wipe_text_view(&views.sidebar);
    clear_sheet_chrome(views);
    PAINTED.set(false);
}

fn blur_filter() -> Option<Retained<AnyObject>> {
    BLUR.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| mac_ui::blur::gaussian(BLUR_RADIUS))
            .clone()
    })
}

/// Masked now: the privacy filter setting (read live) is on and the window is not revealed.
fn masked() -> bool {
    commands::effective_mask(
        crate::settings::privacy_filter(),
        true,
        REVEALED.with(Cell::get),
    )
}

/// The privacy filter was switched in Settings. The reveal is forgotten, so switching on blurs
/// an open window at once (a click reveals it again); switching off shows it unmasked.
pub fn privacy_filter_changed() {
    REVEALED.set(false);
    apply_mask();
}

/// A click on the blurred grid or sidebar: sharp, unless the labels are still being checked.
fn reveal() {
    let checking = SHOWN.with(|slot| slot.borrow().as_ref().is_some_and(|view| view.checking));
    if checking {
        NSBeep();
        return;
    }
    REVEALED.set(true);
    apply_mask();
}

/// The window is no longer key: blurred again; the next click reveals it.
fn mask_again() {
    // Choose columns… and our save and open panels are modals: they steal key. Keep the
    // table revealed under them.
    if COLUMN_PICKER_OPEN.with(Cell::get) || OWN_PANEL.with(Cell::get) {
        return;
    }
    if !REVEALED.with(Cell::get) {
        return;
    }
    REVEALED.set(false);
    apply_mask();
}

fn visible_window() -> Option<Retained<TableWindowPanel>> {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        borrowed
            .as_ref()
            .and_then(|views| views.window.isVisible().then(|| views.window.clone()))
    })
}

/// While `body` runs, a revealed grid does not blur. The flag stays set until [`end_own_panel`].
fn begin_own_panel() -> (Option<Retained<TableWindowPanel>>, bool) {
    let window = visible_window();
    let keep = window.is_some() && REVEALED.with(Cell::get);
    if keep {
        OWN_PANEL.set(true);
    }
    (window, keep)
}

fn end_own_panel(keep: bool) {
    if keep {
        // Not `reveal`: that beeps while labels are still being checked. This grid was
        // already shown.
        REVEALED.set(true);
        apply_mask();
    }
    OWN_PANEL.set(false);
}

/// Run `body` (the open panel) without blurring a revealed grid. Does not change which
/// window is key; the caller puts the previous one back.
pub fn holding_reveal<T>(body: impl FnOnce() -> T) -> T {
    let (_window, keep) = begin_own_panel();
    let result = body();
    end_own_panel(keep);
    result
}

/// Run `body` (the table window's save panel) without blurring a revealed grid. Afterwards
/// the window is in front again. A grid that was sharp stays sharp and key, so a later real
/// focus loss still blurs it. A grid that was already blurred stays blurred.
pub fn keeping_revealed<T>(body: impl FnOnce() -> T) -> T {
    let (window, keep) = begin_own_panel();
    let result = body();
    // Still holding while this window comes back: ordering it must not count as a blur.
    if let Some(window) = window.as_ref() {
        if let Some(mtm) = MainThreadMarker::new() {
            panel::activate_app(mtm);
        }
        panel::bring_to_front(window);
    }
    end_own_panel(keep);
    result
}

/// Schedule Choose columns… after the Table menu finishes tracking (so the alert is not
/// hidden behind it).
pub fn open_column_picker() {
    let delegate = DELEGATE.with(|slot| slot.borrow().clone());
    let Some(delegate) = delegate else {
        return;
    };
    // SAFETY: open_column_picker_clicked takes an optional sender; delay 0 runs after the menu.
    unsafe {
        delegate.performSelector_withObject_afterDelay(
            sel!(tableWindowOpenColumnPicker:),
            None,
            0.0,
        );
    }
}

/// Modal Choose columns… for the table window: All / None, checkboxes, Apply / Cancel.
/// Uses the same [`ColumnPicker`] model as the card; Apply emits one select step. The table
/// stays revealed (the alert steals key; we do not re-blur on Apply or Cancel).
fn run_column_picker() {
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
    let btn_h = 24.0;
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

    let all = GlassButton::pill(mtm, crate::locale::t("picker_all"), ButtonSize::Small);
    all.set_accessibility_label(crate::locale::t("picker_keep_all"));
    wire(all.button(), sel!(tableWindowPickerAll:));
    let none = GlassButton::pill(mtm, crate::locale::t("picker_none"), ButtonSize::Small);
    none.set_accessibility_label(crate::locale::t("picker_keep_none"));
    wire(none.button(), sel!(tableWindowPickerNone:));
    let mut x = pad;
    for item in [&all, &none] {
        let w = item.width_within(90.0);
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
        let check = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(&column.shown),
                None,
                None,
                mtm,
            )
        };
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
    // Alert stole key; always leave the table window revealed after Choose columns….
    reveal();
}

fn set_picker_checks(kept: bool) {
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

fn run(id: CommandId) {
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::Run(id)));
}

/// ⌘O opens a file, ⌘S saves this table, ⌘Z / ⇧⌘Z step through the versions, ⌘W closes.
fn window_key(event: &NSEvent) -> bool {
    if mac_ui::keys::is_command_chord(event, "o") {
        launcher::emit(UserEvent::Run(CommandId::ChooseFile));
        return true;
    }
    if mac_ui::keys::is_command_chord(event, "s") {
        run(CommandId::Save);
        return true;
    }
    if mac_ui::keys::is_command_chord(event, "w") {
        let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
        if let Some(window) = window {
            window.performClose(None);
        }
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

fn wire(control: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: every action wired here is a TableWindowDelegate method taking the
            // sender; DELEGATE keeps it alive for the app's lifetime.
            unsafe { widgets::set_target_action(control, delegate, action) };
        }
    });
}

fn mask(options: &[NSAutoresizingMaskOptions]) -> NSAutoresizingMaskOptions {
    options
        .iter()
        .fold(NSAutoresizingMaskOptions::empty(), |all, one| all | *one)
}

fn ensure_views(mtm: MainThreadMarker) {
    if VIEWS.with(|slot| slot.borrow().is_some()) {
        return;
    }
    use NSAutoresizingMaskOptions as A;
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), INITIAL_SIZE);
    let window: Retained<TableWindowPanel> = unsafe {
        msg_send![
            TableWindowPanel::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // SAFETY: VIEWS holds a `Retained` to the window, so AppKit must not also release it on
    // close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(crate::locale::t("table_window_title")));
    window.setContentMinSize(MIN_SIZE);
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    // Like the card: out of screenshots, recordings and screen sharing where macOS honours it
    // (not ScreenCaptureKit on macOS 15.4+, by reports; see `macos_launcher::ensure_window`),
    // unless "Allow screenshots" is on.
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();
    let delegate = TableWindowDelegate::new(mtm);
    // SAFETY: DELEGATE keeps the delegate alive for the rest of the process, like VIEWS.
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let (width, height) = (INITIAL_SIZE.width, INITIAL_SIZE.height);
    let body_h = height - TOOLBAR_H - META_H;

    // The toolbar: Table ▾, Copy, Save; version capsule at the right; hint between.
    let toolbar_y = height - TOOLBAR_H + (TOOLBAR_H - commands::CHIP_PILL_H) / 2.0;
    let table_button = GlassButton::pill(mtm, crate::locale::t("table_menu"), ButtonSize::Regular);
    table_button.set_accessibility_label(crate::locale::t("table_steps_a11y"));
    wire(table_button.button(), sel!(tableWindowMenuClicked:));
    let table_w = table_button.width_within(120.0);
    table_button.view().setFrame(NSRect::new(
        NSPoint::new(PAD, toolbar_y),
        NSSize::new(table_w, commands::CHIP_PILL_H),
    ));
    table_button
        .view()
        .setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(table_button.view());

    let copy_button = GlassButton::pill(mtm, crate::locale::t("copy"), ButtonSize::Regular);
    copy_button.set_accessibility_label(crate::locale::t("copy_table_a11y"));
    copy_button
        .button()
        .setToolTip(Some(&NSString::from_str(crate::locale::t(
            "copy_table_tip",
        ))));
    wire(copy_button.button(), sel!(tableWindowCopyClicked:));
    let copy_w = copy_button.width_within(100.0);
    let mut actions_x = PAD + table_w + mac_ui::metrics::BUTTON_GAP;
    copy_button.view().setFrame(NSRect::new(
        NSPoint::new(actions_x, toolbar_y),
        NSSize::new(copy_w, commands::CHIP_PILL_H),
    ));
    copy_button
        .view()
        .setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(copy_button.view());
    actions_x += copy_w + mac_ui::metrics::BUTTON_GAP;

    let save_button = GlassButton::pill(mtm, crate::locale::t("save"), ButtonSize::Regular);
    save_button.set_accessibility_label(crate::locale::t("save_table_a11y"));
    save_button
        .button()
        .setToolTip(Some(&NSString::from_str(crate::locale::t(
            "save_table_tip",
        ))));
    wire(save_button.button(), sel!(tableWindowSaveClicked:));
    let save_w = save_button.width_within(100.0);
    save_button.view().setFrame(NSRect::new(
        NSPoint::new(actions_x, toolbar_y),
        NSSize::new(save_w, commands::CHIP_PILL_H),
    ));
    save_button
        .view()
        .setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(save_button.view());
    actions_x += save_w + PAD;

    let capsule = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(width - PAD - commands::VERSION_SPAN, toolbar_y),
            NSSize::new(commands::VERSION_SPAN, commands::CHIP_PILL_H),
        ),
    );
    capsule.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMinXMargin]));
    let (undo, title, redo) = version_capsule(mtm, &capsule);
    content.addSubview(&capsule);

    let hint = widgets::label(mtm, mac_ui::metrics::FONT_CAPTION, &crate::appearance::caption_color());
    hint.setAlignment(NSTextAlignment::Center);
    let hint_x = actions_x;
    hint.setFrame(NSRect::new(
        NSPoint::new(hint_x, toolbar_y + 5.0),
        NSSize::new(
            (width - PAD - commands::VERSION_SPAN - PAD - hint_x).max(10.0),
            18.0,
        ),
    ));
    hint.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewWidthSizable]));
    content.addSubview(&hint);

    // The meta line along the bottom.
    let meta = widgets::label(mtm, mac_ui::metrics::FONT_CAPTION, &crate::appearance::caption_color());
    meta.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    meta.setUsesSingleLineMode(true);
    meta.setMaximumNumberOfLines(1);
    meta.setFrame(NSRect::new(
        NSPoint::new(PAD, (META_H - 18.0) / 2.0),
        NSSize::new(width - PAD * 2.0, 18.0),
    ));
    meta.setAutoresizingMask(mask(&[A::ViewMaxYMargin, A::ViewWidthSizable]));
    content.addSubview(&meta);

    // The sidebar: "Columns" and one line per column (name, type).
    let sidebar_title = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    sidebar_title.setFrame(NSRect::new(
        NSPoint::new(PAD, META_H + body_h - SIDEBAR_TITLE_H),
        NSSize::new(SIDEBAR_W - PAD * 2.0, 18.0),
    ));
    sidebar_title.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(&sidebar_title);
    let sidebar = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    sidebar.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("columns_title"))));
    let sidebar_scroll = widgets::passive_text_scroll(mtm, &sidebar);
    sidebar_scroll.setFrame(NSRect::new(
        NSPoint::new(PAD - 4.0, META_H),
        NSSize::new(SIDEBAR_W - PAD, body_h - SIDEBAR_TITLE_H - 4.0),
    ));
    sidebar_scroll.setAutoresizingMask(mask(&[A::ViewHeightSizable, A::ViewMaxXMargin]));
    content.addSubview(&sidebar_scroll);

    // The grid in a well, as on the card, with its first column frozen.
    let grid_frame = NSRect::new(
        NSPoint::new(SIDEBAR_W, META_H),
        NSSize::new(width - SIDEBAR_W - PAD, body_h),
    );
    let well = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::textBackgroundColor());
    crate::appearance::apply_well(&well, false, true);
    well.setFrame(grid_frame);
    well.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&well);
    let grid = table_grid_view(mtm);
    grid.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("table_view"))));
    widgets::scroll_text_both_ways(&grid);
    // Clip host around the scroller: clips the tall floating first column without
    // setClipsToBounds on the NSScrollView (which breaks scrolling).
    let grid_clip = NSView::initWithFrame(NSView::alloc(mtm), grid_frame);
    grid_clip.setClipsToBounds(true);
    grid_clip.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&grid_clip);
    let grid_scroll = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), grid_frame.size),
    );
    widgets::configure_text_scroll(&grid_scroll, &grid);
    grid_scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    grid_clip.addSubview(&grid_scroll);
    let clip_view = grid_scroll.contentView();
    clip_view.setPostsBoundsChangedNotifications(true);
    if let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) {
        let observer: &AnyObject = &delegate;
        let object: &AnyObject = &clip_view;
        // SAFETY: tableWindowGridScrolled: is a TableWindowDelegate method taking the
        // notification. DELEGATE keeps the observer alive. The clip view is the object and
        // stays in this window.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                observer,
                sel!(tableWindowGridScrolled:),
                Some(NSViewBoundsDidChangeNotification),
                Some(object),
            );
        }
    }
    let frozen = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    frozen.setDrawsBackground(true);
    // Opaque plate: the same colour the grid paints, so the scrolling body does not show
    // through the frozen column and the two stay one surface.
    frozen.setBackgroundColor(&NSColor::controlBackgroundColor());
    // A copy of what the grid says: VoiceOver reads the grid itself.
    frozen.setAccessibilityElement(false);
    frozen.setHidden(true);
    widgets::scroll_text_both_ways(&frozen);
    grid_scroll.addFloatingSubview_forAxis(&frozen, NSEventGestureAxis::Horizontal);
    // Header strip above the scroller (non-flipped: pinned to the top of the clip). The body
    // scroll view keeps a top margin of the same height while the sheet is showing.
    let header_clip = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    header_clip.setClipsToBounds(true);
    header_clip.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMinYMargin]));
    header_clip.setHidden(true);
    let header_back = widgets::filled_box(mtm, 0.0, &NSColor::controlBackgroundColor());
    header_back.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    header_clip.addSubview(&header_back);
    let header = widgets::read_only_text_view(mtm, NSSize::new(12.0, 4.0));
    header.setDrawsBackground(true);
    header.setBackgroundColor(&NSColor::controlBackgroundColor());
    widgets::scroll_text_both_ways(&header);
    header_clip.addSubview(&header);
    let header_rule = widgets::filled_box(mtm, 0.0, &NSColor::separatorColor());
    header_rule.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewMaxYMargin]));
    header_rule.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(10.0, 1.0)));
    header_clip.addSubview(&header_rule);
    grid_clip.addSubview(&header_clip);
    let placeholder = widgets::label(mtm, mac_ui::metrics::FONT_BODY, &crate::appearance::caption_color());
    placeholder.setAlignment(NSTextAlignment::Center);
    placeholder.setFrame(NSRect::new(
        NSPoint::new(SIDEBAR_W, META_H + body_h / 2.0 - 9.0),
        NSSize::new(width - SIDEBAR_W - PAD, 18.0),
    ));
    placeholder.setAutoresizingMask(mask(&[
        A::ViewWidthSizable,
        A::ViewMinYMargin,
        A::ViewMaxYMargin,
    ]));
    placeholder.setHidden(true);
    content.addSubview(&placeholder);

    // The cover over the sidebar and the grid: a click reveals them. Its shade stands in for
    // a blur that cannot be installed.
    let cover_frame = NSRect::new(NSPoint::new(0.0, META_H), NSSize::new(width, body_h));
    let shade = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::windowBackgroundColor());
    shade.setFrame(cover_frame);
    shade.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    shade.setHidden(true);
    content.addSubview(&shade);
    let cover = NSButton::initWithFrame(NSButton::alloc(mtm), cover_frame);
    cover.setBordered(false);
    cover.setTransparent(true);
    cover.setTitle(&NSString::from_str(""));
    cover.setFocusRingType(NSFocusRingType::Default);
    cover.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("reveal_table"))));
    cover.setToolTip(Some(&NSString::from_str(crate::locale::t(
        "click_to_reveal",
    ))));
    cover.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    wire(&cover, sel!(tableWindowRevealClicked:));
    content.addSubview(&cover);

    VIEWS.with(|slot| {
        slot.replace(Some(Views {
            window,
            grid,
            frozen,
            header,
            header_clip,
            header_back,
            header_rule,
            well,
            sidebar,
            sidebar_title,
            meta,
            placeholder,
            hint,
            table_button,
            copy_button,
            save_button,
            undo,
            title,
            redo,
            cover,
            shade,
        }))
    });
}

/// The card's version row: ↶, the version shown (a button that pops the versions) and ↷.
/// Separate glass buttons with the same gap as the card, not a second glass capsule.
fn version_capsule(
    mtm: MainThreadMarker,
    capsule: &NSView,
) -> (GlassButton, Retained<NSButton>, GlassButton) {
    let height = commands::CHIP_PILL_H;
    let step_button = |symbol: &str, name: &str, glyph: &str, keys: &str, action: Sel| {
        let button = GlassButton::symbol(mtm, symbol, name, glyph, NAV_SYMBOL);
        let ns = button.button();
        ns.setAlignment(NSTextAlignment::Center);
        ns.setAccessibilityLabel(Some(&NSString::from_str(name)));
        ns.setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
        wire(ns, action);
        button
    };
    let undo = step_button(
        "arrow.uturn.backward",
        crate::locale::t("undo_table_step"),
        "↶",
        "⌘Z",
        sel!(tableWindowUndoClicked:),
    );
    let redo = step_button(
        "arrow.uturn.forward",
        crate::locale::t("redo_table_step"),
        "↷",
        "⇧⌘Z",
        sel!(tableWindowRedoClicked:),
    );
    let title = widgets::text_button(mtm, "v20/20", NAV_FONT);
    title.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
        NAV_FONT, 0.0,
    )));
    title.setContentTintColor(Some(&crate::appearance::caption_color()));
    title.setAlignment(NSTextAlignment::Center);
    wire(&title, sel!(tableWindowVersionsClicked:));
    let title_h = title.fittingSize().height.ceil();
    undo.view().setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    title.setFrame(NSRect::new(
        NSPoint::new(
            commands::nav_mid_x(),
            ((height - title_h) / 2.0).floor(),
        ),
        NSSize::new(commands::NAV_COUNT_W, title_h),
    ));
    redo.view().setFrame(NSRect::new(
        NSPoint::new(commands::nav_end_x(), 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    capsule.addSubview(undo.view());
    capsule.addSubview(&title);
    capsule.addSubview(redo.view());
    (undo, title, redo)
}

/// Well border and caption colours follow Reduce Transparency and Increase Contrast.
/// The frozen column and the header strip stay an opaque plate in the grid's own colour,
/// so scrolling cells do not show through and the plate does not become a second surface.
fn apply_table_chrome(views: &Views) {
    crate::appearance::apply_well(&views.well, false, true);
    let plate = NSColor::controlBackgroundColor();
    views.header_back.setFillColor(&plate);
    views.frozen.setBackgroundColor(&plate);
    views.header.setBackgroundColor(&plate);
    let caption = crate::appearance::caption_color();
    views.hint.setTextColor(Some(&caption));
    views.sidebar_title.setTextColor(Some(&caption));
    views.placeholder.setTextColor(Some(&caption));
    views.title.setContentTintColor(Some(&caption));
}

/// Everything from the view shown: the toolbar, the meta line and (unless blanked for a
/// missing blur) the sidebar and grid; then the mask.
fn paint() {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        apply_table_chrome(views);
        let empty = SHOWN.with(|shown| {
            let shown = shown.borrow();
            let Some(view) = shown.as_ref() else {
                return true;
            };
            paint_chrome(views, view);
            view.grid.is_empty()
        });
        // Replace the text in place. Wiping first flashes an empty well when the window
        // repaints for appearance or a new table.
        if empty {
            wipe_content(views);
        } else {
            PAINTED.set(false);
        }
    });
    apply_mask();
}

fn paint_chrome(views: &Views, view: &TableWindowView) {
    views
        .table_button
        .button()
        .setEnabled(!view.menu.is_empty());
    // Enabled once the version's grid is there (not while "Working on the table…").
    let ready = !view.grid.is_empty();
    views.copy_button.button().setEnabled(ready);
    views.save_button.button().setEnabled(ready);
    views.undo.button().setEnabled(view.can_undo());
    views.redo.button().setEnabled(view.can_redo());
    views
        .title
        .setTitle(&NSString::from_str(&view.version_title()));
    if !view.versions.labels.is_empty() {
        let bar = VersionBar {
            labels: view.versions.labels.clone(),
            current: view.versions.current.min(view.versions.labels.len() - 1),
        };
        let spoken = NSString::from_str(&bar.spoken());
        views.title.setAccessibilityLabel(Some(&spoken));
        views.title.setToolTip(Some(&spoken));
    }
    paint_meta(&views.meta, &view.meta);
    let columns = view.columns.len();
    views
        .sidebar_title
        .setStringValue(&NSString::from_str(&match columns {
            0 => crate::locale::t("columns_title").to_string(),
            1 => crate::locale::t("one_column").to_string(),
            n => crate::locale::tf("n_columns", &[&n]),
        }));
    views
        .placeholder
        .setStringValue(&NSString::from_str(&view.placeholder));
    views.placeholder.setHidden(view.placeholder.is_empty());
}

/// The grid (with its frozen column) and the sidebar from the view shown.
fn paint_content(views: &Views, view: &TableWindowView) {
    paint_table_grid(views, view);
    let mut text = String::new();
    let mut kinds = Vec::new();
    for column in &view.columns {
        text.push_str(&column.shown);
        text.push_str("  ");
        let start = text.len();
        text.push_str(&column.kind);
        kinds.push(start..text.len());
        text.push('\n');
    }
    let attr = AttrText::new(
        &text,
        &NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION),
        &NSColor::labelColor(),
    );
    let dim = if crate::appearance::display().increase_contrast {
        crate::appearance::caption_color()
    } else {
        NSColor::tertiaryLabelColor()
    };
    for range in &kinds {
        attr.color(range, &dim);
    }
    if let Some(storage) = unsafe { views.sidebar.textStorage() } {
        storage.setAttributedString(&attr.into_attributed());
    }
    zeroize::Zeroize::zeroize(&mut text);
    widgets::fit_text_view(&views.sidebar, Some(SIDEBAR_W - PAD));
    PAINTED.set(true);
}

/// The meta line, its sensitivity labels marked as on the card.
fn paint_meta(label: &NSTextField, meta: &str) {
    let flat = meta.replace('\n', "  ·  ");
    let font = NSFont::systemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    let attr = AttrText::new(&flat, &font, &crate::appearance::caption_color());
    let warn_font = NSFont::boldSystemFontOfSize(mac_ui::metrics::FONT_CAPTION);
    for mark in crate::sensitivity::warning_marks(&flat) {
        let range = mark.start..mark.end;
        let (ink, wash) = match mark.label {
            crate::sensitivity::Label::Pii => (NSColor::blackColor(), NSColor::systemYellowColor()),
            crate::sensitivity::Label::Financial | crate::sensitivity::Label::Credential => {
                (NSColor::whiteColor(), NSColor::systemRedColor())
            }
        };
        attr.font(&range, &warn_font)
            .color(&range, &ink)
            .background(&range, &wash);
    }
    label.setUsesSingleLineMode(true);
    label.setMaximumNumberOfLines(1);
    let style = NSMutableParagraphStyle::new();
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let built = attr.into_attributed();
    let range = NSRange {
        location: 0,
        length: built.length(),
    };
    // SAFETY: NSParagraphStyleAttributeName takes an NSParagraphStyle.
    unsafe {
        built.addAttribute_value_range(NSParagraphStyleAttributeName, &style, range);
    }
    label.setAttributedStringValue(&built);
}

/// Blur (or blank) the sidebar and grid while masked, sharp once revealed.
fn apply_mask() {
    let masked = masked();
    let filter = if masked { blur_filter() } else { None };
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        let checking = SHOWN.with(|shown| {
            let shown = shown.borrow();
            let Some(view) = shown.as_ref() else {
                return false;
            };
            // Without a blur, masked content is not drawn at all.
            let blank = masked && filter.is_none();
            if blank {
                wipe_content(views);
            } else if !PAINTED.get() && !view.grid.is_empty() {
                paint_content(views, view);
            }
            view.checking
        });
        let filters = match &filter {
            Some(filter) => NSArray::from_slice(&[&**filter]),
            None => NSArray::from_slice(&[]),
        };
        for text in [&views.grid, &views.frozen, &views.sidebar, &views.header] {
            mac_ui::blur::set_content_filters(text, &filters);
            text.setAccessibilityElement(!masked);
        }
        views.shade.setHidden(!(masked && filter.is_none()));
        views.cover.setHidden(!masked);
        let hint = if !masked {
            ""
        } else if checking {
            crate::locale::t("checking_sensitive")
        } else {
            crate::locale::t("click_table_to_reveal")
        };
        views.hint.setStringValue(&NSString::from_str(hint));
    });
}

/// Pop `items` (a check mark on the `true` ones) under `anchor`; a pick goes to the app as
/// the window's command. Items of one group go into a submenu under its name, as on the card.
fn pop_menu(items: Vec<(Command, bool)>, anchor: &NSView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if items.is_empty() {
        NSBeep();
        return;
    }
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let mut group: Option<(&str, Retained<NSMenu>)> = None;
    let delegate = DELEGATE.with(|slot| slot.borrow().clone());
    for (index, (cmd, checked)) in items.iter().enumerate() {
        let target = match commands::menu_group(&cmd.id) {
            None => {
                group = None;
                menu.clone()
            }
            Some(name) => match &group {
                Some((shown, submenu)) if *shown == name => submenu.clone(),
                _ => {
                    let parent = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(mtm),
                            &NSString::from_str(name),
                            None,
                            &NSString::from_str(""),
                        )
                    };
                    let submenu =
                        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(name));
                    submenu.setAutoenablesItems(false);
                    parent.setSubmenu(Some(&submenu));
                    menu.addItem(&parent);
                    group = Some((name, submenu.clone()));
                    submenu
                }
            },
        };
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(&cmd.title),
                Some(sel!(tableWindowPoppedClicked:)),
                &NSString::from_str(""),
            )
        };
        item.setTag(index as isize);
        if *checked {
            item.setState(NSControlStateValueOn);
        }
        if let Some(delegate) = delegate.as_ref() {
            // SAFETY: DELEGATE keeps the target alive for the app's lifetime.
            unsafe { item.setTarget(Some(delegate)) };
        }
        target.addItem(&item);
    }
    POPPED.with(|slot| {
        let mut popped = slot.borrow_mut();
        for cmd in popped.iter_mut() {
            cmd.wipe();
        }
        *popped = items.into_iter().map(|(cmd, _)| cmd).collect();
    });
    let below = NSPoint::new(0.0, anchor.frame().size.height + 4.0);
    menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(anchor));
}

fn pop_table_menu() {
    let items = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| view.menu.clone())
            .unwrap_or_default()
    });
    let anchor = VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|views| views.table_button.button().retain())
    });
    if let Some(anchor) = anchor {
        pop_menu(items, &anchor);
    }
}

fn pop_versions_menu() {
    let bar = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|view| !view.versions.labels.is_empty())
            .map(|view| VersionBar {
                labels: view.versions.labels.clone(),
                current: view.versions.current.min(view.versions.labels.len() - 1),
            })
    });
    let Some(bar) = bar else {
        return;
    };
    let anchor = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.title.clone()));
    if let Some(anchor) = anchor {
        pop_menu(bar.menu(), &anchor);
    }
}

include!("macos_table_paint.rs");
