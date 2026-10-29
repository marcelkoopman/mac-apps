#![cfg(target_os = "macos")]
// The table window ("Open Table" on the card): the table of one entry in its own resizable
// window. A native toolbar (`toolbar`: Table ▾, Copy, Copy as JSON, Copy as CSV, Save, the search field), a sidebar
// with the columns (names and types), the grid (every row, soft-capped only for huge
// tables), the version strip along the bottom (`strip`, as the history on the card) and the
// meta line. A parsed grid is drawn as aligned columns with a fixed header
// (`macos_table_paint`); a grid that does not parse keeps the boxed text and a frozen first
// column. What it shows comes from `commands::table_window_view`; what it asks for goes to
// the app as `UserEvent::TableWindow`.
//
// Opens with the grid and sidebar revealed (Open table from the card). Blurs again when
// the window stops being key (another window or app). Choose Columns… and our save and open
// panels steal key; a grid that was sharp stays sharp. The app closes the window and wipes
// it on Wipe, Clear History, screen lock, sleep and when its entry is gone. Without a blur
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
    NSModalPanelWindowLevel, NSMutableParagraphStyle, NSScrollView, NSSearchField, NSTextAlignment,
    NSTextField, NSTextTab, NSTextView, NSToolbar, NSToolbarDelegate, NSToolbarItem, NSView,
    NSViewBoundsDidChangeNotification, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSDictionary, NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint,
    NSRange, NSRect, NSSize, NSString,
};
use mac_ui::panel;
use mac_ui::text::AttrText;
use mac_ui::widgets;

use crate::column_picker::ColumnPicker;
use crate::commands::{self, Command, CommandId, TableWindowView};
use crate::format::FormatKind;
use crate::launcher::{self, TableWindowEvent, UserEvent};
use zeroize::Zeroize;

/// The window's first size; it is resizable down to [`MIN_SIZE`].
const INITIAL_SIZE: NSSize = NSSize::new(900.0, 560.0);
const MIN_SIZE: NSSize = NSSize::new(560.0, 320.0);
const META_H: f64 = 28.0;
const SIDEBAR_W: f64 = 220.0;
const SIDEBAR_TITLE_H: f64 = 22.0;
const PAD: f64 = mac_ui::metrics::MARGIN;
const WELL_RADIUS: f64 = mac_ui::metrics::WELL_RADIUS;
/// The card's text blur (`TEXT_BLUR_RADIUS` in `macos_launcher`): lines and their shape show,
/// the characters do not.
const BLUR_RADIUS: f64 = 6.0;

thread_local! {
    static VIEWS: RefCell<Option<Views>> = const { RefCell::new(None) };
    /// The toolbar's items, handed to AppKit by the delegate.
    static TOOLBAR: RefCell<Option<toolbar::ToolbarItems>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<TableWindowDelegate>>> = const { RefCell::new(None) };
    /// What the window shows; wiped when it is replaced or the window closes.
    static SHOWN: RefCell<Option<TableWindowView>> = const { RefCell::new(None) };
    /// The grid and sidebar were clicked since the window last became key.
    static REVEALED: Cell<bool> = const { Cell::new(false) };
    /// The grid and sidebar hold the shown view's text (not blanked for a missing blur).
    static PAINTED: Cell<bool> = const { Cell::new(false) };
    /// The grid cell its right-click menu was opened on (row, column).
    static MENU_CELL: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
    /// The commands behind the toolbar's menus (Table ▾ and the versions), by item tag.
    static POPPED: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// The blur filter, built once; `None` inside when Core Image cannot build it.
    static BLUR: RefCell<Option<Option<Retained<AnyObject>>>> = const { RefCell::new(None) };
    /// Choose Columns… modal is up: do not blur on resign-key (the alert takes key).
    static COLUMN_PICKER_OPEN: Cell<bool> = const { Cell::new(false) };
    /// Our save or open panel is up. It takes key; a revealed grid stays sharp.
    static OWN_PANEL: Cell<bool> = const { Cell::new(false) };
    /// Checkboxes of the open Choose Columns… alert (All / None toggle these).
    static PICKER_CHECKS: RefCell<Vec<Retained<NSButton>>> = const { RefCell::new(Vec::new()) };
    /// What the toolbar's search field says (never logged); zeroized when it changes and when
    /// the window is wiped. Filters the rows drawn, not the table.
    static SEARCH: RefCell<String> = const { RefCell::new(String::new()) };
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
    cover: Retained<NSButton>,
    shade: Retained<NSBox>,
    /// The versions along the bottom ([`strip`]).
    strip: strip::StripViews,
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

        /// A card in the version strip: show that version (its tag is the version's index).
        #[unsafe(method(tableWindowVersionClicked:))]
        fn version_clicked(&self, sender: Option<&NSButton>) {
            if let Some(index) = sender.and_then(|button| usize::try_from(button.tag()).ok()) {
                run(commands::version_tile_click(index));
            }
        }

        #[unsafe(method(tableWindowCopyClicked:))]
        fn copy_clicked(&self, _sender: Option<&AnyObject>) {
            run(CommandId::Copy);
        }

        /// *Copy as JSON* on the toolbar: what Table ▾'s item does (toast included).
        #[unsafe(method(tableWindowCopyJsonClicked:))]
        fn copy_json_clicked(&self, _sender: Option<&AnyObject>) {
            run(CommandId::TableCopyAs(crate::dataframe::TableCopy::Json));
        }

        /// *Copy as CSV* on the toolbar: what Table ▾'s item does (toast included).
        #[unsafe(method(tableWindowCopyCsvClicked:))]
        fn copy_csv_clicked(&self, _sender: Option<&AnyObject>) {
            run(CommandId::TableCopyAs(crate::dataframe::TableCopy::Csv));
        }

        #[unsafe(method(tableWindowSaveClicked:))]
        fn save_clicked(&self, _sender: Option<&AnyObject>) {
            run(CommandId::Save);
        }

        /// A row of the grid's right-click menu: tag 0 the cell, 1 its row, 2 its column.
        #[unsafe(method(tableWindowCellMenuClicked:))]
        fn cell_menu_clicked(&self, sender: Option<&NSMenuItem>) {
            let (Some(item), Some((row, col))) = (sender, MENU_CELL.get()) else {
                return;
            };
            let menu = commands::table_cell_menu(row, col);
            if let Some(cmd) = usize::try_from(item.tag()).ok().and_then(|i| menu.get(i)) {
                run(cmd.id.clone());
            }
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

        /// After the Table menu closes: show Choose Columns… (modal).
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

        /// The search field's text changed (after a pause in the typing, or cleared with Esc
        /// or its ×): draw only the rows that match.
        #[unsafe(method(tableWindowSearchChanged:))]
        fn search_changed(&self, sender: Option<&NSSearchField>) {
            if let Some(field) = sender {
                set_search(field.stringValue().to_string());
            }
        }

        /// The grid scrolled sideways: slide the header row with it.
        #[unsafe(method(tableWindowGridScrolled:))]
        fn grid_scrolled(&self, _note: &NSNotification) {
            on_grid_scrolled();
        }
    }

    // SAFETY: NSObject conforms to NSObjectProtocol and this subclass changes nothing about it.
    unsafe impl NSObjectProtocol for TableWindowDelegate {}

    // SAFETY: the methods below have the protocol's selectors, argument and return types; the
    // returned items and arrays are retained (autoreleased on return by objc2).
    // The toolbar's items, all fixed: the default set is the allowed set, nothing to customise.
    unsafe impl NSToolbarDelegate for TableWindowDelegate {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn toolbar_item(
            &self,
            _toolbar: &NSToolbar,
            id: &NSString,
            _inserted: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            toolbar::item(id)
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbar_default_items(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            toolbar::identifiers()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbar_allowed_items(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            toolbar::identifiers()
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

        /// Right-click or ⌃-click on a cell: Copy Cell, Copy Row, Copy Column. A masked or
        /// boxed grid keeps the text view's own menu.
        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
            cell_menu(self, event).or_else(|| self.menu())
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

/// Activate the app and put the table window in front and key, also when it was already open.
pub fn bring_forward() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window {
        panel::activate_app(mtm);
        panel::bring_to_front(&window);
    }
}

/// Point the window's fixed labels (title, toolbar, sidebar, grid, cover) at the current
/// language; the toolbar lays itself out. Nothing when the window has not been built. The meta line, sidebar title and hint text
/// are filled by the next paint.
pub fn relabel() {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        let t = crate::locale::t;
        let title = SHOWN.with(|shown| {
            crate::commands::table_window_title(
                shown
                    .borrow()
                    .as_ref()
                    .and_then(|view| view.source_name.as_deref()),
            )
        });
        views.window.setTitle(&NSString::from_str(&title));
        toolbar::relabel();
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

/// Give the window (when made) the sharing type "Allow Screenshots" asks for.
/// The table window's number, when it is open.
pub(crate) fn window_number() -> Option<isize> {
    VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|views| views.window.windowNumber())
    })
}

pub fn apply_capture() {
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window {
        window.setSharingType(crate::macos_launcher::sharing_type());
    }
}

/// Close the window and wipe what it shows (Wipe, Clear History, lock, sleep, its entry
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
    SEARCH.with(|slot| slot.borrow_mut().zeroize());
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
        strip::wipe(views);
        widgets::wipe_text_field(&views.meta);
        widgets::wipe_text_field(&views.placeholder);
    });
    toolbar::wipe();
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

/// The blur for a masked table, or `None` (a blank, shaded table) when the blur setting is off
/// (the default) or Core Image cannot build it.
fn blur_filter() -> Option<Retained<AnyObject>> {
    if !crate::settings::load().blur {
        return None;
    }
    BLUR.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| mac_ui::blur::gaussian(BLUR_RADIUS))
            .clone()
    })
}

/// The grid's cell menu for a right-click at `event` ([`commands::table_cell_menu`]); `None`
/// when the grid is masked, not drawn as columns, or the click is below the last row.
fn cell_menu(grid: &TableGridView, event: &NSEvent) -> Option<Retained<NSMenu>> {
    let mtm = MainThreadMarker::new()?;
    if masked() || !TABLE_STRIPES.with(Cell::get) {
        return None;
    }
    let point = grid.convertPoint_fromView(event.locationInWindow(), None);
    let at = grid.characterIndexForInsertionAtPoint(point);
    let mut body = grid.string().to_string();
    let cell = commands::grid_cell_at(&body, at, FIRST_NUMERIC.with(Cell::get));
    body.zeroize();
    let (row, col) = cell?;
    // A search draws only the matching rows: the menu acts on the table's row.
    let row = SHOWN_ROWS.with(|rows| commands::table_row_shown(rows.borrow().as_deref(), row))?;
    MENU_CELL.set(Some((row, col)));
    let menu = menus::empty_menu(mtm);
    let delegate = DELEGATE.with(|slot| slot.borrow().clone());
    for (tag, cmd) in commands::table_cell_menu(row, col).into_iter().enumerate() {
        let item = mac_ui::actions::menu_item_with_action(
            mtm,
            &cmd.title,
            sel!(tableWindowCellMenuClicked:),
        );
        item.setTag(tag as isize);
        if let Some(delegate) = delegate.as_ref() {
            mac_ui::actions::set_menu_target(&item, delegate);
        }
        menu.addItem(&item);
    }
    Some(menu)
}

/// The search field says `text`: repaint the grid with the rows that match (when it is drawn;
/// a blank masked grid is filtered when it is revealed).
fn set_search(mut text: String) {
    let changed = SEARCH.with(|slot| {
        let mut query = slot.borrow_mut();
        if *query == text {
            return false;
        }
        query.zeroize();
        std::mem::swap(&mut *query, &mut text);
        true
    });
    text.zeroize();
    if changed {
        PAINTED.set(false);
        apply_mask();
    }
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
    // Choose Columns… and our save and open panels are modals: they steal key. Keep the
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

/// Schedule Choose Columns… after the Table menu finishes tracking (so the alert is not
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
mod strip;
mod toolbar;

/// The card theme changed: repaint the grid in the new colours (text, header, stripes).
pub(crate) fn apply_card_theme() {
    VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            apply_table_chrome(views);
            let (text, header, _) = crate::appearance::table_grid_colours();
            views.grid.setTextColor(Some(&text));
            views.header.setTextColor(Some(&header));
            views.grid.setNeedsDisplay(true);
        }
    });
}

/// Reduce Transparency or Increase Contrast changed: restyle the toolbar, well and captions.
pub(crate) fn restyle() {
    VIEWS.with(|slot| {
        if let Some(views) = slot.borrow().as_ref() {
            apply_table_chrome(views);
        }
    });
}

use build::*;
use paint::*;
use picker::*;
