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
use mac_ui::objc2::AnyThread;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, NSObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSAlert, NSAlertFirstButtonReturn, NSAttributedStringNSStringDrawing,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSBeep, NSBezierPath, NSBorderType, NSBox,
    NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSEvent, NSEventGestureAxis,
    NSEventModifierFlags, NSFocusRingType, NSFont, NSLineBreakMode, NSMenu, NSMenuItem,
    NSModalPanelWindowLevel, NSMutableParagraphStyle, NSScrollView, NSTextAlignment, NSTextField,
    NSTextTab, NSTextView, NSView, NSViewBoundsDidChangeNotification, NSWindow, NSWindowStyleMask,
    NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSPoint, NSRange, NSRect, NSSize,
    NSString,
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
                // SAFETY: forwards the event to the superclass implementation of the same selector,
                // with the same argument and return types.
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
        // SAFETY: `init` on a freshly allocated instance of a class that declares no other
        // initializer.
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
    // Delay 0 runs after the menu.
    mac_ui::actions::perform_after(&delegate, sel!(tableWindowOpenColumnPicker:), 0.0);
}

// Shared painting helpers (also used by the card), textually included as before.
include!("../macos_table_paint.rs");

mod build;
mod menus;
mod paint;
mod picker;

use build::*;
use menus::*;
use paint::*;
use picker::*;
