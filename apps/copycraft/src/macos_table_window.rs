#![cfg(target_os = "macos")]
// The table window ("Open in window" in Table ▾): the table of one entry in its own resizable
// window. A toolbar with the window's Table ▾ menu and the version capsule (↶ v2/3 ↷), a
// sidebar with the columns (names and types), the grid with its first column frozen, and the
// meta line. What it shows comes from `commands::table_window_view`; what it asks for goes to
// the app as `UserEvent::TableWindow`.
//
// Protected like the card's well: the sidebar and the grid are blurred (and hidden from
// accessibility) until clicked, blurred again when the window stops being key (another
// window or app), and the app closes the window and wipes it on Wipe, Clear history, screen
// lock, sleep and when its entry is gone. Without a blur filter they stay blank under an
// opaque cover until revealed. No extra entitlements: an ordinary in-process AppKit window.

use std::cell::{Cell, RefCell};

use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::glass;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, NSObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBackingStoreType, NSBeep, NSBox, NSButton,
    NSCellImagePosition, NSColor, NSControlStateValueOn, NSEvent, NSEventGestureAxis,
    NSEventModifierFlags, NSFocusRingType, NSFont, NSMenu, NSMenuItem, NSScrollView,
    NSTextAlignment, NSTextField, NSTextView, NSView, NSWindow, NSWindowSharingType,
    NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSNotification, NSPoint, NSRange, NSRect, NSSize, NSString,
};
use mac_ui::panel;
use mac_ui::text::AttrText;
use mac_ui::widgets;

use crate::commands::{self, Command, CommandId, TableWindowView, VersionBar};
use crate::format::FormatKind;
use crate::launcher::{self, TableWindowEvent, UserEvent};

/// The window's first size; it is resizable down to [`MIN_SIZE`].
const INITIAL_SIZE: NSSize = NSSize::new(900.0, 560.0);
const MIN_SIZE: NSSize = NSSize::new(560.0, 320.0);
const TOOLBAR_H: f64 = 44.0;
const META_H: f64 = 28.0;
const SIDEBAR_W: f64 = 220.0;
const SIDEBAR_TITLE_H: f64 = 22.0;
const PAD: f64 = 12.0;
const WELL_RADIUS: f64 = 8.0;
const NAV_SYMBOL: f64 = 12.0;
const NAV_FONT: f64 = 12.0;
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
}

struct Views {
    window: Retained<TableWindowPanel>,
    grid: Retained<NSTextView>,
    frozen: Retained<NSTextView>,
    sidebar: Retained<NSTextView>,
    sidebar_title: Retained<NSTextField>,
    meta: Retained<NSTextField>,
    placeholder: Retained<NSTextField>,
    hint: Retained<NSTextField>,
    table_button: GlassButton,
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
        // Opened (or opened again): blurred until clicked.
        REVEALED.set(false);
        apply_mask();
        panel::activate_app(mtm);
        panel::bring_to_front(&window);
    }
}

/// The window in front and key again, when it is open.
pub fn bring_to_front() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let window = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.window.clone()));
    if let Some(window) = window
        && window.isVisible()
    {
        panel::activate_app(mtm);
        panel::bring_to_front(&window);
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
    PAINTED.set(false);
}

fn blur_filter() -> Option<Retained<AnyObject>> {
    BLUR.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| mac_ui::blur::gaussian(BLUR_RADIUS))
            .clone()
    })
}

fn masked() -> bool {
    !REVEALED.with(Cell::get)
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
    if !REVEALED.with(Cell::get) {
        return;
    }
    REVEALED.set(false);
    apply_mask();
}

fn run(id: CommandId) {
    launcher::emit(UserEvent::TableWindow(TableWindowEvent::Run(id)));
}

/// ⌘Z / ⇧⌘Z step through the versions, ⌘W closes.
fn window_key(event: &NSEvent) -> bool {
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
    window.setTitle(&NSString::from_str("Copycraft · Table"));
    window.setContentMinSize(MIN_SIZE);
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    // Like the card: out of screenshots, recordings and screen sharing where macOS honours it
    // (not ScreenCaptureKit on macOS 15.4+, by reports; see `macos_launcher::ensure_window`).
    window.setSharingType(NSWindowSharingType::None);
    window.center();
    let delegate = TableWindowDelegate::new(mtm);
    // SAFETY: DELEGATE keeps the delegate alive for the rest of the process, like VIEWS.
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let (width, height) = (INITIAL_SIZE.width, INITIAL_SIZE.height);
    let body_h = height - TOOLBAR_H - META_H;

    // The toolbar: Table ▾ at the left, the version capsule at the right, the hint between.
    let toolbar_y = height - TOOLBAR_H + (TOOLBAR_H - commands::CHIP_PILL_H) / 2.0;
    let table_button = GlassButton::pill(mtm, "Table ▾", ButtonSize::Regular);
    table_button.set_accessibility_label("Table steps");
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

    let hint = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    hint.setAlignment(NSTextAlignment::Center);
    let hint_x = PAD + table_w + PAD;
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
    let meta = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    meta.setFrame(NSRect::new(
        NSPoint::new(PAD, (META_H - 18.0) / 2.0),
        NSSize::new(width - PAD * 2.0, 18.0),
    ));
    meta.setAutoresizingMask(mask(&[A::ViewMaxYMargin, A::ViewWidthSizable]));
    content.addSubview(&meta);

    // The sidebar: "Columns" and one line per column (name, type).
    let sidebar_title = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    sidebar_title.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    sidebar_title.setFrame(NSRect::new(
        NSPoint::new(PAD, META_H + body_h - SIDEBAR_TITLE_H),
        NSSize::new(SIDEBAR_W - PAD * 2.0, 18.0),
    ));
    sidebar_title.setAutoresizingMask(mask(&[A::ViewMinYMargin, A::ViewMaxXMargin]));
    content.addSubview(&sidebar_title);
    let sidebar = widgets::read_only_text_view(mtm, NSSize::new(4.0, 4.0));
    sidebar.setAccessibilityLabel(Some(&NSString::from_str("Columns")));
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
    let well = widgets::filled_box(mtm, WELL_RADIUS, &NSColor::controlBackgroundColor());
    well.setFrame(grid_frame);
    well.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&well);
    let grid = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    grid.setSelectable(true);
    grid.setAccessibilityLabel(Some(&NSString::from_str("Table")));
    widgets::scroll_text_both_ways(&grid);
    let grid_scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), grid_frame);
    widgets::configure_text_scroll(&grid_scroll, &grid);
    grid_scroll.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    content.addSubview(&grid_scroll);
    let frozen = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    frozen.setDrawsBackground(true);
    frozen.setBackgroundColor(&NSColor::controlBackgroundColor());
    // A copy of what the grid says: VoiceOver reads the grid itself.
    frozen.setAccessibilityElement(false);
    frozen.setHidden(true);
    widgets::scroll_text_both_ways(&frozen);
    grid_scroll.addFloatingSubview_forAxis(&frozen, NSEventGestureAxis::Horizontal);
    let placeholder = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
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
    cover.setAccessibilityLabel(Some(&NSString::from_str("Reveal the table")));
    cover.setToolTip(Some(&NSString::from_str("Click to reveal")));
    cover.setAutoresizingMask(mask(&[A::ViewWidthSizable, A::ViewHeightSizable]));
    wire(&cover, sel!(tableWindowRevealClicked:));
    content.addSubview(&cover);

    VIEWS.with(|slot| {
        slot.replace(Some(Views {
            window,
            grid,
            frozen,
            sidebar,
            sidebar_title,
            meta,
            placeholder,
            hint,
            table_button,
            undo,
            title,
            redo,
            cover,
            shade,
        }))
    });
}

/// The card's version capsule: ↶, the version shown (a button that pops the versions) and ↷,
/// on glass (or frosted before macOS 26) inside `capsule`.
fn version_capsule(
    mtm: MainThreadMarker,
    capsule: &NSView,
) -> (GlassButton, Retained<NSButton>, GlassButton) {
    let height = commands::CHIP_PILL_H;
    let inside = glass::background(mtm, capsule, height / 2.0).content;
    let step_button = |symbol: &str, name: &str, glyph: &str, keys: &str, action: Sel| {
        let button = GlassButton::symbol(mtm, symbol, name, glyph, NAV_SYMBOL);
        let ns = button.button();
        // The capsule is the glass; no second bezel inside it.
        ns.setBordered(false);
        let has_image = ns.image().is_some();
        ns.setTitle(&NSString::from_str(if has_image { "" } else { glyph }));
        if has_image {
            ns.setImagePosition(NSCellImagePosition::ImageOnly);
        }
        ns.setAlignment(NSTextAlignment::Center);
        ns.setAccessibilityLabel(Some(&NSString::from_str(name)));
        ns.setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
        wire(ns, action);
        button
    };
    let undo = step_button(
        "arrow.uturn.backward",
        "Undo table step",
        "↶",
        "⌘Z",
        sel!(tableWindowUndoClicked:),
    );
    let redo = step_button(
        "arrow.uturn.forward",
        "Redo table step",
        "↷",
        "⇧⌘Z",
        sel!(tableWindowRedoClicked:),
    );
    let title = widgets::text_button(mtm, "v20/20", NAV_FONT);
    title.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
        NAV_FONT, 0.0,
    )));
    title.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    title.setAlignment(NSTextAlignment::Center);
    wire(&title, sel!(tableWindowVersionsClicked:));
    let title_h = title.fittingSize().height.ceil();
    undo.view().setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    title.setFrame(NSRect::new(
        NSPoint::new(commands::NAV_BUTTON, ((height - title_h) / 2.0).floor()),
        NSSize::new(commands::NAV_COUNT_W, title_h),
    ));
    redo.view().setFrame(NSRect::new(
        NSPoint::new(commands::NAV_BUTTON + commands::NAV_COUNT_W, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    inside.addSubview(undo.view());
    inside.addSubview(&title);
    inside.addSubview(redo.view());
    (undo, title, redo)
}

/// Everything from the view shown: the toolbar, the meta line and (unless blanked for a
/// missing blur) the sidebar and grid; then the mask.
fn paint() {
    VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        SHOWN.with(|shown| {
            let shown = shown.borrow();
            let Some(view) = shown.as_ref() else {
                return;
            };
            paint_chrome(views, view);
        });
        wipe_content(views);
    });
    apply_mask();
}

fn paint_chrome(views: &Views, view: &TableWindowView) {
    views
        .table_button
        .button()
        .setEnabled(!view.menu.is_empty());
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
            0 => "Columns".to_string(),
            1 => "1 column".to_string(),
            n => format!("{n} columns"),
        }));
    views
        .placeholder
        .setStringValue(&NSString::from_str(&view.placeholder));
    views.placeholder.setHidden(view.placeholder.is_empty());
}

/// The grid (with its frozen column) and the sidebar from the view shown.
fn paint_content(views: &Views, view: &TableWindowView) {
    crate::macos_card_text::paint(
        &views.grid,
        &view.grid,
        Some(FormatKind::Dataframe),
        true,
        None,
    );
    widgets::fit_text_view(&views.grid, None);
    views.grid.scrollRangeToVisible(NSRange {
        location: 0,
        length: 0,
    });
    match crate::dataframe::frozen_column(&view.grid) {
        Some(lines) => {
            crate::macos_card_text::copy_frozen_column(&views.grid, &views.frozen, &lines);
            views.frozen.setHidden(false);
        }
        None => {
            widgets::wipe_text_view(&views.frozen);
            views.frozen.setHidden(true);
        }
    }
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
        &NSFont::systemFontOfSize(12.0),
        &NSColor::labelColor(),
    );
    let dim = NSColor::tertiaryLabelColor();
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
    let font = NSFont::systemFontOfSize(12.0);
    let attr = AttrText::new(meta, &font, &NSColor::secondaryLabelColor());
    let warn_font = NSFont::boldSystemFontOfSize(12.0);
    for mark in crate::sensitivity::warning_marks(meta) {
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
    label.setAttributedStringValue(&attr.into_attributed());
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
        for text in [&views.grid, &views.frozen, &views.sidebar] {
            mac_ui::blur::set_content_filters(text, &filters);
            text.setAccessibilityElement(!masked);
        }
        views.shade.setHidden(!(masked && filter.is_none()));
        views.cover.setHidden(!masked);
        let hint = if !masked {
            ""
        } else if checking {
            "Checking for sensitive data…"
        } else {
            "Click the table to reveal it"
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
