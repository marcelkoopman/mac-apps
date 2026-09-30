#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::ops::Range;

use mac_ui::glass;
use mac_ui::icon::system_symbol;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol, Sel};
use mac_ui::objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSBackgroundColorAttributeName, NSBackingStoreType, NSBorderType, NSBox, NSButton,
    NSCellImagePosition, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn,
    NSEvent, NSEventModifierFlags, NSFloatingWindowLevel, NSFocusRingType, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSImage, NSImageScaling, NSImageView,
    NSLineBreakMode, NSMenu, NSMenuItem, NSScreen, NSScrollView, NSSearchField, NSTextAlignment,
    NSTextField, NSTextView, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use mac_ui::objc2_foundation::{
    NSArray, NSEdgeInsets, NSMutableAttributedString, NSNotification, NSPoint, NSRange, NSRect,
    NSSize, NSString,
};
use mac_ui::widgets::{self, filled_box, raise_view, round_view};
use zeroize::Zeroize;

use crate::appearance::Theme;
use crate::commands::{self, ChipFrame, Command, CommandId, LaunchData};
use crate::format::FormatKind;
use crate::launcher::{self, UserEvent};
use crate::macos_preview_image::nsimage_from_bytes;

const WIDTH: f64 = 440.0;
const PAD: f64 = 14.0;
const HEADER_H: f64 = 32.0;
const HEADER_BUTTON: f64 = 22.0;
const CLEAR_BUTTON_W: f64 = 64.0;
const PREVIEW_H: f64 = 264.0;
const WELL_ACTION: f64 = 26.0;
const WELL_INSET: f64 = 8.0;
const WELL_GAP: f64 = 6.0;
/// Horizontal padding already inside the text view. The gutter is the extra
/// space that keeps lines clear of the icons.
const TEXT_INSET_X: f64 = 12.0;
const META_H: f64 = 22.0;
const SEARCH_H: f64 = 36.0;
/// Fixed in-item field under the header. Hidden until the well is revealed.
const ITEM_FIND_H: f64 = 28.0;
const ITEM_COUNT_W: f64 = 56.0;
const GAP: f64 = 8.0;
const HOTKEY: &str = crate::hotkey::LABEL;
/// Strong enough that 12pt text in the well cannot be read.
const BLUR_RADIUS: f64 = 22.0;

#[link(name = "CoreImage", kind = "framework")]
unsafe extern "C" {
    static kCIInputRadiusKey: *const AnyObject;
}

thread_local! {
    static OPEN: Cell<bool> = const { Cell::new(false) };
    static SUPPRESS_RESIGN: Cell<bool> = const { Cell::new(false) };
    static SEARCHING: Cell<bool> = const { Cell::new(false) };
    static SELECTION: Cell<usize> = const { Cell::new(0) };
    static THEME: Cell<Theme> = const { Cell::new(Theme::System) };
    static SHOWS_IMAGE: Cell<bool> = const { Cell::new(false) };
    static THUMB_TOKEN: Cell<isize> = const { Cell::new(-1) };
    static ACTIONS: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    static POOL: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    static OVERFLOW: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    static SHOWN: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    static FRAMES: RefCell<Vec<ChipFrame>> = const { RefCell::new(Vec::new()) };
    static CARD_TITLE: RefCell<String> = const { RefCell::new(String::new()) };
    static CARD_META: RefCell<String> = const { RefCell::new(String::new()) };
    static CARD_EXCERPT: RefCell<String> = const { RefCell::new(String::new()) };
    static CARD_PLACEHOLDER: RefCell<String> = const { RefCell::new(String::new()) };
    static CARD_HIGHLIGHT: RefCell<Option<FormatKind>> = const { RefCell::new(None) };
    static CARD_SELECTABLE: Cell<bool> = const { Cell::new(false) };
    static PAINTED: RefCell<Option<(String, Option<FormatKind>, bool)>> =
        const { RefCell::new(None) };
    static LINK_PAGE: RefCell<Option<String>> = const { RefCell::new(None) };
    static LINK_CACHE: RefCell<Vec<(String, CachedLink)>> = const { RefCell::new(Vec::new()) };
    static LINK_IN_FLIGHT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static WARM_LINKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static WINDOW: RefCell<Option<Retained<LauncherWindow>>> = const { RefCell::new(None) };
    static FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static HEADER: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static META: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static WELL: RefCell<Option<Retained<NSBox>>> = const { RefCell::new(None) };
    static PREVIEW_TEXT: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
    static PREVIEW_SCROLL: RefCell<Option<Retained<PreviewScroll>>> = const { RefCell::new(None) };
    static PREVIEW_IMAGE: RefCell<Option<Retained<NSImageView>>> = const { RefCell::new(None) };
    static PILLS: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
    static HISTORY_NAV: RefCell<Option<commands::HistoryNav>> = const { RefCell::new(None) };
    static OLDER: RefCell<Option<NavButton>> = const { RefCell::new(None) };
    static NEWER: RefCell<Option<NavButton>> = const { RefCell::new(None) };
    static MORE: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static CLEAR: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static CLOSE: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static CONTENT_ACTIONS: Cell<commands::ContentActions> = const {
        Cell::new(commands::ContentActions {
            copy: false,
            save: false,
        })
    };
    static TEXT_GUTTER: Cell<f64> = const { Cell::new(-1.0) };
    static COPY_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    static SAVE_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    static REVEAL: RefCell<Option<RevealCover>> = const { RefCell::new(None) };
    static REVEALED: Cell<bool> = const { Cell::new(false) };
    /// In-item search is visible only while the well is revealed.
    static FIND_ON: Cell<bool> = const { Cell::new(false) };
    /// Zero-based index of the match Enter last landed on.
    static FIND_INDEX: Cell<usize> = const { Cell::new(0) };
    static ITEM_FIELD: RefCell<Option<Retained<NSSearchField>>> = const { RefCell::new(None) };
    static ITEM_COUNT: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static ITEM_QUERY: RefCell<String> = const { RefCell::new(String::new()) };
    /// Text the field searches. Image and link wells keep this empty.
    static ITEM_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
    static BLUR_ON: Cell<bool> = const { Cell::new(false) };
    static SHADE_ON: Cell<bool> = const { Cell::new(false) };
    static MASKS: Cell<bool> = const { Cell::new(false) };
    static CONTENT_KEY: Cell<u64> = const { Cell::new(0) };
    static DELEGATE: RefCell<Option<Retained<LauncherDelegate>>> = const { RefCell::new(None) };
}

#[cfg(test)]
thread_local! {
    static FAIL_BLUR: Cell<bool> = const { Cell::new(false) };
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftLauncherWindow"]
    struct LauncherWindow;

    impl LauncherWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            on_key(event);
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if is_item_find_chord(event) {
                if item_find_on() {
                    focus_item_field();
                }
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
    #[unsafe(super(NSScrollView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftPreviewScroll"]
    struct PreviewScroll;

    impl PreviewScroll {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            false
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftLauncherDelegate"]
    struct LauncherDelegate;

    impl LauncherDelegate {
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _note: &NSNotification) {
            if SUPPRESS_RESIGN.with(Cell::get) {
                return;
            }
            hide();
        }

        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, note: &NSNotification) {
            if note_is_item_field(note) {
                take_item_query_from_field();
                return;
            }
            let query = current_query();
            if query.as_str() == "/" {
                set_query("");
                SEARCHING.set(true);
            } else if !query.is_empty() {
                SEARCHING.set(true);
            }
            SELECTION.set(0);
            layout(false);
        }

        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(
            &self,
            control: &NSControl,
            _text_view: &NSTextView,
            command: Sel,
        ) -> bool {
            if control_is_item_field(control) {
                item_field_command(command)
            } else if command == sel!(moveDown:) {
                nudge(0, 1);
                true
            } else if command == sel!(moveUp:) {
                nudge(0, -1);
                true
            } else if command == sel!(moveLeft:) {
                nudge(-1, 0);
                true
            } else if command == sel!(moveRight:) {
                nudge(1, 0);
                true
            } else if command == sel!(insertNewline:) {
                activate_selected();
                true
            } else if command == sel!(cancelOperation:)
                || (command == sel!(deleteBackward:) && current_query().is_empty())
            {
                close_search();
                true
            } else {
                false
            }
        }

        #[unsafe(method(chipClicked:))]
        fn chip_clicked(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let index = button.tag();
            if index < 0 {
                return;
            }
            SELECTION.set(index as usize);
            activate_selected();
        }

        #[unsafe(method(moreClicked:))]
        fn more_clicked(&self, _sender: Option<&NSButton>) {
            pop_overflow();
        }

        #[unsafe(method(closeClicked:))]
        fn close_clicked(&self, _sender: Option<&NSButton>) {
            hide();
        }

        #[unsafe(method(clearClicked:))]
        fn clear_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::Clear));
        }

        #[unsafe(method(olderClicked:))]
        fn older_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::HistoryOlder));
        }

        #[unsafe(method(newerClicked:))]
        fn newer_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::HistoryNewer));
        }

        #[unsafe(method(copyClicked:))]
        fn copy_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::Copy));
        }

        #[unsafe(method(saveClicked:))]
        fn save_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::Save));
        }

        #[unsafe(method(revealClicked:))]
        fn reveal_clicked(&self, _sender: Option<&NSButton>) {
            if !MASKS.with(Cell::get) || REVEALED.with(Cell::get) {
                return;
            }
            REVEALED.set(true);
            layout(false);
        }

        #[unsafe(method(overflowClicked:))]
        fn overflow_clicked(&self, sender: Option<&NSMenuItem>) {
            let Some(item) = sender else {
                return;
            };
            let index = item.tag();
            if index < 0 {
                return;
            }
            activate_overflow(index as usize);
        }
    }
);

impl LauncherDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm);
        unsafe { msg_send![this, init] }
    }
}

pub fn is_open() -> bool {
    OPEN.with(Cell::get)
}

pub fn set_suppress_resign(suppress: bool) {
    SUPPRESS_RESIGN.set(suppress);
}

pub fn order_front() {
    if !is_open() {
        return;
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.makeKeyAndOrderFront(None);
            window.orderFrontRegardless();
        }
    });
}

pub fn summon(data: LaunchData) {
    if is_open() {
        hide();
        return;
    }
    reveal(data);
}

pub fn reveal(data: LaunchData) {
    let fresh = !is_open();
    present(data, fresh);
}

pub fn sync(data: LaunchData) {
    if !is_open() {
        return;
    }
    store(data);
    layout(false);
}

fn present(data: LaunchData, fresh: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    activate_app(mtm);
    ensure_window(mtm);
    if fresh {
        set_query("");
        SEARCHING.set(false);
        SELECTION.set(0);
    }
    if fresh {
        REVEALED.set(false);
    }
    store(data);
    OPEN.set(true);
    layout(fresh);
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.makeKeyAndOrderFront(None);
            window.orderFrontRegardless();
        }
    });
    focus_card();
    // Layout runs before the window is on screen. Setting the picture only then
    // leaves the layer empty, and Original stays on that blank well.
    if SHOWS_IMAGE.with(Cell::get) {
        THUMB_TOKEN.set(-1);
        show_preview_image();
        load_thumbnail();
    }
    // The picture and the text scroller are brought forward after the arrows
    // are placed. Put the blur, then `<` `>` and the well icons, back on top.
    if well_is_masked() {
        show_reveal_cover(true);
    }
    raise_history_nav();
    raise_content_actions();
}

fn store(data: LaunchData) {
    let key = commands::content_key(&data);
    if CONTENT_KEY.with(Cell::get) != key {
        CONTENT_KEY.set(key);
        REVEALED.set(false);
        set_item_find(false);
    }
    let card = commands::work_card(&data);
    MASKS.set(commands::masks_content(&card, data.view));
    CARD_TITLE.with(|slot| set_secret(slot, card.title));
    CARD_META.with(|slot| set_secret(slot, card.meta));
    CARD_EXCERPT.with(|slot| set_secret(slot, card.excerpt));
    CARD_PLACEHOLDER.with(|slot| set_secret(slot, card.placeholder));
    CARD_HIGHLIGHT.with(|slot| slot.replace(card.highlight));
    CARD_SELECTABLE.set(card.selectable);
    LINK_PAGE.with(|slot| set_secret_opt(slot, card.link_page));
    SHOWS_IMAGE.set(card.shows_image);
    publish_search_text();
    THEME.set(data.theme);
    ACTIONS.with(|slot| set_commands(slot, commands::chips(&data)));
    POOL.with(|slot| set_commands(slot, commands::search_pool(&data)));
    OVERFLOW.with(|slot| set_commands(slot, commands::overflow(&data)));
    HISTORY_NAV.with(|slot| slot.replace(data.history_nav));
    CONTENT_ACTIONS.set(commands::content_actions(&data));
    WARM_LINKS.with(|slot| {
        wipe_strings(&mut slot.borrow_mut());
        *slot.borrow_mut() = data.warm_links.clone();
    });
}

fn hide() {
    set_item_find(false);
    if !is_open() && !window_is_visible() {
        return;
    }
    SUPPRESS_RESIGN.with(|flag| flag.set(true));
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.orderOut(None);
        }
    });
    SUPPRESS_RESIGN.with(|flag| flag.set(false));
    OPEN.set(false);
}

fn window_is_visible() -> bool {
    WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|window| window.isVisible())
    })
}

fn activate_app(mtm: MainThreadMarker) {
    use mac_ui::objc2_app_kit::NSApplication;
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

fn ensure_window(mtm: MainThreadMarker) {
    if WINDOW.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, 420.0));
    let style = NSWindowStyleMask::Borderless;
    let allocated = LauncherWindow::alloc(mtm);
    let window: Retained<LauncherWindow> = unsafe {
        msg_send![
            allocated,
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setHasShadow(true);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setLevel(NSFloatingWindowLevel);
    window.setMovableByWindowBackground(true);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    let delegate = LauncherDelegate::new(mtm);
    unsafe {
        let _: () = msg_send![&*window, setDelegate: &*delegate];
    }
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    let window_view = window.contentView().expect("content view");
    window_view.setWantsLayer(true);
    window_view.setLayerUsesCoreImageFilters(true);
    round_view(&window_view, 16.0);

    // Liquid Glass on macOS 26+, else the frosted view. Before 26, `content` is `window_view`.
    let content = glass::background(mtm, &window_view, 16.0).content;

    let header = widgets::label(mtm, 13.0, &NSColor::labelColor());
    let meta = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    // Keep the warning at the end when a long filename is cut short.
    meta.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    meta.setAllowsEditingTextAttributes(true);
    let field = search_field(mtm);
    let item_find = item_search_field(mtm);
    let item_count = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    item_count.setAlignment(NSTextAlignment::Right);
    item_count.setHidden(true);
    let well = filled_box(mtm, 10.0, &NSColor::controlBackgroundColor());
    let preview_text = payload_view(mtm);
    let preview_scroll = text_scroll(mtm, &preview_text);
    let preview_image = image_view(mtm);
    let pills = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
    let older = nav_button(mtm, "<", sel!(olderClicked:));
    let newer = nav_button(mtm, ">", sel!(newerClicked:));
    let more = icon_button(mtm, "⋯", sel!(moreClicked:));
    let clear = icon_button(mtm, "Wipe", sel!(clearClicked:));
    clear.setToolTip(Some(&NSString::from_str("Wipe copied data from memory")));
    let close = icon_button(mtm, "✕", sel!(closeClicked:));
    let copy_button = well_action(mtm, "doc.on.doc", "⎘", sel!(copyClicked:));
    let save_button = well_action(mtm, "square.and.arrow.down", "↓", sel!(saveClicked:));
    let reveal = reveal_cover(mtm);

    content.addSubview(&header);
    content.addSubview(&item_find);
    content.addSubview(&item_count);
    content.addSubview(&more);
    content.addSubview(&clear);
    content.addSubview(&close);
    content.addSubview(&well);
    content.addSubview(&preview_scroll);
    content.addSubview(&preview_image);
    content.addSubview(&reveal.root);
    content.addSubview(&meta);
    content.addSubview(&field);
    content.addSubview(&pills);
    content.addSubview(&older.root);
    content.addSubview(&newer.root);
    content.addSubview(&copy_button.root);
    content.addSubview(&save_button.root);

    HEADER.with(|slot| slot.replace(Some(header)));
    META.with(|slot| slot.replace(Some(meta)));
    FIELD.with(|slot| slot.replace(Some(field)));
    ITEM_FIELD.with(|slot| slot.replace(Some(item_find)));
    ITEM_COUNT.with(|slot| slot.replace(Some(item_count)));
    WELL.with(|slot| slot.replace(Some(well)));
    PREVIEW_TEXT.with(|slot| slot.replace(Some(preview_text)));
    PREVIEW_SCROLL.with(|slot| slot.replace(Some(preview_scroll)));
    PREVIEW_IMAGE.with(|slot| slot.replace(Some(preview_image)));
    PILLS.with(|slot| slot.replace(Some(pills)));
    OLDER.with(|slot| slot.replace(Some(older)));
    NEWER.with(|slot| slot.replace(Some(newer)));
    MORE.with(|slot| slot.replace(Some(more)));
    CLEAR.with(|slot| slot.replace(Some(clear)));
    CLOSE.with(|slot| slot.replace(Some(close)));
    COPY_BUTTON.with(|slot| slot.replace(Some(copy_button)));
    SAVE_BUTTON.with(|slot| slot.replace(Some(save_button)));
    REVEAL.with(|slot| slot.replace(Some(reveal)));
    WINDOW.with(|slot| slot.replace(Some(window)));
}

fn layout(fresh_place: bool) {
    let searching = SEARCHING.with(Cell::get);
    let query = if searching {
        current_query()
    } else {
        zeroize::Zeroizing::new(String::new())
    };
    let shown = if searching && !query.trim().is_empty() {
        POOL.with(|slot| commands::matching(&slot.borrow(), query.as_str()))
    } else {
        ACTIONS.with(|slot| slot.borrow().clone())
    };
    let meta = resolved_meta();
    let labels: Vec<String> = shown.iter().map(|cmd| cmd.title.clone()).collect();
    let titles: Vec<&str> = labels.iter().map(String::as_str).collect();
    let inner = WIDTH - PAD * 2.0;
    let nav = HISTORY_NAV.with(|slot| *slot.borrow());
    let reserve = if nav.is_some() {
        commands::NAV_RESERVE
    } else {
        0.0
    };
    let frames = if shown.is_empty() {
        Vec::new()
    } else {
        commands::layout_chips(&titles, inner, reserve)
    };
    let show_empty = shown.is_empty() && searching && !query.trim().is_empty();
    let item_find = !well_is_masked();
    let placed = place_sections(
        &meta,
        searching,
        &frames,
        show_empty,
        nav.is_some(),
        item_find,
    );
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    place_window(mtm, placed.height, fresh_place);
    let title = CARD_TITLE.with(|slot| format!("{} · {HOTKEY}", slot.borrow()));
    let close_x = WIDTH - PAD - 24.0;
    let more_x = close_x - 6.0 - HEADER_BUTTON;
    let clear_x = more_x - 6.0 - CLEAR_BUTTON_W;
    let title_w = (clear_x - PAD - 8.0).max(40.0);
    HEADER.with(|slot| {
        set_label(slot, PAD, placed.header_y, title_w, HEADER_H, &title);
    });
    CLEAR.with(|slot| place_header_button(slot, clear_x, placed.header_y, CLEAR_BUTTON_W));
    MORE.with(|slot| place_header_button(slot, more_x, placed.header_y, HEADER_BUTTON));
    CLOSE.with(|slot| place_header_button(slot, close_x, placed.header_y, HEADER_BUTTON));
    place_item_find(placed.find_y, item_find);
    place_well(placed.preview_y);
    apply_preview(placed.preview_y);
    place_content_actions(placed.preview_y);
    META.with(|slot| {
        let borrowed = slot.borrow();
        let Some(label) = borrowed.as_ref() else {
            return;
        };
        label.setFrame(NSRect::new(
            NSPoint::new(PAD + 2.0, placed.meta_y),
            NSSize::new(inner - 4.0, META_H),
        ));
        label.setHidden(meta.is_empty());
        if !meta.is_empty() {
            paint_warning_meta(label, &meta);
        }
    });
    FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setHidden(false);
            let (field_y, field_h, alpha) = if searching {
                (placed.search_y + 4.0, 28.0, 1.0)
            } else {
                (0.0, 0.0, 0.0)
            };
            field.setFrame(NSRect::new(
                NSPoint::new(PAD, field_y),
                NSSize::new(inner, field_h),
            ));
            field.setAlphaValue(alpha);
        }
    });
    PILLS.with(|slot| {
        if let Some(pills) = slot.borrow().as_ref() {
            pills.setFrame(NSRect::new(
                NSPoint::new(PAD, placed.chips_y),
                NSSize::new(inner, placed.chips_h),
            ));
            rebuild_pills(
                mtm,
                pills,
                &shown,
                &frames,
                show_empty,
                (inner - reserve).max(0.0),
            );
        }
    });
    let nav_y = placed.chips_y + placed.chips_h - commands::CHIP_PITCH
        + (commands::CHIP_PITCH - commands::CHIP_PILL_H) / 2.0;
    place_history_nav(nav_y, nav);
    let selected = SELECTION.with(Cell::get);
    if shown.is_empty() {
        SELECTION.set(0);
    } else if selected >= shown.len() {
        SELECTION.set(shown.len() - 1);
    }
    SHOWN.with(|slot| set_commands(slot, shown));
    FRAMES.with(|slot| slot.replace(frames));
    paint_pills();
    warm_around();
}

struct Sections {
    height: f64,
    header_y: f64,
    find_y: f64,
    preview_y: f64,
    meta_y: f64,
    search_y: f64,
    chips_y: f64,
    chips_h: f64,
}

fn place_sections(
    meta: &str,
    searching: bool,
    frames: &[ChipFrame],
    show_empty: bool,
    show_nav: bool,
    item_find: bool,
) -> Sections {
    let meta_h = if meta.is_empty() { 0.0 } else { META_H };
    let search_h = if searching { SEARCH_H } else { 0.0 };
    let find_h = if item_find { ITEM_FIND_H } else { 0.0 };
    let gap_after_find = if find_h > 0.0 { 6.0 } else { 0.0 };
    let mut chips_h = if show_empty {
        28.0
    } else {
        commands::chips_height(frames)
    };
    if show_nav {
        chips_h = chips_h.max(commands::CHIP_PITCH);
    }
    let gap_after_preview = if meta_h > 0.0 || search_h > 0.0 || chips_h > 0.0 {
        GAP
    } else {
        0.0
    };
    let gap_after_meta = if meta_h > 0.0 && (search_h > 0.0 || chips_h > 0.0) {
        GAP
    } else {
        0.0
    };
    let gap_after_search = if search_h > 0.0 && chips_h > 0.0 {
        4.0
    } else {
        0.0
    };
    let height = PAD
        + HEADER_H
        + GAP
        + find_h
        + gap_after_find
        + PREVIEW_H
        + gap_after_preview
        + meta_h
        + gap_after_meta
        + search_h
        + gap_after_search
        + chips_h
        + PAD;
    let mut cursor = height - PAD;
    cursor -= HEADER_H;
    let header_y = cursor;
    cursor -= GAP + find_h;
    let find_y = cursor;
    cursor -= gap_after_find + PREVIEW_H;
    let preview_y = cursor;
    cursor -= gap_after_preview + meta_h;
    let meta_y = cursor;
    cursor -= gap_after_meta + search_h;
    let search_y = cursor;
    cursor -= gap_after_search + chips_h;
    let chips_y = cursor;
    Sections {
        height,
        header_y,
        find_y,
        preview_y,
        meta_y,
        search_y,
        chips_y,
        chips_h,
    }
}

fn place_window(mtm: MainThreadMarker, height: f64, fresh: bool) {
    WINDOW.with(|slot| {
        let borrowed = slot.borrow();
        let Some(window) = borrowed.as_ref() else {
            return;
        };
        let frame = if fresh {
            cursor_frame(mtm, height)
        } else {
            let current = window.frame();
            let top = current.origin.y + current.size.height;
            NSRect::new(
                NSPoint::new(current.origin.x, top - height),
                NSSize::new(WIDTH, height),
            )
        };
        window.setFrame_display(frame, true);
    });
}

fn cursor_frame(mtm: MainThreadMarker, height: f64) -> NSRect {
    let cursor = NSEvent::mouseLocation();
    let visible = screen_for_point(mtm, cursor)
        .map(|screen| screen.visibleFrame())
        .unwrap_or_else(|| NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0)));
    let min_x = visible.origin.x + 8.0;
    let max_x = visible.origin.x + visible.size.width - WIDTH - 8.0;
    let x = clamp_axis(cursor.x - 36.0, min_x, max_x);
    let top = visible.origin.y + visible.size.height - 8.0;
    let mut y = cursor.y + 12.0;
    if y + height > top {
        y = cursor.y - 12.0 - height;
    }
    y = clamp_axis(
        y,
        visible.origin.y + 8.0,
        (top - height).max(visible.origin.y),
    );
    NSRect::new(NSPoint::new(x, y), NSSize::new(WIDTH, height))
}

fn screen_for_point(mtm: MainThreadMarker, point: NSPoint) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    for screen in screens.iter() {
        let frame = screen.frame();
        let inside = point.x >= frame.origin.x
            && point.x < frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y < frame.origin.y + frame.size.height;
        if inside {
            return Some(screen);
        }
    }
    NSScreen::mainScreen(mtm)
}

fn clamp_axis(value: f64, min: f64, max: f64) -> f64 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

fn place_well(y: f64) {
    WELL.with(|slot| {
        if let Some(well) = slot.borrow().as_ref() {
            well.setFrame(NSRect::new(
                NSPoint::new(PAD, y),
                NSSize::new(WIDTH - PAD * 2.0, PREVIEW_H),
            ));
        }
    });
}

struct RevealCover {
    root: Retained<NSView>,
    /// Opaque stand-in when the gaussian blur cannot be installed.
    shade: Retained<NSBox>,
    hit: Retained<NSButton>,
}

fn reveal_cover(mtm: MainThreadMarker) -> RevealCover {
    let width = WIDTH - PAD * 2.0;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, PREVIEW_H));
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let shade = filled_box(mtm, 10.0, &NSColor::controlBackgroundColor());
    shade.setFrame(frame);
    shade.setHidden(true);
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setTransparent(true);
    hit.setTitle(&NSString::from_str(""));
    hit.setFocusRingType(NSFocusRingType::None);
    wire_button(&hit, sel!(revealClicked:));
    root.addSubview(&shade);
    root.addSubview(&hit);
    root.setHidden(true);
    RevealCover { root, shade, hit }
}

fn place_reveal_cover(y: f64) {
    let width = WIDTH - PAD * 2.0;
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.root.setFrame(NSRect::new(
            NSPoint::new(PAD, y),
            NSSize::new(width, PREVIEW_H),
        ));
        cover.hit.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, PREVIEW_H),
        ));
        cover.shade.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width, PREVIEW_H),
        ));
    });
}

fn show_mask_shade(shown: bool) {
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.shade.setHidden(!shown);
    });
}

fn show_reveal_cover(shown: bool) {
    REVEAL.with(|slot| {
        let borrowed = slot.borrow();
        let Some(cover) = borrowed.as_ref() else {
            return;
        };
        cover.root.setHidden(!shown);
        if shown {
            raise_view(&cover.root);
        }
    });
}

fn well_is_masked() -> bool {
    MASKS.with(Cell::get) && !REVEALED.with(Cell::get)
}

fn gaussian_blur() -> Option<Retained<AnyObject>> {
    #[cfg(test)]
    if FAIL_BLUR.with(Cell::get) {
        return None;
    }
    let _linked = unsafe { kCIInputRadiusKey };
    let cls = AnyClass::get(c"CIFilter")?;
    let name = NSString::from_str("CIGaussianBlur");
    let filter = unsafe {
        let ptr: *mut AnyObject = msg_send![cls, filterWithName: &*name];
        Retained::retain_autoreleased(ptr)
    }?;
    let _: () = unsafe { msg_send![&*filter, setDefaults] };
    let number_cls = AnyClass::get(c"NSNumber")?;
    let radius = unsafe {
        let ptr: *mut AnyObject = msg_send![number_cls, numberWithDouble: BLUR_RADIUS];
        Retained::retain_autoreleased(ptr)
    }?;
    let key = unsafe { kCIInputRadiusKey };
    if key.is_null() {
        return None;
    }
    let key = unsafe { &*key };
    let _: () = unsafe { msg_send![&*filter, setValue: &*radius, forKey: key] };
    Some(filter)
}

fn set_well_blur(on: bool) {
    // The cover and the find bar follow the same mask. Revealed text can be searched.
    set_item_find(!on);
    if !on {
        SHADE_ON.set(false);
        show_mask_shade(false);
        clear_blur();
        return;
    }
    let filter = gaussian_blur();
    let plan = commands::well_mask(true, filter.is_some());
    SHADE_ON.set(plan.shade);
    show_mask_shade(plan.shade);
    if let Some(filter) = filter {
        if !BLUR_ON.get() {
            blur_both(&NSArray::from_slice(&[&*filter]));
            BLUR_ON.set(true);
        }
        return;
    }
    // The filter is missing. Leave the well blank under an opaque cover.
    clear_blur();
    if !plan.show_body {
        blank_masked_well();
    }
}

fn clear_blur() {
    if !BLUR_ON.get() {
        return;
    }
    blur_both(&NSArray::from_slice(&[]));
    BLUR_ON.set(false);
}

fn blur_both(filters: &NSArray<AnyObject>) {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            blur_view(view, filters);
        }
    });
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            blur_view(view, filters);
        }
    });
}

fn blank_masked_well() {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setString(&NSString::from_str(""));
            view.setSelectable(false);
        }
    });
    set_preview_image_hidden(true);
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
}

fn blur_view(view: &NSView, filters: &NSArray<AnyObject>) {
    view.setWantsLayer(true);
    view.setLayerUsesCoreImageFilters(true);
    let _: () = unsafe { msg_send![view, setContentFilters: filters] };
}

struct CachedLink {
    image: Option<Retained<NSImage>>,
    caption: Option<String>,
    failed: bool,
}

const LINK_CACHE_MAX: usize = 20;

fn resolved_meta() -> String {
    let page = LINK_PAGE.with(|slot| slot.borrow().clone());
    let Some(page) = page else {
        return card_meta();
    };
    if let Some(caption) = cached_caption(&page)
        && !caption.is_empty()
    {
        return caption;
    }
    if cached_failed(&page) {
        return card_meta();
    }
    // Keep the title already on screen while the next preview is still loading.
    if cached_image(&page).is_some() || preview_holding_picture() {
        let held = current_meta_label();
        if !held.is_empty() {
            return held;
        }
    }
    card_meta()
}

fn card_meta() -> String {
    CARD_META.with(|slot| slot.borrow().clone())
}

fn current_meta_label() -> String {
    META.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|label| label.stringValue().to_string())
            .unwrap_or_default()
    })
}

fn apply_preview(y: f64) {
    if !well_is_masked() {
        SHADE_ON.set(false);
        show_mask_shade(false);
    }
    let image_frame = NSRect::new(
        NSPoint::new(PAD + 8.0, y + 8.0),
        NSSize::new(WIDTH - PAD * 2.0 - 16.0, PREVIEW_H - 16.0),
    );
    let text_frame = NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, PREVIEW_H),
    );
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setFrame(image_frame);
        }
    });
    PREVIEW_SCROLL.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setFrame(text_frame);
        }
    });
    place_reveal_cover(y);
    if SHOWS_IMAGE.with(Cell::get) {
        clear_preview_text();
        set_preview_text_hidden(true);
        show_preview_image();
        load_thumbnail();
        finish_preview();
        return;
    }
    THUMB_TOKEN.set(-1);
    let page = LINK_PAGE.with(|slot| slot.borrow().clone());
    if let Some(page) = page.as_deref() {
        // Hiding the scroller used to leave the previous item's string in place.
        clear_preview_text();
        if blocks_link_fetch(page) {
            set_preview_image_hidden(true);
            paint_preview_text("No preview", false);
            finish_preview();
            return;
        }
        if let Some(image) = cached_image(page) {
            set_preview_image(&image);
            show_preview_image();
            set_preview_text_hidden(true);
            finish_preview();
            return;
        }
        if cached_failed(page) {
            set_preview_image_hidden(true);
            paint_preview_text("No preview", false);
            finish_preview();
            return;
        }
        // Painting the address here is the flash between history steps.
        if preview_holding_picture() {
            show_preview_image();
            set_preview_text_hidden(true);
        } else {
            set_preview_image_hidden(true);
            set_preview_text_hidden(true);
        }
        finish_preview();
        return;
    }
    set_preview_image_hidden(true);
    let excerpt = CARD_EXCERPT.with(|slot| slot.borrow().clone());
    let placeholder = CARD_PLACEHOLDER.with(|slot| slot.borrow().clone());
    let payload = !excerpt.is_empty();
    let body = if !excerpt.is_empty() {
        excerpt
    } else {
        placeholder
    };
    paint_preview_text(&body, payload);
    finish_preview();
}

fn finish_preview() {
    let masked = well_is_masked();
    set_well_blur(masked);
    if masked {
        PREVIEW_TEXT.with(|slot| {
            if let Some(view) = slot.borrow().as_ref() {
                view.setSelectable(false);
            }
        });
    }
    show_reveal_cover(masked);
    refresh_item_marks(false);
}

fn item_find_on() -> bool {
    FIND_ON.get()
}

fn set_item_find(on: bool) {
    if !on {
        clear_item_query();
    }
    FIND_ON.set(on);
    apply_item_find_visible(on);
}

fn apply_item_find_visible(on: bool) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setHidden(!on);
        }
    });
    ITEM_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(!on);
        }
    });
}

fn focus_item_field() {
    if !item_find_on() {
        return;
    }
    let field = ITEM_FIELD.with(|slot| slot.borrow().clone());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        let _ = window.makeFirstResponder(Some(&*field));
    }
}

fn is_item_find_chord(event: &NSEvent) -> bool {
    let flags = event.modifierFlags();
    if !flags.contains(NSEventModifierFlags::Command)
        || flags.contains(NSEventModifierFlags::Shift)
        || flags.contains(NSEventModifierFlags::Control)
        || flags.contains(NSEventModifierFlags::Option)
    {
        return false;
    }
    event
        .charactersIgnoringModifiers()
        .is_some_and(|text| text.to_string().eq_ignore_ascii_case("f"))
}

fn note_is_item_field(note: &NSNotification) -> bool {
    let Some(object) = note.object() else {
        return false;
    };
    ITEM_FIELD.with(|slot| {
        slot.borrow().as_ref().is_some_and(|field| {
            std::ptr::eq(
                field.as_ref() as *const NSSearchField as *const AnyObject,
                &*object as *const AnyObject,
            )
        })
    })
}

fn control_is_item_field(control: &NSControl) -> bool {
    ITEM_FIELD.with(|slot| {
        slot.borrow().as_ref().is_some_and(|field| {
            std::ptr::eq(
                field.as_ref() as *const NSSearchField as *const NSControl,
                control as *const NSControl,
            )
        })
    })
}

fn item_field_command(command: Sel) -> bool {
    if command == sel!(insertNewline:) || command == sel!(insertNewlineIgnoringFieldEditor:) {
        step_item_match(!event_shift());
        true
    } else if command == sel!(cancelOperation:) {
        if item_query().is_empty() {
            if SEARCHING.with(Cell::get) {
                close_search();
            } else {
                hide();
            }
        } else {
            clear_item_query();
        }
        true
    } else {
        false
    }
}

fn event_shift() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = mac_ui::objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.currentEvent()
        .is_some_and(|event| event.modifierFlags().contains(NSEventModifierFlags::Shift))
}

/// Case-insensitive, non-overlapping spans in `text`. Indexes are bytes.
fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if chars.len() < needle.len() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut index = 0;
    while index + needle.len() <= chars.len() {
        let hit = needle
            .iter()
            .enumerate()
            .all(|(offset, expected)| chars_equal_ignore_case(chars[index + offset].1, *expected));
        if hit {
            let start = chars[index].0;
            let end_index = index + needle.len();
            let end = chars
                .get(end_index)
                .map(|(byte, _)| *byte)
                .unwrap_or(text.len());
            matches.push(start..end);
            index = end_index;
        } else {
            index += 1;
        }
    }
    matches
}

fn chars_equal_ignore_case(left: char, right: char) -> bool {
    if left == right || left.eq_ignore_ascii_case(&right) {
        return true;
    }
    let mut left_chars = left.to_lowercase();
    let mut right_chars = right.to_lowercase();
    loop {
        match (left_chars.next(), right_chars.next()) {
            (Some(a), Some(b)) if a == b => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

/// `index` is zero-based. `3/12` is the third match of twelve.
fn match_counter(index: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", index + 1, total)
    }
}

fn match_label(query: &str, index: usize, total: usize) -> String {
    if query.is_empty() {
        String::new()
    } else {
        match_counter(index, total)
    }
}

fn step_match(index: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (index + 1) % total
    } else {
        (index + total - 1) % total
    }
}

fn search_body(shows_image: bool, linked: bool, excerpt: &str, placeholder: &str) -> String {
    if shows_image || linked {
        String::new()
    } else if !excerpt.is_empty() {
        excerpt.to_string()
    } else {
        placeholder.to_string()
    }
}

fn publish_search_text() {
    let shows_image = SHOWS_IMAGE.with(Cell::get);
    let linked = LINK_PAGE.with(|slot| slot.borrow().is_some());
    let mut excerpt = CARD_EXCERPT.with(|slot| slot.borrow().clone());
    let mut placeholder = CARD_PLACEHOLDER.with(|slot| slot.borrow().clone());
    let text = search_body(shows_image, linked, &excerpt, &placeholder);
    excerpt.zeroize();
    placeholder.zeroize();
    set_item_text(text);
}

fn set_item_text(next: String) {
    ITEM_TEXT.with(|slot| {
        let mut text = slot.borrow_mut();
        text.zeroize();
        *text = next;
    });
}

fn current_item_text() -> String {
    ITEM_TEXT.with(|slot| slot.borrow().clone())
}

fn item_query() -> String {
    ITEM_QUERY.with(|slot| slot.borrow().clone())
}

fn set_item_query(text: &str) {
    ITEM_QUERY.with(|slot| {
        let mut query = slot.borrow_mut();
        query.zeroize();
        query.push_str(text);
    });
    FIND_INDEX.set(0);
    if text.is_empty() {
        wipe_item_field();
    } else if MainThreadMarker::new().is_some() {
        ITEM_FIELD.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                field.setStringValue(&NSString::from_str(text));
            }
        });
    }
    refresh_item_marks(true);
}

fn clear_item_query() {
    set_item_query("");
}

fn take_item_query_from_field() {
    let mut next = ITEM_FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| field.stringValue().to_string())
            .unwrap_or_default()
    });
    ITEM_QUERY.with(|slot| {
        let mut query = slot.borrow_mut();
        query.zeroize();
        query.push_str(&next);
    });
    FIND_INDEX.set(0);
    next.zeroize();
    refresh_item_marks(true);
}

fn step_item_match(forward: bool) {
    let mut text = current_item_text();
    let mut query = item_query();
    let total = find_matches(&text, &query).len();
    text.zeroize();
    query.zeroize();
    if total == 0 {
        return;
    }
    let index = FIND_INDEX.get().min(total - 1);
    FIND_INDEX.set(step_match(index, total, forward));
    refresh_item_marks(true);
}

fn refresh_item_marks(scroll: bool) {
    let mut text = current_item_text();
    let mut query = item_query();
    let matches = find_matches(&text, &query);
    let total = matches.len();
    if total == 0 || FIND_INDEX.get() >= total {
        FIND_INDEX.set(0);
    }
    let index = FIND_INDEX.get();
    let label = match_label(&query, index, total);
    if MainThreadMarker::new().is_some() {
        paint_match_marks(&text, &matches, index, scroll && total > 0);
        set_match_label(&label);
    }
    text.zeroize();
    query.zeroize();
}

fn set_match_label(label: &str) {
    ITEM_COUNT.with(|slot| {
        if let Some(count) = slot.borrow().as_ref() {
            count.setStringValue(&NSString::from_str(label));
        }
    });
}

fn paint_match_marks(text: &str, matches: &[Range<usize>], current: usize, scroll: bool) {
    let view = PREVIEW_TEXT.with(|slot| slot.borrow().clone());
    let Some(view) = view else {
        return;
    };
    let shown = view.string();
    let shown_text = shown.to_string();
    let Some(manager) = (unsafe { view.layoutManager() }) else {
        return;
    };
    if !shown_text.is_empty() {
        unsafe {
            manager.removeTemporaryAttribute_forCharacterRange(
                NSBackgroundColorAttributeName,
                NSRange {
                    location: 0,
                    length: shown.length(),
                },
            );
        }
    }
    if shown_text != text || matches.is_empty() {
        return;
    }
    let wash = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.92, 0.35, 0.55);
    let hot = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.62, 0.05, 0.9);
    for (index, range) in matches.iter().enumerate() {
        let color = if index == current { &hot } else { &wash };
        unsafe {
            manager.addTemporaryAttribute_value_forCharacterRange(
                NSBackgroundColorAttributeName,
                color,
                utf16_range(text, range),
            );
        }
    }
    if scroll && let Some(range) = matches.get(current) {
        view.scrollRangeToVisible(utf16_range(text, range));
    }
}

fn utf16_range(text: &str, range: &Range<usize>) -> NSRange {
    NSRange {
        location: text[..range.start].encode_utf16().count(),
        length: text[range.start..range.end].encode_utf16().count(),
    }
}

fn wipe_item_field() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ITEM_FIELD.with(|slot| {
        let borrowed = slot.borrow();
        let Some(field) = borrowed.as_ref() else {
            return;
        };
        if let Some(editor) = field.currentEditor() {
            let current = editor.string();
            let length = current.length();
            if length > 0 {
                let zeros = "\0".repeat(length);
                editor.setString(&NSString::from_str(&zeros));
            }
            editor.setString(&NSString::from_str(""));
        }
        wipe_field(field);
    });
}

fn clear_preview_text() {
    set_item_text(String::new());
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            zero_text_view(view);
        }
    });
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
}

fn zero_text_view(view: &NSTextView) {
    if let Some(storage) = unsafe { view.textStorage() } {
        let length = storage.length();
        if length > 0 {
            let zeros = "\0".repeat(length);
            storage.replaceCharactersInRange_withString(
                NSRange {
                    location: 0,
                    length,
                },
                &NSString::from_str(&zeros),
            );
        }
    }
    view.setString(&NSString::from_str(""));
}

fn preview_holding_picture() -> bool {
    PREVIEW_IMAGE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|view| !view.isHidden() && view.image().is_some())
    })
}

fn set_preview_image_hidden(hidden: bool) {
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setHidden(hidden);
        }
    });
}

fn set_preview_text_hidden(hidden: bool) {
    PREVIEW_SCROLL.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setHidden(hidden);
            // A layer-backed scroll view can keep painting after setHidden.
            view.setAlphaValue(if hidden { 0.0 } else { 1.0 });
            if let Some(parent) = unsafe { view.superview() } {
                if hidden {
                    // Keep the text scroller behind the well so it cannot cover the picture.
                    parent.addSubview_positioned_relativeTo(
                        view,
                        NSWindowOrderingMode::Below,
                        None,
                    );
                } else {
                    parent.addSubview(view);
                    raise_content_actions();
                }
            }
        }
    });
}

fn show_preview_image() {
    if SHADE_ON.with(Cell::get) {
        set_preview_image_hidden(true);
        return;
    }
    let image = PREVIEW_IMAGE.with(|slot| slot.borrow().clone());
    let Some(view) = image else {
        return;
    };
    view.setHidden(false);
    view.setAlphaValue(1.0);
    if let Some(parent) = unsafe { view.superview() } {
        parent.addSubview(&view);
    }
    raise_content_actions();
}

fn set_preview_image(image: &NSImage) {
    image.setTemplate(false);
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setImage(Some(image));
            view.setNeedsDisplayInRect(view.bounds());
        }
    });
}

fn preview_wrap_width() -> f64 {
    PREVIEW_SCROLL.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| view.contentSize().width)
            .filter(|width| *width > 1.0)
            .unwrap_or(WIDTH - PAD * 2.0)
    })
}

fn paint_preview_text(body: &str, payload: bool) {
    set_item_text(body.to_string());
    let highlight = CARD_HIGHLIGHT.with(|slot| *slot.borrow());
    let selectable = CARD_SELECTABLE.with(Cell::get);
    set_preview_text_hidden(false);
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setSelectable(selectable);
        }
    });
    let unchanged = PAINTED.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|(text, kind, was_payload)| {
                text == body && *kind == highlight && *was_payload == payload
            })
    });
    if unchanged {
        return;
    }
    PREVIEW_TEXT.with(|slot| {
        let borrowed = slot.borrow();
        let Some(view) = borrowed.as_ref() else {
            return;
        };
        crate::macos_card_text::paint(view, body, highlight, payload);
        let wrap = (highlight.is_none() || highlight == Some(FormatKind::Markdown))
            .then(preview_wrap_width);
        crate::macos_card_text::fit_document(view, wrap);
        view.scrollRangeToVisible(NSRange {
            location: 0,
            length: 0,
        });
    });
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        slot.replace(Some((body.to_string(), highlight, payload)));
    });
}

fn cached_image(page: &str) -> Option<Retained<NSImage>> {
    LINK_CACHE.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(key, _)| key == page)
            .and_then(|(_, entry)| entry.image.clone())
    })
}

fn cached_caption(page: &str) -> Option<String> {
    LINK_CACHE.with(|slot| {
        slot.borrow()
            .iter()
            .find(|(key, _)| key == page)
            .and_then(|(_, entry)| entry.caption.clone())
    })
}

fn cached_failed(page: &str) -> bool {
    LINK_CACHE.with(|slot| {
        slot.borrow()
            .iter()
            .any(|(key, entry)| key == page && entry.failed)
    })
}

fn link_settled(page: &str) -> bool {
    LINK_CACHE.with(|slot| {
        slot.borrow()
            .iter()
            .any(|(key, entry)| key == page && (entry.image.is_some() || entry.failed))
    })
}

fn link_in_flight(page: &str) -> bool {
    LINK_IN_FLIGHT.with(|slot| slot.borrow().iter().any(|item| item == page))
}

fn update_link_cache(page: &str, edit: impl FnOnce(&mut CachedLink)) {
    LINK_CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let mut entry = if let Some(index) = cache.iter().position(|(key, _)| key == page) {
            cache.remove(index)
        } else {
            (
                page.to_string(),
                CachedLink {
                    image: None,
                    caption: None,
                    failed: false,
                },
            )
        };
        edit(&mut entry.1);
        cache.insert(0, entry);
        while cache.len() > LINK_CACHE_MAX {
            if let Some(mut dropped) = cache.pop() {
                wipe_link_pair(&mut dropped);
            }
        }
    });
}

fn store_link_image(page: &str, bytes: &[u8]) {
    let image = nsimage_from_bytes(bytes);
    let failed = image.is_none();
    update_link_cache(page, |entry| {
        entry.image = image;
        entry.failed = failed;
    });
}

fn store_link_caption(page: &str, caption: Option<String>) {
    let Some(caption) = caption.filter(|text| !text.is_empty()) else {
        return;
    };
    update_link_cache(page, |entry| {
        entry.caption = Some(caption);
    });
}

fn end_link_fetch(page: &str) {
    LINK_IN_FLIGHT.with(|slot| slot.borrow_mut().retain(|item| item != page));
}

fn reveal_link(page: &str) {
    let current = LINK_PAGE.with(|slot| slot.borrow().clone());
    if current.as_deref() == Some(page) && OPEN.with(Cell::get) {
        layout(false);
    }
}

fn show_link_caption(page: &str) {
    let current = LINK_PAGE.with(|slot| slot.borrow().clone());
    if current.as_deref() != Some(page) || !OPEN.with(Cell::get) {
        return;
    }
    let Some(caption) = cached_caption(page) else {
        return;
    };
    let painted = META.with(|slot| {
        let borrowed = slot.borrow();
        let Some(label) = borrowed.as_ref() else {
            return false;
        };
        if label.isHidden() {
            return false;
        }
        label.setStringValue(&NSString::from_str(&caption));
        true
    });
    if !painted {
        layout(false);
    }
}

fn warm_around() {
    let mut pages = WARM_LINKS.with(|slot| slot.borrow().clone());
    if let Some(page) = LINK_PAGE.with(|slot| slot.borrow().clone())
        && !pages.iter().any(|existing| existing == &page)
    {
        pages.insert(0, page);
    }
    for page in pages {
        warm_link(page);
    }
}

fn blocks_link_fetch(page: &str) -> bool {
    if crate::page_preview::url_has_userinfo(page) {
        return true;
    }
    crate::page_preview::canonical_url(page)
        .is_some_and(|url| crate::page_preview::url_has_userinfo(&url))
}

fn warm_link(page: String) {
    if blocks_link_fetch(&page) || link_settled(&page) || link_in_flight(&page) {
        return;
    }
    LINK_IN_FLIGHT.with(|slot| slot.borrow_mut().push(page.clone()));
    std::thread::spawn(move || publish_link(page));
}

fn publish_link(page: String) {
    if blocks_link_fetch(&page) {
        dispatch2::DispatchQueue::main().exec_async(move || {
            end_link_fetch(&page);
        });
        return;
    }
    if crate::youtube::video_id(&page).is_some() {
        let bytes = mac_ui::objc2::rc::autoreleasepool(|_| youtube_thumb_bytes(&page));
        let image_page = page.clone();
        dispatch2::DispatchQueue::main().exec_async(move || {
            store_link_image(&image_page, &bytes);
            reveal_link(&image_page);
        });
        let caption = mac_ui::objc2::rc::autoreleasepool(|_| youtube_caption(&page));
        dispatch2::DispatchQueue::main().exec_async(move || {
            store_link_caption(&page, caption);
            end_link_fetch(&page);
            show_link_caption(&page);
        });
        return;
    }
    let (bytes, caption) = mac_ui::objc2::rc::autoreleasepool(|_| page_preview_parts(&page));
    dispatch2::DispatchQueue::main().exec_async(move || {
        store_link_caption(&page, caption);
        store_link_image(&page, &bytes);
        end_link_fetch(&page);
        reveal_link(&page);
    });
}

fn usable_thumbnail(bytes: &[u8]) -> bool {
    bytes.len() > 8_000 && (bytes.starts_with(b"\xFF\xD8") || bytes.starts_with(b"\x89PNG"))
}

fn youtube_thumb_bytes(page: &str) -> Vec<u8> {
    if blocks_link_fetch(page) {
        return Vec::new();
    }
    let Some(id) = crate::youtube::video_id(page) else {
        return Vec::new();
    };
    crate::macos_fetch::get(&crate::youtube::wide_thumbnail_url(id))
        .filter(|bytes| usable_thumbnail(bytes))
        .or_else(|| crate::macos_fetch::get(&crate::youtube::thumbnail_url(id)))
        .unwrap_or_default()
}

fn youtube_caption(page: &str) -> Option<String> {
    if blocks_link_fetch(page) {
        return None;
    }
    let watch = crate::format::format_text(page);
    crate::macos_fetch::get(&crate::youtube::oembed_endpoint(&watch))
        .and_then(|body| crate::youtube::caption_from_oembed(&body))
}

fn page_preview_parts(page: &str) -> (Vec<u8>, Option<String>) {
    if blocks_link_fetch(page) {
        return (Vec::new(), None);
    }
    let fetch_at = crate::page_preview::canonical_url(page).unwrap_or_else(|| page.to_string());
    if crate::page_preview::url_has_userinfo(&fetch_at) {
        return (Vec::new(), None);
    }
    let html = crate::macos_fetch::get_document(&fetch_at).unwrap_or_default();
    let text = String::from_utf8_lossy(&html);
    let found = crate::page_preview::from_html(&text, &fetch_at);
    let bytes = found
        .image
        .as_deref()
        .and_then(crate::macos_fetch::get_asset)
        .unwrap_or_default();
    (bytes, found.title)
}

fn load_thumbnail() {
    let change = crate::macos_pasteboard::change_count();
    let already = PREVIEW_IMAGE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|view| !view.isHidden() && view.image().is_some())
    });
    if THUMB_TOKEN.with(Cell::get) == change && already {
        return;
    }
    let Some(image) = crate::macos_pasteboard::clipboard_picture() else {
        return;
    };
    // A picture set before the window is visible sticks as an empty layer.
    if window_is_visible() {
        THUMB_TOKEN.set(change);
    }
    set_preview_image(&image);
}

fn rebuild_pills(
    mtm: MainThreadMarker,
    list: &NSView,
    shown: &[Command],
    frames: &[ChipFrame],
    show_empty: bool,
    text_width: f64,
) {
    while list.subviews().count() > 0 {
        list.subviews().objectAtIndex(0).removeFromSuperview();
    }
    if shown.is_empty() {
        if show_empty {
            let empty = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
            empty.setFrame(NSRect::new(
                NSPoint::new(2.0, 4.0),
                NSSize::new(text_width, 20.0),
            ));
            empty.setStringValue(&NSString::from_str("No matching commands"));
            list.addSubview(&empty);
        }
        return;
    }
    let area_h = commands::chips_height(frames);
    for (index, cmd) in shown.iter().enumerate() {
        let Some(frame) = frames.get(index) else {
            continue;
        };
        let y = area_h - (frame.row as f64 + 1.0) * commands::CHIP_PITCH
            + (commands::CHIP_PITCH - commands::CHIP_PILL_H) / 2.0;
        let pill = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(
                NSPoint::new(frame.x, y),
                NSSize::new(frame.width, commands::CHIP_PILL_H),
            ),
        );
        let fill = filled_box(mtm, commands::CHIP_PILL_H / 2.0, &NSColor::clearColor());
        fill.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(frame.width, commands::CHIP_PILL_H),
        ));
        let title = widgets::label(mtm, 13.0, &NSColor::labelColor());
        title.setAlignment(NSTextAlignment::Center);
        title.setFrame(NSRect::new(
            NSPoint::new(8.0, 5.0),
            NSSize::new((frame.width - 16.0).max(8.0), 18.0),
        ));
        title.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        title.setStringValue(&NSString::from_str(&cmd.title));
        let hit = NSButton::initWithFrame(
            NSButton::alloc(mtm),
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(frame.width, commands::CHIP_PILL_H),
            ),
        );
        hit.setBordered(false);
        hit.setTransparent(true);
        hit.setTitle(&NSString::from_str(""));
        hit.setTag(index as isize);
        wire_button(&hit, sel!(chipClicked:));
        pill.addSubview(&fill);
        pill.addSubview(&title);
        pill.addSubview(&hit);
        list.addSubview(&pill);
    }
}

fn paint_pills() {
    if SHOWN.with(|slot| slot.borrow().is_empty()) {
        return;
    }
    let selected = SELECTION.with(Cell::get);
    PILLS.with(|slot| {
        let borrowed = slot.borrow();
        let Some(list) = borrowed.as_ref() else {
            return;
        };
        for (index, pill) in list.subviews().iter().enumerate() {
            paint_pill(&pill, index == selected);
        }
    });
}

fn paint_pill(pill: &NSView, selected: bool) {
    let fill = if selected {
        NSColor::controlAccentColor()
    } else {
        NSColor::unemphasizedSelectedContentBackgroundColor()
    };
    let text = if selected {
        NSColor::whiteColor()
    } else {
        NSColor::labelColor()
    };
    for view in pill.subviews().iter() {
        if let Some(box_view) = view.downcast_ref::<NSBox>() {
            box_view.setFillColor(&fill);
        } else if let Some(label) = view.downcast_ref::<NSTextField>() {
            label.setTextColor(Some(&text));
        }
    }
}

fn nudge(dx: isize, dy: isize) {
    let frames = FRAMES.with(|slot| slot.borrow().clone());
    let next = commands::step_chip(&frames, SELECTION.with(Cell::get), dx, dy);
    if next == SELECTION.with(Cell::get) {
        return;
    }
    SELECTION.set(next);
    paint_pills();
}

fn activate_selected() {
    let cmd = SHOWN.with(|slot| slot.borrow().get(SELECTION.with(Cell::get)).cloned());
    let Some(cmd) = cmd else {
        return;
    };
    run_command(cmd);
}

fn overflow_item(mtm: MainThreadMarker, title: &str, index: usize) -> Retained<NSMenuItem> {
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

fn activate_overflow(index: usize) {
    let cmd = OVERFLOW.with(|slot| slot.borrow().get(index).cloned());
    let Some(cmd) = cmd else {
        return;
    };
    run_command(cmd);
}

fn run_command(cmd: Command) {
    let formatting_link =
        cmd.id == CommandId::Format && LINK_PAGE.with(|slot| slot.borrow().is_some());
    if !commands::keeps_card_open(&cmd.id) && !formatting_link {
        hide();
    }
    launcher::emit(UserEvent::Run(cmd.id));
}

fn pop_overflow() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let items = OVERFLOW.with(|slot| slot.borrow().clone());
    let theme = THEME.with(Cell::get);
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let mut saw_appearance = false;
    let mut saw_quit = false;
    let mut history_menu: Option<Retained<NSMenu>> = None;
    for (index, cmd) in items.iter().enumerate() {
        let in_history = matches!(cmd.id, CommandId::History(_) | CommandId::ClearHistory);
        if in_history {
            let submenu = history_menu.get_or_insert_with(|| {
                let parent = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str("History"),
                        None,
                        &NSString::from_str(""),
                    )
                };
                let submenu =
                    NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("History"));
                submenu.setAutoenablesItems(false);
                parent.setSubmenu(Some(&submenu));
                menu.addItem(&parent);
                submenu
            });
            let item = overflow_item(mtm, &cmd.title, index);
            submenu.addItem(&item);
            continue;
        }
        if !saw_appearance && matches!(cmd.id, CommandId::Appearance(_)) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_appearance = true;
        }
        if !saw_quit && matches!(cmd.id, CommandId::Quit) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_quit = true;
        }
        let item = overflow_item(mtm, &cmd.title, index);
        if let CommandId::Appearance(item_theme) = cmd.id {
            let state = if item_theme == theme {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            };
            item.setState(state);
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
    let button = MORE.with(|slot| slot.borrow().clone());
    let Some(button) = button else {
        return;
    };
    SUPPRESS_RESIGN.set(true);
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, 0.0), Some(&*button));
    SUPPRESS_RESIGN.set(false);
    focus_card();
}

fn on_key(event: &NSEvent) {
    if is_item_find_chord(event) {
        if item_find_on() {
            focus_item_field();
        }
        return;
    }
    match event.keyCode() {
        123 => nudge(-1, 0),
        124 => nudge(1, 0),
        126 => nudge(0, -1),
        125 => nudge(0, 1),
        36 | 76 => activate_selected(),
        53 => {
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

fn begin_search_from_key(event: &NSEvent) {
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

fn is_search_text(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| !ch.is_control())
}

fn open_search(initial: &str) {
    SEARCHING.set(true);
    set_query(initial);
    SELECTION.set(0);
    layout(false);
    focus_field();
}

fn close_search() {
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
    set_item_find(false);
    CARD_TITLE.with(wipe_string_slot);
    CARD_META.with(wipe_string_slot);
    CARD_EXCERPT.with(wipe_string_slot);
    CARD_PLACEHOLDER.with(wipe_string_slot);
    LINK_PAGE.with(|slot| set_secret_opt(slot, None));
    for slot in [&ACTIONS, &POOL, &OVERFLOW, &SHOWN] {
        slot.with(|slot| set_commands(slot, Vec::new()));
    }
    WARM_LINKS.with(|slot| {
        wipe_strings(&mut slot.borrow_mut());
        slot.borrow_mut().clear();
    });
    LINK_IN_FLIGHT.with(|slot| {
        wipe_strings(&mut slot.borrow_mut());
        slot.borrow_mut().clear();
    });
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
    LINK_CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        for entry in cache.iter_mut() {
            wipe_link_pair(entry);
        }
        cache.clear();
    });
    wipe_shown_views();
    SEARCHING.set(false);
}

fn wipe_string_slot(slot: &RefCell<String>) {
    slot.borrow_mut().zeroize();
}

fn set_secret(slot: &RefCell<String>, next: String) {
    wipe_string_slot(slot);
    *slot.borrow_mut() = next;
}

fn set_secret_opt(slot: &RefCell<Option<String>>, next: Option<String>) {
    if let Some(text) = slot.borrow_mut().as_mut() {
        text.zeroize();
    }
    *slot.borrow_mut() = next;
}

fn set_commands(slot: &RefCell<Vec<Command>>, next: Vec<Command>) {
    commands::wipe_commands(&mut slot.borrow_mut());
    slot.replace(next);
}

fn wipe_strings(texts: &mut [String]) {
    for text in texts.iter_mut() {
        text.zeroize();
    }
}

fn wipe_link_pair(entry: &mut (String, CachedLink)) {
    entry.0.zeroize();
    if let Some(caption) = entry.1.caption.as_mut() {
        caption.zeroize();
    }
    entry.1.image = None;
}

fn wipe_shown_views() {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            zero_text_view(view);
        }
    });
    set_item_text(String::new());
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setImage(None);
        }
    });
    THUMB_TOKEN.set(-1);
    for slot in [&HEADER, &META, &FIELD, &ITEM_COUNT] {
        slot.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                wipe_field(field);
            }
        });
    }
    wipe_item_field();
}

fn wipe_field(field: &NSTextField) {
    let current = field.stringValue();
    let length = current.length();
    if length > 0 {
        let zeros = "\0".repeat(length);
        field.setStringValue(&NSString::from_str(&zeros));
    }
    field.setStringValue(&NSString::from_str(""));
}

fn current_query() -> zeroize::Zeroizing<String> {
    FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| zeroize::Zeroizing::new(field.stringValue().to_string()))
            .unwrap_or_default()
    })
}

fn set_query(text: &str) {
    FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(text));
        }
    });
}

fn focus_card() {
    focus_field();
}

fn focus_field() {
    let field = FIELD.with(|slot| slot.borrow().clone());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        window.makeFirstResponder(Some(&field));
    }
}

/// Size and filename stay quiet. PII is a yellow caution; financial and
/// credential are a red alert.
fn paint_warning_meta(label: &NSTextField, meta: &str) {
    let ns = NSString::from_str(meta);
    let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
    let all = NSRange {
        location: 0,
        length: ns.length(),
    };
    let font = NSFont::systemFontOfSize(12.0);
    unsafe {
        attr.addAttribute_value_range(NSFontAttributeName, &font, all);
        attr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            &NSColor::secondaryLabelColor(),
            all,
        );
    }
    let warn_font = NSFont::boldSystemFontOfSize(12.0);
    for mark in crate::sensitivity::warning_marks(meta) {
        let location = meta[..mark.start].encode_utf16().count();
        let length = meta[mark.start..mark.end].encode_utf16().count();
        let range = NSRange { location, length };
        let (ink, wash) = warning_colors(mark.label);
        unsafe {
            attr.addAttribute_value_range(NSFontAttributeName, &warn_font, range);
            attr.addAttribute_value_range(NSForegroundColorAttributeName, &ink, range);
            attr.addAttribute_value_range(NSBackgroundColorAttributeName, &wash, range);
        }
    }
    label.setAttributedStringValue(&attr);
}

fn warning_colors(label: crate::sensitivity::Label) -> (Retained<NSColor>, Retained<NSColor>) {
    match label {
        crate::sensitivity::Label::Pii => (NSColor::blackColor(), NSColor::systemYellowColor()),
        crate::sensitivity::Label::Financial | crate::sensitivity::Label::Credential => {
            (NSColor::whiteColor(), NSColor::systemRedColor())
        }
    }
}

fn set_label(
    slot: &RefCell<Option<Retained<NSTextField>>>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    text: &str,
) {
    let borrowed = slot.borrow();
    let Some(label) = borrowed.as_ref() else {
        return;
    };
    label.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
    label.setStringValue(&NSString::from_str(text));
}

fn place_header_button(
    slot: &RefCell<Option<Retained<NSButton>>>,
    x: f64,
    header_y: f64,
    width: f64,
) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    let y = header_y + (HEADER_H - HEADER_BUTTON) / 2.0;
    button.setFrame(NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(width, HEADER_BUTTON),
    ));
}

fn search_field(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = widgets::plain_field(mtm, 16.0, "Search");
    field.setAlphaValue(0.0);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            unsafe {
                let _: () = msg_send![&*field, setDelegate: &**delegate];
            }
        }
    });
    field
}

fn item_search_field(mtm: MainThreadMarker) -> Retained<NSSearchField> {
    let field = widgets::search_field(mtm, 13.0, "Find");
    field.setHidden(true);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            unsafe {
                let _: () = msg_send![&*field, setDelegate: &**delegate];
            }
        }
    });
    field
}

fn place_item_find(y: f64, shown: bool) {
    let inner = WIDTH - PAD * 2.0;
    let field_w = (inner - ITEM_COUNT_W - 6.0).max(40.0);
    ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setHidden(!shown);
            field.setFrame(NSRect::new(
                NSPoint::new(PAD, y + 1.0),
                NSSize::new(field_w, ITEM_FIND_H - 2.0),
            ));
        }
    });
    ITEM_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(!shown);
            label.setFrame(NSRect::new(
                NSPoint::new(PAD + field_w + 6.0, y + (ITEM_FIND_H - 16.0) / 2.0),
                NSSize::new(ITEM_COUNT_W, 16.0),
            ));
        }
    });
}

fn payload_view(mtm: MainThreadMarker) -> Retained<NSTextView> {
    let text = NSTextView::initWithFrame(NSTextView::alloc(mtm), NSRect::ZERO);
    text.setEditable(false);
    text.setSelectable(false);
    text.setUsesFindPanel(false);
    text.setDrawsBackground(false);
    text.setRichText(true);
    text.setTextContainerInset(NSSize::new(12.0, 10.0));
    crate::macos_card_text::configure_scrolling(&text);
    text
}

fn text_scroll(mtm: MainThreadMarker, text: &NSTextView) -> Retained<PreviewScroll> {
    let allocated = PreviewScroll::alloc(mtm);
    let scroll: Retained<PreviewScroll> =
        unsafe { msg_send![allocated, initWithFrame: NSRect::ZERO] };
    scroll.setDrawsBackground(false);
    scroll.setBorderType(NSBorderType::NoBorder);
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setAutomaticallyAdjustsContentInsets(false);
    scroll.contentView().setDrawsBackground(false);
    scroll.setDocumentView(Some(text));
    scroll.setHidden(true);
    scroll.setAlphaValue(0.0);
    scroll
}

fn image_view(mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = widgets::image_view(mtm);
    view.setHidden(true);
    view
}

struct NavButton {
    root: Retained<NSView>,
    hit: Retained<NSButton>,
}

fn place_history_nav(y: f64, nav: Option<commands::HistoryNav>) {
    let newer_x = WIDTH - PAD - commands::NAV_BUTTON;
    let older_x = newer_x - commands::NAV_GAP - commands::NAV_BUTTON;
    let shown = nav.is_some();
    OLDER.with(|slot| {
        place_nav_button(
            slot,
            older_x,
            y,
            shown,
            nav.is_some_and(|nav| nav.can_older),
        );
    });
    NEWER.with(|slot| {
        place_nav_button(
            slot,
            newer_x,
            y,
            shown,
            nav.is_some_and(|nav| nav.can_newer),
        );
    });
}

fn place_nav_button(slot: &RefCell<Option<NavButton>>, x: f64, y: f64, shown: bool, enabled: bool) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    button.root.setHidden(!shown);
    button.root.setFrame(NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(commands::NAV_BUTTON, commands::CHIP_PILL_H),
    ));
    button.hit.setEnabled(enabled);
    button.root.setAlphaValue(if enabled { 1.0 } else { 0.35 });
    if shown {
        raise_view(&button.root);
    }
}

fn raise_history_nav() {
    OLDER.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            raise_view(&button.root);
        }
    });
    NEWER.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            raise_view(&button.root);
        }
    });
}

fn place_content_actions(preview_y: f64) {
    let actions = CONTENT_ACTIONS.get();
    let title = CARD_TITLE.with(|slot| slot.borrow().clone());
    let right = WIDTH - PAD - WELL_INSET - WELL_ACTION;
    let top = preview_y + PREVIEW_H - WELL_INSET - WELL_ACTION;
    let copy_x = if actions.save {
        right - WELL_GAP - WELL_ACTION
    } else {
        right
    };
    COPY_BUTTON.with(|slot| {
        place_well_action(slot, copy_x, top, actions.copy, &commands::copy_tip(&title));
    });
    SAVE_BUTTON.with(|slot| {
        place_well_action(slot, right, top, actions.save, &commands::save_tip(&title));
    });
    set_text_gutter(text_gutter(actions));
}

fn text_gutter(actions: commands::ContentActions) -> f64 {
    let count = usize::from(actions.copy) + usize::from(actions.save);
    if count == 0 {
        return 0.0;
    }
    let cluster = WELL_INSET + count as f64 * WELL_ACTION + (count - 1) as f64 * WELL_GAP + 6.0;
    (cluster - TEXT_INSET_X).max(0.0)
}

fn place_well_action(slot: &RefCell<Option<WellAction>>, x: f64, y: f64, shown: bool, tip: &str) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    button.root.setHidden(!shown);
    button.root.setFrame(NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(WELL_ACTION, WELL_ACTION),
    ));
    button.hit.setToolTip(Some(&NSString::from_str(tip)));
    if shown {
        raise_view(&button.root);
    }
}

fn raise_content_actions() {
    for slot in [&COPY_BUTTON, &SAVE_BUTTON] {
        slot.with(|slot| {
            if let Some(button) = slot.borrow().as_ref()
                && !button.root.isHidden()
            {
                raise_view(&button.root);
            }
        });
    }
}

fn set_text_gutter(right: f64) {
    if (TEXT_GUTTER.get() - right).abs() < 0.5 {
        return;
    }
    TEXT_GUTTER.set(right);
    PREVIEW_SCROLL.with(|slot| {
        if let Some(scroll) = slot.borrow().as_ref() {
            scroll.setContentInsets(NSEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom: 0.0,
                right,
            });
        }
    });
}

struct WellAction {
    root: Retained<NSView>,
    hit: Retained<NSButton>,
}

fn well_action(mtm: MainThreadMarker, symbol: &str, fallback: &str, action: Sel) -> WellAction {
    let frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(WELL_ACTION, WELL_ACTION),
    );
    let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
    let fill = filled_box(mtm, WELL_ACTION / 2.0, &NSColor::controlBackgroundColor());
    fill.setFrame(frame);
    fill.setAlphaValue(0.92);
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setFocusRingType(NSFocusRingType::None);
    hit.setImagePosition(NSCellImagePosition::ImageOnly);
    hit.setImageScaling(NSImageScaling::ScaleProportionallyDown);
    if let Some(image) = system_symbol(symbol, fallback) {
        image.setTemplate(true);
        image.setSize(NSSize::new(15.0, 15.0));
        hit.setImage(Some(&image));
        hit.setTitle(&NSString::from_str(""));
        if hit.respondsToSelector(sel!(setContentTintColor:)) {
            hit.setContentTintColor(Some(&NSColor::labelColor()));
        }
    } else {
        hit.setTitle(&NSString::from_str(fallback));
        hit.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    }
    wire_button(&hit, action);
    root.addSubview(&fill);
    root.addSubview(&hit);
    root.setHidden(true);
    WellAction { root, hit }
}

fn nav_button(mtm: MainThreadMarker, title: &str, action: Sel) -> NavButton {
    let root = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::NAV_BUTTON, commands::CHIP_PILL_H),
        ),
    );
    let fill = filled_box(
        mtm,
        commands::CHIP_PILL_H / 2.0,
        &NSColor::unemphasizedSelectedContentBackgroundColor(),
    );
    fill.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(commands::NAV_BUTTON, commands::CHIP_PILL_H),
    ));
    let label = widgets::label(mtm, 15.0, &NSColor::labelColor());
    label.setAlignment(NSTextAlignment::Center);
    label.setLineBreakMode(NSLineBreakMode::ByClipping);
    label.setFrame(NSRect::new(
        NSPoint::new(0.0, 5.0),
        NSSize::new(commands::NAV_BUTTON, 18.0),
    ));
    label.setStringValue(&NSString::from_str(title));
    let hit = NSButton::initWithFrame(
        NSButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::NAV_BUTTON, commands::CHIP_PILL_H),
        ),
    );
    hit.setBordered(false);
    hit.setTransparent(true);
    hit.setTitle(&NSString::from_str(""));
    hit.setFocusRingType(NSFocusRingType::None);
    wire_button(&hit, action);
    root.addSubview(&fill);
    root.addSubview(&label);
    root.addSubview(&hit);
    root.setHidden(true);
    NavButton { root, hit }
}

fn icon_button(mtm: MainThreadMarker, title: &str, action: Sel) -> Retained<NSButton> {
    let button = widgets::text_button(mtm, title, 14.0);
    wire_button(&button, action);
    button
}

fn wire_button(button: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            unsafe {
                button.setTarget(Some(delegate));
                button.setAction(Some(action));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroize;

    struct ForceBlurOff;

    impl ForceBlurOff {
        fn arm() -> Self {
            super::FAIL_BLUR.with(|cell| cell.set(true));
            Self
        }
    }

    impl Drop for ForceBlurOff {
        fn drop(&mut self) {
            super::FAIL_BLUR.with(|cell| cell.set(false));
        }
    }

    #[test]
    fn credential_link_is_not_fetched() {
        let url = "https://deploy:s3cr3t@github.com/acme/app.git";
        assert!(super::blocks_link_fetch(url));
        assert!(super::blocks_link_fetch("deploy:s3cr3t@github.com/acme"));
        assert!(super::blocks_link_fetch(
            "https://viewer:s3cr3t@www.youtube.com/watch?v=bEN9Dyg48b0"
        ));
        assert!(!super::blocks_link_fetch("https://github.com/acme/app"));
        assert!(!super::blocks_link_fetch("https://example.com/news/story"));
        assert!(!super::blocks_link_fetch(
            "https://www.youtube.com/watch?v=bEN9Dyg48b0"
        ));
        let (bytes, caption) = super::page_preview_parts(url);
        assert!(bytes.is_empty());
        assert!(caption.is_none());
    }

    #[test]
    fn blur_hook_fails_closed() {
        let _force = ForceBlurOff::arm();
        assert!(super::gaussian_blur().is_none());
        let masked = crate::commands::well_mask(true, false);
        assert!(masked.shade);
        assert!(!masked.show_body);
        assert!(!masked.blur);
        let revealed = crate::commands::well_mask(false, false);
        assert!(revealed.show_body);
        assert!(!revealed.shade);
    }

    #[test]
    fn failed_blur_covers_the_well_until_reveal() {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                super::MASKS.set(false);
                super::REVEALED.set(false);
                super::SHADE_ON.set(false);
                super::BLUR_ON.set(false);
            }
        }
        let _force = ForceBlurOff::arm();
        let _reset = Reset;
        super::MASKS.set(true);
        super::REVEALED.set(false);
        super::BLUR_ON.set(false);
        super::SHADE_ON.set(false);
        super::finish_preview();
        assert!(super::SHADE_ON.get());
        assert!(!super::BLUR_ON.get());
        super::REVEALED.set(true);
        super::finish_preview();
        assert!(!super::SHADE_ON.get());
        assert!(!super::BLUR_ON.get());
        super::REVEALED.set(false);
        super::finish_preview();
        assert!(super::SHADE_ON.get());
    }

    struct ResetFind;

    impl ResetFind {
        fn arm() -> Self {
            Self
        }
    }

    impl Drop for ResetFind {
        fn drop(&mut self) {
            super::MASKS.set(false);
            super::REVEALED.set(false);
            super::FIND_ON.set(false);
            super::FIND_INDEX.set(0);
            super::ITEM_QUERY.with(|slot| slot.borrow_mut().zeroize());
            super::ITEM_TEXT.with(|slot| slot.borrow_mut().zeroize());
            super::CONTENT_KEY.set(0);
            super::OPEN.set(false);
            super::SEARCHING.set(false);
            super::SHADE_ON.set(false);
            super::BLUR_ON.set(false);
        }
    }

    fn copied(text: &str) -> crate::commands::LaunchData {
        crate::commands::LaunchData {
            subject_kind: crate::commands::SubjectKind::Text,
            subject_text: Some(text.to_string()),
            image: None,
            history: Vec::new(),
            can_clear_history: false,
            history_nav: None,
            warm_links: Vec::new(),
            theme: crate::appearance::Theme::System,
            view: crate::commands::CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
        }
    }

    fn assert_find(on: bool) {
        assert_eq!(super::FIND_ON.get(), on);
        super::ITEM_FIELD.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                assert_eq!(field.isHidden(), !on);
            }
        });
    }

    fn assert_query_empty() {
        assert!(super::item_query().is_empty());
        super::ITEM_FIELD.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                assert!(field.stringValue().to_string().is_empty());
            }
        });
    }

    fn picture() -> crate::commands::LaunchData {
        crate::commands::LaunchData {
            subject_kind: crate::commands::SubjectKind::Image,
            subject_text: None,
            image: Some(crate::commands::ImageFacts {
                format: "png".to_string(),
                width: 4,
                height: 4,
                byte_len: 64,
            }),
            history: Vec::new(),
            can_clear_history: false,
            history_nav: None,
            warm_links: Vec::new(),
            theme: crate::appearance::Theme::System,
            view: crate::commands::CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
        }
    }

    #[test]
    fn find_stays_off_while_the_well_is_masked() {
        let _reset = ResetFind::arm();
        let _force = ForceBlurOff::arm();
        super::FIND_ON.set(true);
        super::MASKS.set(true);
        super::REVEALED.set(false);
        super::finish_preview();
        assert_find(false);
    }

    #[test]
    fn find_turns_on_after_reveal() {
        let _reset = ResetFind::arm();
        let _force = ForceBlurOff::arm();
        super::MASKS.set(true);
        super::REVEALED.set(false);
        super::finish_preview();
        super::FIND_ON.set(false);
        super::REVEALED.set(true);
        super::finish_preview();
        assert_find(true);
    }

    #[test]
    fn find_turns_off_after_mask_new_item_wipe_or_close() {
        let _reset = ResetFind::arm();
        let _force = ForceBlurOff::arm();
        let first = copied("correct horse battery staple");
        let second = copied("another copy");
        super::store(first.clone());
        super::REVEALED.set(true);
        super::finish_preview();
        assert_find(true);
        super::set_item_query("horse");

        super::store(first);
        assert!(super::REVEALED.get());
        super::finish_preview();
        assert_find(true);
        assert_eq!(super::item_query(), "horse");

        super::REVEALED.set(false);
        super::finish_preview();
        assert_find(false);
        assert_query_empty();

        super::REVEALED.set(true);
        super::finish_preview();
        super::set_item_query("horse");
        super::store(second);
        super::finish_preview();
        assert!(!super::REVEALED.get());
        assert_find(false);
        assert_query_empty();

        super::REVEALED.set(true);
        super::finish_preview();
        super::set_item_query("horse");
        super::wipe_shown();
        assert_find(false);
        assert_query_empty();

        super::REVEALED.set(true);
        super::finish_preview();
        super::set_item_query("horse");
        super::OPEN.set(true);
        super::hide();
        assert_find(false);
        assert_query_empty();
        assert!(!super::OPEN.get());
    }

    #[test]
    fn failed_blur_leaves_find_off() {
        let _reset = ResetFind::arm();
        let _force = ForceBlurOff::arm();
        super::MASKS.set(true);
        super::REVEALED.set(true);
        super::BLUR_ON.set(false);
        super::finish_preview();
        assert_find(true);
        super::set_item_query("horse");
        super::REVEALED.set(false);
        super::finish_preview();
        assert!(super::SHADE_ON.get());
        assert!(!super::BLUR_ON.get());
        assert_find(false);
        assert_query_empty();
    }

    #[test]
    fn search_after_image_or_link_finds_no_old_text() {
        let _reset = ResetFind::arm();
        super::store(copied("correct horse battery staple"));
        assert_eq!(
            super::find_matches(&super::current_item_text(), "horse").len(),
            1
        );
        super::store(picture());
        assert!(super::current_item_text().is_empty());
        assert!(super::find_matches(&super::current_item_text(), "horse").is_empty());
        super::store(copied("https://example.com/docs"));
        assert!(super::current_item_text().is_empty());
        assert!(super::find_matches(&super::current_item_text(), "horse").is_empty());
        assert!(super::find_matches(&super::current_item_text(), "example").is_empty());
    }

    #[test]
    fn match_counter_is_one_based() {
        assert_eq!(super::match_counter(2, 12), "3/12");
        assert_eq!(super::match_counter(0, 1), "1/1");
        assert_eq!(super::match_counter(0, 0), "0/0");
        assert_eq!(super::match_label("", 0, 4), "");
        assert_eq!(super::match_label("a", 0, 0), "0/0");
    }

    #[test]
    fn step_match_wraps() {
        assert_eq!(super::step_match(0, 12, true), 1);
        assert_eq!(super::step_match(11, 12, true), 0);
        assert_eq!(super::step_match(0, 12, false), 11);
        assert_eq!(super::step_match(3, 12, false), 2);
        assert_eq!(super::step_match(0, 1, true), 0);
        assert_eq!(super::step_match(0, 0, true), 0);
        assert_eq!(super::step_match(4, 0, false), 0);
    }

    #[test]
    fn find_matches_are_case_insensitive_and_nonoverlapping() {
        let text = "Alpha alpha ALPHA";
        let hits = super::find_matches(text, "alpha");
        assert_eq!(hits.len(), 3);
        assert_eq!(&text[hits[0].start..hits[0].end], "Alpha");
        assert_eq!(&text[hits[1].start..hits[1].end], "alpha");
        assert_eq!(super::match_counter(0, hits.len()), "1/3");
        assert_eq!(super::step_match(0, hits.len(), true), 1);
        assert_eq!(super::step_match(0, hits.len(), false), 2);
        assert!(super::find_matches(text, "").is_empty());
        assert_eq!(super::find_matches("aaaa", "aa"), vec![0..2, 2..4]);
        let accented = "héllo héllo";
        let marks = super::find_matches(accented, "Héllo");
        assert_eq!(marks.len(), 2);
        assert_eq!(&accented[marks[0].start..marks[0].end], "héllo");
    }
}
