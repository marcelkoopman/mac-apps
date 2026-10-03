#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::corners;
#[cfg(test)]
use mac_ui::find::find_matches;
use mac_ui::find::{match_label, step_match};
use mac_ui::glass;
use mac_ui::keys::Key;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, NSObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSApplicationDidResignActiveNotification, NSBeep, NSBox, NSButton,
    NSCellImagePosition, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn,
    NSEvent, NSEventModifierFlags, NSFocusRingType, NSFont, NSImage, NSImageView, NSLineBreakMode,
    NSMenu, NSMenuItem, NSScrollView, NSSearchField, NSTextAlignment, NSTextField,
    NSTextFieldBezelStyle, NSTextView, NSView, NSWindow, NSWindowOrderingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSEdgeInsets, NSNotification, NSNotificationCenter, NSObjectNSDelayedPerforming,
    NSPoint, NSRange, NSRect, NSSize, NSString,
};
use mac_ui::panel;
use mac_ui::progress::{self, SpinnerSize};
use mac_ui::text::AttrText;
use mac_ui::widgets::{self, filled_box, raise_view};
use zeroize::{Zeroize, Zeroizing};

use crate::appearance::Theme;
use crate::clipboard::SecretBytes;
use crate::commands::{self, ChipFrame, Command, CommandId, LaunchData};
use crate::format::FormatKind;
use crate::item_find;
use crate::launcher::{self, UserEvent};

const WIDTH: f64 = 440.0;
use crate::card_layout::{
    HEADER_H, ITEM_FIND_H, META_H, PAD, PREVIEW_H, VERSION_H, place_sections,
};
const HEADER_BUTTON: f64 = 22.0;
const CLEAR_BUTTON_W: f64 = 64.0;
const WELL_ACTION: f64 = 26.0;
const WELL_INSET: f64 = 8.0;
const WELL_GAP: f64 = 6.0;
/// Horizontal padding already inside the text view. The gutter is the extra
/// space that keeps lines clear of the icons.
const TEXT_INSET_X: f64 = 12.0;
const ITEM_COUNT_W: f64 = 56.0;
const HOTKEY: &str = crate::hotkey::LABEL;
/// Gaussian blur (`CIGaussianBlur` radius, in points) over masked text in the well. The privacy
/// tradeoff: the blur should show the shape of what was copied (how many lines, their indentation
/// and length, where code blocks are) so you recognise it, but never the text. At 6 pt each glyph
/// of the 12 pt monospace payload (7 pt wide, 15 pt lines) is spread over about ±12 pt, so letters
/// and words run together into one grey band per line: unreadable, also for digits and short
/// words, while lines and indentation still show. Below about 4 pt short words and numbers start
/// to come through.
const TEXT_BLUR_RADIUS: f64 = 6.0;
/// The blur over a masked picture. Stronger than [`TEXT_BLUR_RADIUS`]: a picture is scaled to the
/// well, so text inside it (a screenshot, a photographed card or document) can be much larger
/// than 12 pt. At 10 pt the outline, the main shapes and colours stay recognisable, large text
/// does not.
const IMAGE_BLUR_RADIUS: f64 = 10.0;
const _: () = assert!(TEXT_BLUR_RADIUS >= 5.0 && TEXT_BLUR_RADIUS < IMAGE_BLUR_RADIUS);
/// Glass controls this close merge on macOS 26+ (`glass::group`). Below the 6 pt gaps between
/// chips and header buttons, so they only merge while they morph closer together.
const GLASS_MERGE: f64 = 4.0;
/// History capsule: chevron symbol size and the position label's font size.
const NAV_SYMBOL: f64 = 12.0;
const NAV_FONT: f64 = 12.0;
/// Corner radius of the launcher panel, its glass and the frosted fallback. Every nested
/// radius derives from it. Fixed: objc2-app-kit 0.3.2 has no public API for the system window
/// corner radius on macOS 26+ (`NSViewCornerConfiguration` is not bound), so the panel keeps
/// this documented value instead of matching it at runtime.
const PANEL_RADIUS: f64 = 16.0;
/// The well (content panel) and its reveal shade sit `PAD` inside the panel edge, so their
/// corners are concentric with the panel's: `PANEL_RADIUS - PAD`, clamped at
/// [`corners::MIN_RADIUS`].
const WELL_RADIUS: f64 = corners::concentric_radius(PANEL_RADIUS, PAD);
/// What the card takes when it is dropped on it: one text file of any kind (plain, source code,
/// JSON, XML, CSV, YAML, Markdown, … all conform to `public.text`) or picture (`public.image`:
/// PNG, JPEG, HEIC, GIF, WebP, TIFF, …), also as a promised file (Photos); image data (a picture
/// dragged from Safari or Preview); or dropped text. Not PDFs, folders or several files.
const DROP_ACCEPT: mac_ui::drop::Accept = mac_ui::drop::Accept {
    file_types: &["public.text", "public.image"],
    images: true,
    text: true,
};
/// The drop outline sits this far inside the panel edge, concentric with it.
const DROP_INSET: f64 = 4.0;
const DROP_HIGHLIGHT: mac_ui::drop::Highlight = mac_ui::drop::Highlight {
    inset: DROP_INSET,
    corner_radius: corners::concentric_radius(PANEL_RADIUS, DROP_INSET),
    announcement: "Drop to open",
};

thread_local! {
    static OPEN: Cell<bool> = const { Cell::new(false) };
    /// Set while the menu bar icon opens the card, which then opens under it.
    static ICON_FRAME: Cell<Option<NSRect>> = const { Cell::new(None) };
    static SEARCHING: Cell<bool> = const { Cell::new(false) };
    static SELECTION: Cell<usize> = const { Cell::new(0) };
    static THEME: Cell<Theme> = const { Cell::new(Theme::System) };
    /// The `⋯` menu's check marks for settings.
    static CLEAR_SENSITIVE: Cell<bool> = const { Cell::new(true) };
    static HISTORY_MINUTES: Cell<u32> = const { Cell::new(15) };
    static SHOWS_IMAGE: Cell<bool> = const { Cell::new(false) };
    static THUMB_TOKEN: Cell<isize> = const { Cell::new(-1) };
    /// The dropped picture the card shows instead of the clipboard's (`LaunchData::picture`).
    static PICTURE: RefCell<Option<SecretBytes>> = const { RefCell::new(None) };
    /// Allocation of the dropped picture in the well, 0 for none (see `THUMB_TOKEN`).
    static PICTURE_SHOWN: Cell<usize> = const { Cell::new(0) };
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
    static WINDOW: RefCell<Option<Retained<LauncherWindow>>> = const { RefCell::new(None) };
    static FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static HEADER: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static META: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static WELL: RefCell<Option<Retained<NSBox>>> = const { RefCell::new(None) };
    static PREVIEW_TEXT: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
    static PREVIEW_SCROLL: RefCell<Option<Retained<NSScrollView>>> = const { RefCell::new(None) };
    static PREVIEW_IMAGE: RefCell<Option<Retained<NSImageView>>> = const { RefCell::new(None) };
    /// Chip row: a glass group holding one GlassButton per shown command.
    static PILLS: RefCell<Option<glass::Group>> = const { RefCell::new(None) };
    /// The chips in [`SHOWN`] order; index = chip tag = selection.
    static CHIPS: RefCell<Vec<Rc<GlassButton>>> = const { RefCell::new(Vec::new()) };
    /// Every chip button in the chip row, shown or hidden, by command and title. Searching hides
    /// and shows these instead of making new buttons; a command the card no longer offers goes.
    static CHIP_POOL: RefCell<Vec<PooledChip>> = const { RefCell::new(Vec::new()) };
    /// "No matching commands" under a search that matches nothing.
    static NO_MATCHES: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static HISTORY_NAV: RefCell<Option<commands::HistoryNav>> = const { RefCell::new(None) };
    /// The history capsule: a glass (or frosted) capsule holding the chevrons and the position.
    static NAV_CAPSULE: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
    /// `‹` previous (toward 1, the newest) and `›` next (toward the oldest).
    static PREVIOUS: RefCell<Option<NavButton>> = const { RefCell::new(None) };
    static NEXT: RefCell<Option<NavButton>> = const { RefCell::new(None) };
    /// The position ("1 / 3") between the chevrons.
    static NAV_COUNT: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    /// Wipe, More and Close in one glass group at the right of the header.
    static HEADER_GROUP: RefCell<Option<glass::Group>> = const { RefCell::new(None) };
    static MORE: RefCell<Option<GlassButton>> = const { RefCell::new(None) };
    static CLEAR: RefCell<Option<GlassButton>> = const { RefCell::new(None) };
    static CLOSE: RefCell<Option<GlassButton>> = const { RefCell::new(None) };
    static CONTENT_ACTIONS: Cell<commands::ContentActions> = const {
        Cell::new(commands::ContentActions {
            copy: false,
            save: false,
        })
    };
    /// Right and bottom insets of the text: room for the Copy/Save buttons and the Show all pill.
    static TEXT_GUTTER: Cell<(f64, f64)> = const { Cell::new((-1.0, -1.0)) };
    static COPY_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    static SAVE_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    /// Busy wheel over the well while a save runs. Created on first use.
    static SPINNER: RefCell<Option<progress::Spinner>> = const { RefCell::new(None) };
    /// "Showing 200 of 23,220 rows" when the well holds a preview. Empty otherwise.
    static PREVIEW_NOTE: RefCell<String> = const { RefCell::new(String::new()) };
    /// "Show all" pill at the bottom of the well, rebuilt when its title changes.
    static SHOW_ALL: RefCell<Option<ShowAllButton>> = const { RefCell::new(None) };
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
    /// Counts changes of [`ITEM_TEXT`], so find compares a number instead of the texts.
    static ITEM_GEN: Cell<u64> = const { Cell::new(0) };
    /// The [`ITEM_GEN`] the well's text view shows. `None` while it shows something else
    /// (a placeholder, a blanked mask); find marks are only painted on the item itself.
    static PAINTED_ITEM: Cell<Option<u64>> = const { Cell::new(None) };
    /// The item folded for find and the current query's matches, dropped (zeroized) with the item.
    static FIND_CACHE: RefCell<Option<FindCache>> = const { RefCell::new(None) };
    static BLUR_ON: Cell<bool> = const { Cell::new(false) };
    /// The text and picture blurs ([`gaussian_blurs`]), built the first time the well is masked.
    static BLUR_FILTERS: RefCell<Option<(Retained<AnyObject>, Retained<AnyObject>)>> =
        const { RefCell::new(None) };
    static SHADE_ON: Cell<bool> = const { Cell::new(false) };
    static MASKS: Cell<bool> = const { Cell::new(false) };
    static CONTENT_KEY: Cell<u64> = const { Cell::new(0) };
    static DELEGATE: RefCell<Option<Retained<LauncherDelegate>>> = const { RefCell::new(None) };
    /// The table's version bar ([`commands::VersionBar`]); `None` hides it.
    static VERSION_BAR: RefCell<Option<commands::VersionBar>> = const { RefCell::new(None) };
    /// The version bar's views.
    static VERSION_ROW: RefCell<Option<VersionRow>> = const { RefCell::new(None) };
    /// The "Table ▾" menu's commands ([`commands::table_menu`]).
    static TABLE_MENU: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// The commands of the menu popped last (table steps or versions), by item tag.
    static POPPED: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// The window's one field editor, made on first use. It takes no drops (see
    /// `windowWillReturnFieldEditor:toObject:`).
    static FIELD_EDITOR: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
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
            // Tab and Shift-Tab from a focused button or the reveal cover reach the window.
            // NSWindow's own keyDown: moves along the key-view loop. Without it, Tab would stop
            // on the first button under Full Keyboard Access.
            if Key::from_key_code(event.keyCode()) == Some(Key::Tab) {
                let _: () = unsafe { msg_send![super(self), keyDown: event] };
                return;
            }
            on_key(event);
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if is_item_find_chord(event) {
                if item_find_on() {
                    focus_item_field();
                }
                true
            } else if let Some(id) = undo_chord(event) {
                // ⌘Z / ⇧⌘Z step through the table's versions (typed search text keeps them).
                launcher::emit(UserEvent::Run(id));
                true
            } else if let Some(key) = command_arrow(event) {
                // ⌘← / ⌘→ step through history from anywhere on the card, also from a field
                // with text (key equivalents reach the window before the field editor).
                on_arrow(key, true, commands::ArrowFocus::Card);
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
    #[name = "CopycraftLauncherDelegate"]
    struct LauncherDelegate;

    impl LauncherDelegate {
        /// The card stays open when another app becomes active (it is the drop target while
        /// you drag from there), but revealed content is masked again.
        #[unsafe(method(applicationDidResignActive:))]
        fn application_did_resign_active(&self, _note: &NSNotification) {
            mask_again();
        }

        /// Both search fields edit in a field editor that takes no drops, so text dragged over
        /// a field being edited is dropped on the card like anywhere else.
        #[unsafe(method_id(windowWillReturnFieldEditor:toObject:))]
        fn window_will_return_field_editor(
            &self,
            _sender: &NSWindow,
            _client: Option<&AnyObject>,
        ) -> Option<Retained<NSTextView>> {
            let mtm = self.mtm();
            FIELD_EDITOR.with(|slot| {
                Some(
                    slot.borrow_mut()
                        .get_or_insert_with(|| mac_ui::drop::field_editor_without_drops(mtm))
                        .clone(),
                )
            })
        }

        #[unsafe(method(controlTextDidChange:))]
        fn control_text_did_change(&self, note: &NSNotification) {
            if note_is_item_field(note) {
                item_query_changed(self);
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
            } else if let Some(key) = arrow_command(command) {
                // The search field has the focus while the card is open. Typed text keeps
                // ← → for its caret.
                let focus = if current_query().is_empty() {
                    commands::ArrowFocus::Card
                } else {
                    commands::ArrowFocus::TextWithContent
                };
                on_arrow(key, false, focus)
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

        #[unsafe(method(showAllClicked:))]
        fn show_all_clicked(&self, _sender: Option<&NSButton>) {
            SHOW_ALL.with(|slot| {
                if let Some(pill) = slot.borrow().as_ref() {
                    pill.button.view().setHidden(true);
                }
            });
            launcher::emit(UserEvent::Run(CommandId::ShowAll));
        }

        #[unsafe(method(revealClicked:))]
        fn reveal_clicked(&self, _sender: Option<&NSButton>) {
            if !MASKS.with(Cell::get) || REVEALED.with(Cell::get) {
                return;
            }
            // Not before the labels are known: the meta line says "Checking…" meanwhile.
            if crate::sensitivity::meta_status(&card_meta()) == Some(crate::sensitivity::CHECKING) {
                NSBeep();
                return;
            }
            REVEALED.set(true);
            layout(false);
        }

        #[unsafe(method(findAfterPause:))]
        fn find_after_pause(&self, _sender: Option<&AnyObject>) {
            take_item_query_from_field();
        }

        #[unsafe(method(tableUndoClicked:))]
        fn table_undo_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::TableUndo));
        }

        #[unsafe(method(tableRedoClicked:))]
        fn table_redo_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::TableRedo));
        }

        #[unsafe(method(versionsClicked:))]
        fn versions_clicked(&self, _sender: Option<&NSButton>) {
            pop_versions_menu();
        }

        #[unsafe(method(poppedClicked:))]
        fn popped_clicked(&self, sender: Option<&NSMenuItem>) {
            let Some(item) = sender else {
                return;
            };
            let index = item.tag();
            if index < 0 {
                return;
            }
            let cmd = POPPED.with(|slot| slot.borrow().get(index as usize).cloned());
            if let Some(cmd) = cmd {
                run_command(cmd);
            }
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

/// Spin a busy wheel centered over the well (on top of the preview), or take it away.
pub fn set_busy(busy: bool) {
    if !busy {
        SPINNER.with(|slot| {
            if let Some(spinner) = slot.borrow().as_ref() {
                spinner.remove();
            }
        });
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(well) = WELL.with(|slot| slot.borrow().clone()) else {
        return;
    };
    // SAFETY: the superview is used right away, while the view hierarchy keeps it alive.
    let Some(parent) = (unsafe { well.superview() }) else {
        return;
    };
    SPINNER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let spinner = slot.get_or_insert_with(|| progress::Spinner::new(mtm, SpinnerSize::Regular));
        spinner.add_centered(&parent, well.frame());
        spinner.start();
    });
}

/// Keep a running spinner centered over the well and above the views raised after it.
fn place_spinner(well_frame: NSRect) {
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref()
            && spinner.is_added()
        {
            spinner.center_in(well_frame);
            raise_view(spinner.view());
        }
    });
}

pub fn order_front() {
    if !is_open() {
        return;
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            panel::bring_to_front(window);
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

/// Close an open card, or open it under the menu bar icon of `tray` (at the pointer when the
/// icon has no frame).
pub fn toggle_under_icon(data: LaunchData, tray: &mac_ui::tray_icon::TrayIcon) {
    if is_open() {
        hide();
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    ICON_FRAME.set(panel::tray_icon_frame(mtm, tray));
    reveal(data);
    ICON_FRAME.set(None);
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

/// [`sync`] with `card` already built from `data`: the "Show all" card (built off the main
/// thread), or a history entry's card built earlier.
pub fn sync_with_card(data: LaunchData, card: commands::WorkCard) {
    if !is_open() {
        return;
    }
    store_with_card(data, card);
    layout(false);
}

fn present(data: LaunchData, fresh: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    panel::activate_app(mtm);
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
            panel::bring_to_front(window);
        }
    });
    focus_card();
    // Layout runs before the window is on screen. Setting the picture only then
    // leaves the layer empty, and Original stays on that blank well.
    if SHOWS_IMAGE.with(Cell::get) {
        THUMB_TOKEN.set(-1);
        PICTURE_SHOWN.set(0);
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
    let card = commands::work_card(&data);
    store_with_card(data, card);
}

fn store_with_card(data: LaunchData, card: commands::WorkCard) {
    let key = commands::content_key(&data);
    if CONTENT_KEY.with(Cell::get) != key {
        CONTENT_KEY.set(key);
        REVEALED.set(false);
        set_item_find(false);
    }
    PREVIEW_NOTE.with(|slot| slot.replace(card.preview_note.clone().unwrap_or_default()));
    MASKS.set(commands::masks_content(&card, data.view));
    CARD_TITLE.with(|slot| set_secret(slot, card.title));
    CARD_META.with(|slot| set_secret(slot, card.meta));
    CARD_EXCERPT.with(|slot| set_secret(slot, card.excerpt));
    CARD_PLACEHOLDER.with(|slot| set_secret(slot, card.placeholder));
    CARD_HIGHLIGHT.with(|slot| slot.replace(card.highlight));
    CARD_SELECTABLE.set(card.selectable);
    LINK_PAGE.with(|slot| set_secret_opt(slot, card.link_page));
    SHOWS_IMAGE.set(card.shows_image);
    PICTURE.with(|slot| slot.replace(data.picture.clone()));
    publish_search_text();
    THEME.set(data.theme);
    CLEAR_SENSITIVE.set(data.settings.clear_sensitive);
    HISTORY_MINUTES.set(data.settings.history_minutes);
    ACTIONS.with(|slot| set_commands(slot, commands::chips(&data)));
    POOL.with(|slot| set_commands(slot, commands::search_pool(&data)));
    OVERFLOW.with(|slot| set_commands(slot, commands::overflow(&data)));
    HISTORY_NAV.with(|slot| slot.replace(data.history_nav));
    CONTENT_ACTIONS.set(commands::content_actions(&data));
    VERSION_BAR.with(|slot| slot.replace(commands::VersionBar::of(&data)));
    TABLE_MENU.with(|slot| set_commands(slot, commands::table_menu(data.table.as_ref())));
}

fn hide() {
    set_item_find(false);
    if !is_open() && !window_is_visible() {
        return;
    }
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.orderOut(None);
        }
    });
    OPEN.set(false);
}

fn window_is_visible() -> bool {
    WINDOW.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|window| window.isVisible())
    })
}

fn ensure_window(mtm: MainThreadMarker) {
    if WINDOW.with(|slot| slot.borrow().is_some()) {
        return;
    }
    let window = panel::borderless(LauncherWindow::alloc(mtm), NSSize::new(WIDTH, 420.0));
    panel::configure_floating(&window);
    // The card stays open, floating over other apps' windows, until ✕, Esc, the hotkey or the
    // menu bar icon closes it. (NSWindow's default; said here because the card relies on it.)
    window.setHidesOnDeactivate(false);
    // Chips, header buttons and the find bar are added, removed and hidden on every layout.
    // Recalculating the key-view loop keeps Tab and Shift-Tab in on-screen order (top left to
    // bottom right) over the views that are actually shown.
    window.setAutorecalculatesKeyViewLoop(true);

    let delegate = LauncherDelegate::new(mtm);
    // SAFETY: DELEGATE keeps the delegate alive for the rest of the process, like WINDOW.
    unsafe { panel::set_delegate(&window, &*delegate) };
    // SAFETY: the selector is the delegate's `applicationDidResignActive:`, which takes the
    // notification. DELEGATE keeps the observer alive for the rest of the process.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &delegate,
            sel!(applicationDidResignActive:),
            Some(NSApplicationDidResignActiveNotification),
            None,
        );
    }
    DELEGATE.with(|slot| slot.replace(Some(delegate)));

    // The content view takes drops (mac_ui::drop). Everything on the card is inside it, so a
    // drag anywhere over the card reaches it unless an editable text view takes it first.
    let drop_target = mac_ui::drop::target(mtm, DROP_ACCEPT, Some(DROP_HIGHLIGHT), |dropped| {
        launcher::emit(match dropped {
            mac_ui::drop::Dropped::File(path) => UserEvent::DroppedFile(path),
            mac_ui::drop::Dropped::Promised(path) => UserEvent::DroppedPromisedFile(path),
            mac_ui::drop::Dropped::Image(bytes) => {
                UserEvent::DroppedImage(zeroize::Zeroizing::new(bytes))
            }
            mac_ui::drop::Dropped::Text(text) => {
                UserEvent::DroppedText(zeroize::Zeroizing::new(text))
            }
        });
    });
    window.setContentView(Some(&drop_target));
    let window_view = window.contentView().expect("content view");
    window_view.setWantsLayer(true);
    window_view.setLayerUsesCoreImageFilters(true);

    // Rounded corners plus Liquid Glass on macOS 26+, else the frosted view. Before 26,
    // `content` is `window_view`.
    let content = panel::rounded_glass(mtm, &window_view, PANEL_RADIUS).content;

    let header = widgets::label(mtm, 13.0, &NSColor::labelColor());
    // A long type name is cut in the middle so the hotkey stays visible.
    header.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let meta = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    // Keep the warning at the end when a long filename is cut short.
    meta.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    meta.setAllowsEditingTextAttributes(true);
    let field = search_field(mtm);
    let item_find = item_search_field(mtm);
    let item_count = widgets::label(mtm, 12.0, &NSColor::secondaryLabelColor());
    item_count.setAlignment(NSTextAlignment::Right);
    item_count.setHidden(true);
    let well = filled_box(mtm, WELL_RADIUS, &NSColor::controlBackgroundColor());
    let preview_text = payload_view(mtm);
    let preview_scroll = text_scroll(mtm, &preview_text);
    let preview_image = image_view(mtm);
    let pills = glass::group(mtm, GLASS_MERGE);
    let (nav_capsule, previous, nav_count, next) = history_capsule(mtm);
    let more = header_symbol(mtm, "ellipsis", "More", "⋯", sel!(moreClicked:));
    let clear = GlassButton::pill(mtm, "Wipe", ButtonSize::Small);
    wire_button(clear.button(), sel!(clearClicked:));
    clear
        .button()
        .setToolTip(Some(&NSString::from_str("Wipe copied data from memory")));
    let close = header_symbol(mtm, "xmark", "Close", "✕", sel!(closeClicked:));
    let header_group = glass::group(mtm, GLASS_MERGE);
    header_group.content().addSubview(clear.view());
    header_group.content().addSubview(more.view());
    header_group.content().addSubview(close.view());
    let copy_button = well_action(mtm, "doc.on.doc", "Copy", "⎘", sel!(copyClicked:));
    let save_button = well_action(
        mtm,
        "square.and.arrow.down",
        "Save",
        "↓",
        sel!(saveClicked:),
    );
    let reveal = reveal_cover(mtm);
    let version_row = version_row(mtm);

    content.addSubview(&header);
    content.addSubview(&item_find);
    content.addSubview(&item_count);
    content.addSubview(header_group.view());
    content.addSubview(&well);
    content.addSubview(&preview_scroll);
    content.addSubview(&preview_image);
    content.addSubview(&reveal.root);
    content.addSubview(&meta);
    content.addSubview(&field);
    content.addSubview(pills.view());
    content.addSubview(&nav_capsule);
    content.addSubview(&version_row.capsule);
    content.addSubview(copy_button.view());
    content.addSubview(save_button.view());

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
    NAV_CAPSULE.with(|slot| slot.replace(Some(nav_capsule)));
    PREVIOUS.with(|slot| slot.replace(Some(previous)));
    NEXT.with(|slot| slot.replace(Some(next)));
    NAV_COUNT.with(|slot| slot.replace(Some(nav_count)));
    HEADER_GROUP.with(|slot| slot.replace(Some(header_group)));
    MORE.with(|slot| slot.replace(Some(more)));
    CLEAR.with(|slot| slot.replace(Some(clear)));
    CLOSE.with(|slot| slot.replace(Some(close)));
    COPY_BUTTON.with(|slot| slot.replace(Some(copy_button)));
    SAVE_BUTTON.with(|slot| slot.replace(Some(save_button)));
    REVEAL.with(|slot| slot.replace(Some(reveal)));
    VERSION_ROW.with(|slot| slot.replace(Some(version_row)));
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
    let meta = card_meta();
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // Each chip is laid out at the width its own pill measures, so titles never get cut early.
    // The buttons are reused: a search hides and shows them (see `CHIP_POOL`).
    ACTIONS.with(|actions| {
        POOL.with(|pool| {
            let (actions, pool) = (actions.borrow(), pool.borrow());
            let offered: Vec<&Command> = actions.iter().chain(pool.iter()).collect();
            prune_chip_pool(&offered);
        });
    });
    let chips: Vec<Rc<GlassButton>> = shown.iter().map(|cmd| chip_for(mtm, cmd)).collect();
    let widths: Vec<f64> = chips
        .iter()
        .map(|chip| commands::chip_width(chip.fitted_size().width))
        .collect();
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
        commands::layout_chips(&widths, inner, reserve)
    };
    let show_empty = shown.is_empty() && searching && !query.trim().is_empty();
    let item_find = !well_is_masked();
    let version_bar = VERSION_BAR.with(|slot| slot.borrow().clone());
    let placed = place_sections(
        &meta,
        searching,
        &frames,
        show_empty,
        nav.is_some(),
        item_find,
        version_bar.is_some(),
    );
    place_window(mtm, placed.height, fresh_place);
    let title = CARD_TITLE.with(|slot| header_title(&slot.borrow()));
    let close_x = WIDTH - PAD - 24.0;
    let more_x = close_x - 6.0 - HEADER_BUTTON;
    let clear_x = more_x - 6.0 - CLEAR_BUTTON_W;
    let title_w = (clear_x - PAD - 8.0).max(40.0);
    HEADER.with(|slot| {
        set_label(slot, PAD, placed.header_y, title_w, HEADER_H, &title);
    });
    HEADER_GROUP.with(|slot| {
        if let Some(group) = slot.borrow().as_ref() {
            group.view().setFrame(NSRect::new(
                NSPoint::new(clear_x, placed.header_y + (HEADER_H - HEADER_BUTTON) / 2.0),
                NSSize::new(close_x + HEADER_BUTTON - clear_x, HEADER_BUTTON),
            ));
        }
    });
    CLEAR.with(|slot| place_header_button(slot, 0.0, CLEAR_BUTTON_W));
    MORE.with(|slot| place_header_button(slot, more_x - clear_x, HEADER_BUTTON));
    CLOSE.with(|slot| place_header_button(slot, close_x - clear_x, HEADER_BUTTON));
    place_item_find(placed.find_y, item_find);
    place_well(placed.preview_y);
    apply_preview(placed.preview_y);
    place_content_actions(placed.preview_y);
    place_show_all(mtm, placed.preview_y);
    place_version_row(placed.version_y, version_bar.as_ref());
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
            // Not setHidden: the field stays first responder while the card is open, so typing
            // (or "/") starts a search and the arrow keys move between chips. A hidden view
            // would give up focus. Out of search it has a zero frame, so it takes no clicks, and
            // alpha 0 keeps its focus ring from drawing.
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
            pills.view().setFrame(NSRect::new(
                NSPoint::new(PAD, placed.chips_y),
                NSSize::new(inner, placed.chips_h),
            ));
            rebuild_pills(
                mtm,
                pills.content(),
                chips,
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
}

/// A fresh launcher opens just above (or below) the pointer, 8 pt inside the visible screen.
const NEAR_CURSOR: panel::NearCursor = panel::NearCursor {
    margin: 8.0,
    lead_x: 36.0,
    gap_y: 12.0,
};
/// Opened from the menu bar icon, it opens under the icon instead, like a menu.
const UNDER_ICON: panel::UnderIcon = panel::UnderIcon {
    margin: 8.0,
    gap_y: 6.0,
};

fn place_window(mtm: MainThreadMarker, height: f64, fresh: bool) {
    WINDOW.with(|slot| {
        let borrowed = slot.borrow();
        let Some(window) = borrowed.as_ref() else {
            return;
        };
        let size = NSSize::new(WIDTH, height);
        let frame = if fresh {
            match ICON_FRAME.get() {
                Some(icon) => panel::under_icon(mtm, icon, size, &UNDER_ICON),
                None => panel::near_cursor(mtm, size, &NEAR_CURSOR),
            }
        } else {
            panel::keep_top_left(window.frame(), size)
        };
        window.setFrame_display(frame, true);
    });
}

fn place_well(y: f64) {
    let frame = NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, PREVIEW_H),
    );
    WELL.with(|slot| {
        if let Some(well) = slot.borrow().as_ref() {
            well.setFrame(frame);
        }
    });
    place_spinner(frame);
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
    let shade = filled_box(mtm, WELL_RADIUS, &NSColor::controlBackgroundColor());
    shade.setFrame(frame);
    shade.setHidden(true);
    let hit = NSButton::initWithFrame(NSButton::alloc(mtm), frame);
    hit.setBordered(false);
    hit.setTransparent(true);
    hit.setTitle(&NSString::from_str(""));
    // Keyboard users can Tab to the cover and press Space to reveal, so keep the focus ring.
    hit.setFocusRingType(NSFocusRingType::Default);
    hit.setAccessibilityLabel(Some(&NSString::from_str("Reveal hidden content")));
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

/// Blur revealed content again (the app is no longer active). The next click reveals it.
fn mask_again() {
    if !is_open() || !MASKS.with(Cell::get) || !REVEALED.with(Cell::get) {
        return;
    }
    REVEALED.set(false);
    layout(false);
    // As in `present`: the blur, then `<` `>` and the well icons, on top.
    show_reveal_cover(true);
    raise_history_nav();
    raise_content_actions();
}

fn well_is_masked() -> bool {
    MASKS.with(Cell::get) && !REVEALED.with(Cell::get)
}

/// The blurs for the well's text and its picture, or `None` when Core Image cannot build them.
/// Built once and reused: every card update (each step through history) masks the well again.
fn gaussian_blurs() -> Option<(Retained<AnyObject>, Retained<AnyObject>)> {
    #[cfg(test)]
    if FAIL_BLUR.with(Cell::get) {
        return None;
    }
    if let Some(built) = BLUR_FILTERS.with(|slot| slot.borrow().clone()) {
        return Some(built);
    }
    let built = (
        mac_ui::blur::gaussian(TEXT_BLUR_RADIUS)?,
        mac_ui::blur::gaussian(IMAGE_BLUR_RADIUS)?,
    );
    BLUR_FILTERS.with(|slot| slot.replace(Some(built.clone())));
    Some(built)
}

fn set_well_blur(on: bool) {
    set_well_accessible(!on);
    // The cover and the find bar follow the same mask. Revealed text can be searched.
    set_item_find(!on);
    if !on {
        SHADE_ON.set(false);
        show_mask_shade(false);
        clear_blur();
        return;
    }
    let filters = gaussian_blurs();
    let plan = commands::well_mask(true, filters.is_some());
    SHADE_ON.set(plan.shade);
    show_mask_shade(plan.shade);
    if let Some((text, image)) = filters {
        if !BLUR_ON.get() {
            blur_both(
                &NSArray::from_slice(&[&*text]),
                &NSArray::from_slice(&[&*image]),
            );
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

/// While the well is masked, its blurred text and picture are hidden from accessibility, so
/// VoiceOver (or any accessibility client) cannot read what the blur keeps from the eye. The
/// reveal cover button stays reachable.
fn set_well_accessible(accessible: bool) {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setAccessibilityElement(accessible);
        }
    });
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setAccessibilityElement(accessible);
        }
    });
}

fn clear_blur() {
    if !BLUR_ON.get() {
        return;
    }
    blur_both(&NSArray::from_slice(&[]), &NSArray::from_slice(&[]));
    BLUR_ON.set(false);
}

/// Install `text` on the well's text and `image` on its picture (empty arrays clear them).
fn blur_both(text: &NSArray<AnyObject>, image: &NSArray<AnyObject>) {
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            mac_ui::blur::set_content_filters(view, text);
        }
    });
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            mac_ui::blur::set_content_filters(view, image);
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
    PAINTED_ITEM.set(None);
}

fn card_meta() -> String {
    CARD_META.with(|slot| slot.borrow().clone())
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
    // A link is plain text in the well, like any copy (copycraft never goes online).
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
    mac_ui::keys::is_command_chord(event, "f")
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
        step_item_match(!mac_ui::keys::shift_held());
        true
    } else if (command == sel!(moveLeft:) || command == sel!(moveRight:)) && item_query().is_empty()
    {
        // An empty find field has no caret to move: ← → step through history.
        arrow_command(command).is_some_and(|key| on_arrow(key, false, commands::ArrowFocus::Card))
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

fn set_item_text(mut next: String) {
    let changed = ITEM_TEXT.with(|slot| {
        let mut text = slot.borrow_mut();
        if *text == next {
            next.zeroize();
            return false;
        }
        text.zeroize();
        *text = next;
        true
    });
    if changed {
        ITEM_GEN.set(ITEM_GEN.get().wrapping_add(1));
        FIND_CACHE.with(|slot| slot.borrow_mut().take());
    }
}

#[cfg(test)]
fn current_item_text() -> String {
    ITEM_TEXT.with(|slot| slot.borrow().clone())
}

/// [`ITEM_TEXT`] folded for find ([`item_find::fold`]) and the matches of `query` in it.
struct FindCache {
    item_gen: u64,
    folded: Zeroizing<Vec<u8>>,
    query: Zeroizing<String>,
    matches: item_find::Matches,
}

/// The matches of `query` in the current item, folding the item only when it changed and
/// searching only when the query did. The item's text is borrowed, not copied.
fn with_item_matches<R>(query: &str, f: impl FnOnce(&FindCache) -> R) -> R {
    FIND_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let item_gen = ITEM_GEN.get();
        if slot.as_ref().is_none_or(|cache| cache.item_gen != item_gen) {
            let folded = ITEM_TEXT.with(|text| item_find::fold(&text.borrow()));
            *slot = Some(FindCache {
                item_gen,
                folded,
                query: Zeroizing::new(String::new()),
                matches: item_find::Matches::default(),
            });
        }
        let cache = slot.as_mut().expect("find cache was just filled");
        if cache.query.as_str() != query {
            cache.matches = item_find::find(&cache.folded, query);
            cache.query.zeroize();
            cache.query.push_str(query);
        }
        f(cache)
    })
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
    let mut query = item_query();
    let total = if query.is_empty() {
        0
    } else {
        with_item_matches(&query, |cache| cache.matches.total())
    };
    query.zeroize();
    if total == 0 {
        return;
    }
    let index = FIND_INDEX.get().min(total - 1);
    FIND_INDEX.set(step_match(index, total, forward));
    refresh_item_marks(true);
}

fn refresh_item_marks(scroll: bool) {
    let mut query = item_query();
    if query.is_empty() {
        // Nothing to find: no fold of the item, no scan, just clear the marks.
        FIND_INDEX.set(0);
        if MainThreadMarker::new().is_some() {
            paint_match_marks(&[], None, false);
            set_match_label("");
        }
        return;
    }
    let shown = PAINTED_ITEM.get() == Some(ITEM_GEN.get());
    let (total, painted) = with_item_matches(&query, |cache| {
        let total = cache.matches.total();
        if total == 0 || FIND_INDEX.get() >= total {
            FIND_INDEX.set(0);
        }
        let painted =
            shown.then(|| item_find::painted(&cache.folded, &cache.matches, FIND_INDEX.get()));
        (total, painted)
    });
    let index = FIND_INDEX.get();
    let label = match_label(&query, index, total);
    query.zeroize();
    if MainThreadMarker::new().is_some() {
        match painted {
            Some((ranges, current)) => {
                paint_match_marks(&ranges, Some(current), scroll && total > 0);
            }
            None => paint_match_marks(&[], None, false),
        }
        set_match_label(&label);
    }
}

/// The item field changed. A long item is searched once typing pauses for
/// [`item_find::DEBOUNCE`]; a short one right away.
fn item_query_changed(delegate: &LauncherDelegate) {
    let long = ITEM_TEXT.with(|text| text.borrow().len() >= item_find::DEBOUNCE_FROM);
    if !long {
        take_item_query_from_field();
        return;
    }
    let target: &AnyObject = delegate.as_ref();
    // SAFETY: `findAfterPause:` is a method of the delegate that takes an optional sender.
    unsafe {
        NSObject::cancelPreviousPerformRequestsWithTarget_selector_object(
            target,
            sel!(findAfterPause:),
            None,
        );
        delegate.performSelector_withObject_afterDelay(
            sel!(findAfterPause:),
            None,
            item_find::DEBOUNCE.as_secs_f64(),
        );
    }
}

fn set_match_label(label: &str) {
    ITEM_COUNT.with(|slot| {
        if let Some(count) = slot.borrow().as_ref() {
            count.setStringValue(&NSString::from_str(label));
        }
    });
}

fn paint_match_marks(ranges: &[(usize, usize)], current: Option<usize>, scroll: bool) {
    let view = PREVIEW_TEXT.with(|slot| slot.borrow().clone());
    let Some(view) = view else {
        return;
    };
    widgets::mark_ranges(&view, ranges, current, scroll);
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
        widgets::wipe_field_editor(field);
        widgets::wipe_text_field(field);
    });
}

fn clear_preview_text() {
    set_item_text(String::new());
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            widgets::wipe_text_view(view);
        }
    });
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
    PAINTED_ITEM.set(None);
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
            // setHidden keeps clicks, scrolling and focus off the hidden text. The alpha only
            // backs it up: a layer-backed scroll view can keep painting after setHidden.
            view.setHidden(hidden);
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
        PAINTED_ITEM.set(Some(ITEM_GEN.get()));
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
        widgets::fit_text_view(view, wrap);
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
    // `set_item_text(body)` above: the view shows the item.
    PAINTED_ITEM.set(Some(ITEM_GEN.get()));
}

fn load_thumbnail() {
    let already = PREVIEW_IMAGE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|view| !view.isHidden() && view.image().is_some())
    });
    if let Some(picture) = PICTURE.with(|slot| slot.borrow().clone()) {
        load_dropped_picture(&picture, already);
        return;
    }
    PICTURE_SHOWN.set(0);
    let change = crate::macos_pasteboard::change_count();
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

/// Draw a dropped picture in the well, decoded once per picture like the clipboard's.
fn load_dropped_picture(picture: &SecretBytes, already: bool) {
    // The clipboard's picture is drawn again when the card goes back to it.
    THUMB_TOKEN.set(-1);
    let id = picture.allocation_id();
    if PICTURE_SHOWN.get() == id && already {
        return;
    }
    let Some(image) = picture.with(mac_ui::image::from_bytes) else {
        return;
    };
    if window_is_visible() {
        PICTURE_SHOWN.set(id);
    }
    set_preview_image(&image);
}

/// One chip button kept in the chip row, and the command and title it was made for.
struct PooledChip {
    id: CommandId,
    title: String,
    chip: Rc<GlassButton>,
}

/// The chip for `cmd`: the pooled button with the same command and title, or a new one.
fn chip_for(mtm: MainThreadMarker, cmd: &Command) -> Rc<GlassButton> {
    let pooled = CHIP_POOL.with(|pool| {
        pool.borrow()
            .iter()
            .find(|pooled| pooled.id == cmd.id && pooled.title == cmd.title)
            .map(|pooled| Rc::clone(&pooled.chip))
    });
    let chip = pooled.unwrap_or_else(|| {
        let chip = Rc::new(GlassButton::pill(mtm, &cmd.title, ButtonSize::Regular));
        wire_button(chip.button(), sel!(chipClicked:));
        CHIP_POOL.with(|pool| {
            pool.borrow_mut().push(PooledChip {
                id: cmd.id.clone(),
                title: cmd.title.clone(),
                chip: Rc::clone(&chip),
            });
        });
        chip
    });
    // The title is the label. VoiceOver reads the detail as the hint.
    let help = (!cmd.detail.is_empty()).then(|| NSString::from_str(&cmd.detail));
    chip.button().setAccessibilityHelp(help.as_deref());
    chip
}

/// Drop pooled chips for commands the card no longer offers (`offered`), and zeroize their
/// titles: a history row's title is part of a copy.
fn prune_chip_pool(offered: &[&Command]) {
    CHIP_POOL.with(|pool| {
        pool.borrow_mut().retain_mut(|pooled| {
            let keep = offered
                .iter()
                .any(|cmd| cmd.id == pooled.id && cmd.title == pooled.title);
            if !keep {
                pooled.chip.view().removeFromSuperview();
                pooled.title.zeroize();
            }
            keep
        });
    });
}

/// Every pooled chip and the "no matches" note go; for Wipe.
fn drop_chip_pool() {
    prune_chip_pool(&[]);
    CHIPS.with(|slot| slot.borrow_mut().clear());
}

/// Show `chips` (in order, at `frames`) and hide the other pooled chips.
fn rebuild_pills(
    mtm: MainThreadMarker,
    list: &NSView,
    chips: Vec<Rc<GlassButton>>,
    frames: &[ChipFrame],
    show_empty: bool,
    text_width: f64,
) {
    NO_MATCHES.with(|slot| {
        let mut slot = slot.borrow_mut();
        if show_empty && slot.is_none() {
            let empty = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
            empty.setStringValue(&NSString::from_str("No matching commands"));
            list.addSubview(&empty);
            *slot = Some(empty);
        }
        if let Some(empty) = slot.as_ref() {
            empty.setFrame(NSRect::new(
                NSPoint::new(2.0, 4.0),
                NSSize::new(text_width, 20.0),
            ));
            empty.setHidden(!(show_empty && chips.is_empty()));
        }
    });
    let area_h = commands::chips_height(frames);
    let mut placed = Vec::with_capacity(chips.len());
    for (index, chip) in chips.into_iter().enumerate() {
        let Some(frame) = frames.get(index) else {
            continue;
        };
        let y = area_h - (frame.row as f64 + 1.0) * commands::CHIP_PITCH
            + (commands::CHIP_PITCH - commands::CHIP_PILL_H) / 2.0;
        chip.view().setFrame(NSRect::new(
            NSPoint::new(frame.x, y),
            NSSize::new(frame.width, commands::CHIP_PILL_H),
        ));
        chip.button().setTag(index as isize);
        // SAFETY: reading the view's parent on the main thread, only to compare it with `list`.
        let parent = unsafe { chip.view().superview() };
        if parent.is_none_or(|parent| !std::ptr::eq(&*parent, list)) {
            list.addSubview(chip.view());
        }
        chip.view().setHidden(false);
        placed.push(chip);
    }
    CHIP_POOL.with(|pool| {
        for pooled in pool.borrow().iter() {
            if !placed.iter().any(|chip| Rc::ptr_eq(chip, &pooled.chip)) {
                pooled.chip.view().setHidden(true);
                pooled.chip.button().setTag(-1);
            }
        }
    });
    CHIPS.with(|slot| *slot.borrow_mut() = placed);
}

/// Show the selected chip as the prominent one. The chip index is the `SELECTION` index.
fn paint_pills() {
    let selected = SELECTION.with(Cell::get);
    CHIPS.with(|slot| {
        for (index, chip) in slot.borrow().iter().enumerate() {
            chip.set_prominent(index == selected);
        }
    });
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

/// The arrow key of a field editor's move command (`moveLeft:` …).
fn arrow_command(command: Sel) -> Option<Key> {
    if command == sel!(moveLeft:) {
        Some(Key::Left)
    } else if command == sel!(moveRight:) {
        Some(Key::Right)
    } else if command == sel!(moveUp:) {
        Some(Key::Up)
    } else if command == sel!(moveDown:) {
        Some(Key::Down)
    } else {
        None
    }
}

/// ⌘← or ⌘→ (Shift, Control and Option up: ⇧⌘← still selects to the line start in a field).
fn command_arrow(event: &NSEvent) -> Option<Key> {
    let key = Key::from_key_code(event.keyCode())?;
    let flags = event.modifierFlags();
    let command = flags.contains(NSEventModifierFlags::Command)
        && !flags.intersects(
            NSEventModifierFlags::Shift
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option,
        );
    (command && matches!(key, Key::Left | Key::Right)).then_some(key)
}

/// Do what an arrow key does on the card ([`commands::arrow_action`]). Returns `false` when
/// the key is left to the text field, so its caret moves.
fn on_arrow(key: Key, command: bool, focus: commands::ArrowFocus) -> bool {
    let nav = HISTORY_NAV.with(|slot| *slot.borrow());
    match commands::arrow_action(key, command, focus, nav.is_some()) {
        None | Some(commands::ArrowAction::Text) => false,
        Some(commands::ArrowAction::Chip { dx, dy }) => {
            nudge(dx, dy);
            true
        }
        Some(commands::ArrowAction::History(chevron)) => {
            // Same as clicking the chevron. At 1 (‹) or N (›), or without history: a beep.
            if nav.is_some_and(|nav| chevron.enabled(&nav)) {
                launcher::emit(UserEvent::Run(chevron.command()));
            } else {
                NSBeep();
            }
            true
        }
    }
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
    if cmd.id == CommandId::TableMenu {
        pop_table_menu();
        return;
    }
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
    let mut saw_keep_history = false;
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
        if matches!(cmd.id, CommandId::ClearSensitive) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        if !saw_keep_history && matches!(cmd.id, CommandId::KeepHistory(_)) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            saw_keep_history = true;
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
        if let CommandId::KeepHistory(minutes) = cmd.id {
            item.setState(if HISTORY_MINUTES.with(Cell::get) == minutes {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
        if cmd.id == CommandId::ClearSensitive {
            item.setState(if CLEAR_SENSITIVE.with(Cell::get) {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
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
    let button = MORE.with(|slot| slot.borrow().as_ref().map(|more| more.view().retain()));
    let Some(button) = button else {
        return;
    };
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(0.0, 0.0), Some(&*button));
    focus_card();
}

fn on_key(event: &NSEvent) {
    if is_item_find_chord(event) {
        if item_find_on() {
            focus_item_field();
        }
        return;
    }
    match Key::from_key_code(event.keyCode()) {
        // A button or the reveal cover has the focus: no text caret to keep.
        Some(key @ (Key::Left | Key::Right | Key::Up | Key::Down)) => {
            let command = event
                .modifierFlags()
                .contains(NSEventModifierFlags::Command);
            on_arrow(key, command, commands::ArrowFocus::Card);
        }
        Some(Key::Return) => activate_selected(),
        Some(Key::Escape) => {
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
    PREVIEW_NOTE.with(|slot| slot.borrow_mut().clear());
    CARD_TITLE.with(wipe_string_slot);
    CARD_META.with(wipe_string_slot);
    CARD_EXCERPT.with(wipe_string_slot);
    CARD_PLACEHOLDER.with(wipe_string_slot);
    LINK_PAGE.with(|slot| set_secret_opt(slot, None));
    for slot in [&ACTIONS, &POOL, &OVERFLOW, &SHOWN] {
        slot.with(|slot| set_commands(slot, Vec::new()));
    }
    PAINTED.with(|slot| {
        if let Some((text, _, _)) = slot.borrow_mut().as_mut() {
            text.zeroize();
        }
        *slot.borrow_mut() = None;
    });
    PAINTED_ITEM.set(None);
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

fn wipe_shown_views() {
    FIND_CACHE.with(|slot| slot.borrow_mut().take());
    drop_chip_pool();
    PREVIEW_TEXT.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            widgets::wipe_text_view(view);
        }
    });
    set_item_text(String::new());
    PREVIEW_IMAGE.with(|slot| {
        if let Some(view) = slot.borrow().as_ref() {
            view.setImage(None);
        }
    });
    THUMB_TOKEN.set(-1);
    PICTURE.with(|slot| slot.replace(None));
    PICTURE_SHOWN.set(0);
    for slot in [&HEADER, &META, &FIELD, &ITEM_COUNT] {
        slot.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                widgets::wipe_text_field(field);
            }
        });
    }
    wipe_item_field();
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
    let font = NSFont::systemFontOfSize(12.0);
    let attr = AttrText::new(meta, &font, &NSColor::secondaryLabelColor());
    let warn_font = NSFont::boldSystemFontOfSize(12.0);
    for mark in crate::sensitivity::warning_marks(meta) {
        let range = mark.start..mark.end;
        let (ink, wash) = warning_colors(mark.label);
        attr.font(&range, &warn_font)
            .color(&range, &ink)
            .background(&range, &wash);
    }
    label.setAttributedStringValue(&attr.into_attributed());
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

/// The card title (kind, plus any check such as `missing }`) and the open hotkey.
fn header_title(kind: &str) -> String {
    format!("{kind} · {HOTKEY}")
}

/// Place a header button at `x` inside the header group (which is `HEADER_BUTTON` tall).
fn place_header_button(slot: &RefCell<Option<GlassButton>>, x: f64, width: f64) {
    let borrowed = slot.borrow();
    let Some(button) = borrowed.as_ref() else {
        return;
    };
    button.view().setFrame(NSRect::new(
        NSPoint::new(x, 0.0),
        NSSize::new(width, HEADER_BUTTON),
    ));
}

fn search_field(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = widgets::plain_field(mtm, 16.0, "Search");
    // Native rounded border, background and focus ring while the field is shown.
    field.setBezeled(true);
    field.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);
    field.setDrawsBackground(true);
    field.setFocusRingType(NSFocusRingType::Default);
    field.setAlphaValue(0.0);
    field.setAccessibilityLabel(Some(&NSString::from_str("Search commands")));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: LauncherDelegate implements controlTextDidChange: and
            // control:textView:doCommandBySelector: with their protocol signatures; DELEGATE keeps
            // it alive for the app's lifetime.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    field
}

fn item_search_field(mtm: MainThreadMarker) -> Retained<NSSearchField> {
    let field = widgets::search_field(mtm, 13.0, "Find");
    field.setAccessibilityLabel(Some(&NSString::from_str("Find in item")));
    field.setHidden(true);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: LauncherDelegate implements controlTextDidChange: and
            // control:textView:doCommandBySelector: with their protocol signatures; DELEGATE keeps
            // it alive for the app's lifetime.
            unsafe { widgets::set_text_delegate(&field, delegate) };
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
    let text = widgets::read_only_text_view(mtm, NSSize::new(12.0, 10.0));
    widgets::scroll_text_both_ways(&text);
    text.setAccessibilityLabel(Some(&NSString::from_str("Clipboard content")));
    text
}

fn text_scroll(mtm: MainThreadMarker, text: &NSTextView) -> Retained<NSScrollView> {
    let scroll = widgets::passive_text_scroll(mtm, text);
    scroll.setHidden(true);
    scroll.setAlphaValue(0.0);
    scroll
}

fn image_view(mtm: MainThreadMarker) -> Retained<NSImageView> {
    let view = widgets::image_view(mtm);
    view.setHidden(true);
    // Copied and dropped pictures show here.
    view.setAccessibilityLabel(Some(&NSString::from_str("Image preview")));
    view
}

type NavButton = GlassButton;

/// Show the history capsule at the right of the first chip row, or hide it (`None`: fewer than
/// two copies). The chevrons dim at the ends; the label shows the position.
fn place_history_nav(y: f64, nav: Option<commands::HistoryNav>) {
    NAV_CAPSULE.with(|slot| {
        let borrowed = slot.borrow();
        let Some(capsule) = borrowed.as_ref() else {
            return;
        };
        capsule.setHidden(nav.is_none());
        capsule.setFrame(NSRect::new(
            NSPoint::new(WIDTH - PAD - commands::NAV_SPAN, y),
            NSSize::new(commands::NAV_SPAN, commands::CHIP_PILL_H),
        ));
        if nav.is_some() {
            raise_view(capsule);
        }
    });
    let Some(nav) = nav else {
        return;
    };
    PREVIOUS.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            // A real NSButton draws its own disabled state and leaves the key-view loop.
            button
                .button()
                .setEnabled(commands::PREVIOUS_CHEVRON.enabled(&nav));
        }
    });
    NEXT.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button
                .button()
                .setEnabled(commands::NEXT_CHEVRON.enabled(&nav));
        }
    });
    NAV_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setStringValue(&NSString::from_str(&nav.label()));
            label.setAccessibilityLabel(Some(&NSString::from_str(&nav.spoken())));
        }
    });
}

fn raise_history_nav() {
    NAV_CAPSULE.with(|slot| {
        if let Some(capsule) = slot.borrow().as_ref()
            && !capsule.isHidden()
        {
            raise_view(capsule);
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
    // A preview keeps its last rows scrollable above the Show all pill.
    let previewing = PREVIEW_NOTE.with(|slot| !slot.borrow().is_empty()) && !well_is_masked();
    let bottom = if previewing {
        SHOW_ALL_H + WELL_INSET * 2.0
    } else {
        0.0
    };
    set_text_gutter(text_gutter(actions), bottom);
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
    button.view().setHidden(!shown);
    button.view().setFrame(NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(WELL_ACTION, WELL_ACTION),
    ));
    button.button().setToolTip(Some(&NSString::from_str(tip)));
    if shown {
        raise_view(button.view());
    }
}

fn raise_content_actions() {
    for slot in [&COPY_BUTTON, &SAVE_BUTTON] {
        slot.with(|slot| {
            if let Some(button) = slot.borrow().as_ref()
                && !button.view().isHidden()
            {
                raise_view(button.view());
            }
        });
    }
    SHOW_ALL.with(|slot| {
        if let Some(pill) = slot.borrow().as_ref()
            && !pill.button.view().isHidden()
        {
            raise_view(pill.button.view());
        }
    });
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref()
            && spinner.is_added()
        {
            raise_view(spinner.view());
        }
    });
}

/// "Show all" pill, with the preview note in its title.
struct ShowAllButton {
    button: GlassButton,
    title: String,
}

const SHOW_ALL_H: f64 = 24.0;

/// Show the "Show all" pill centered at the bottom of the well while it holds a preview and is
/// not blurred. The pill title carries the note, e.g. "Showing 200 of 23,220 rows · Show all".
fn place_show_all(mtm: MainThreadMarker, preview_y: f64) {
    let note = PREVIEW_NOTE.with(|slot| slot.borrow().clone());
    let shown = !note.is_empty() && !well_is_masked() && !SHOWS_IMAGE.with(Cell::get);
    let title = format!("{note}  ·  Show all");
    SHOW_ALL.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !shown {
            if let Some(pill) = slot.as_ref() {
                pill.button.view().setHidden(true);
            }
            return;
        }
        if slot.is_none() {
            let button = GlassButton::pill(mtm, &title, ButtonSize::Small);
            wire_button(button.button(), sel!(showAllClicked:));
            button
                .button()
                .setToolTip(Some(&NSString::from_str("Load and show the whole text")));
            let parent = WELL.with(|well| {
                well.borrow()
                    .as_ref()
                    // SAFETY: the superview is used right away, while the view hierarchy keeps
                    // it alive.
                    .and_then(|well| unsafe { well.superview() })
            });
            if let Some(parent) = parent {
                parent.addSubview(button.view());
            }
            *slot = Some(ShowAllButton {
                button,
                title: title.clone(),
            });
        }
        let Some(pill) = slot.as_mut() else {
            return;
        };
        if pill.title != title {
            pill.button.set_title(&title);
            pill.title = title;
        }
        let inner = WIDTH - PAD * 2.0 - WELL_INSET * 2.0;
        let width = pill.button.width_within(inner);
        pill.button.view().setFrame(NSRect::new(
            NSPoint::new(
                PAD + (WIDTH - PAD * 2.0 - width) / 2.0,
                preview_y + WELL_INSET,
            ),
            NSSize::new(width, SHOW_ALL_H),
        ));
        pill.button.view().setHidden(false);
        raise_view(pill.button.view());
    });
}

fn set_text_gutter(right: f64, bottom: f64) {
    let (old_right, old_bottom) = TEXT_GUTTER.get();
    if (old_right - right).abs() < 0.5 && (old_bottom - bottom).abs() < 0.5 {
        return;
    }
    TEXT_GUTTER.set((right, bottom));
    PREVIEW_SCROLL.with(|slot| {
        if let Some(scroll) = slot.borrow().as_ref() {
            scroll.setContentInsets(NSEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom,
                right,
            });
        }
    });
}

type WellAction = GlassButton;

fn well_action(
    mtm: MainThreadMarker,
    symbol: &str,
    label: &str,
    fallback: &str,
    action: Sel,
) -> WellAction {
    let button = GlassButton::symbol(mtm, symbol, label, fallback, 15.0);
    wire_button(button.button(), action);
    button.view().setHidden(true);
    button
}

/// Borderless chevron inside the history capsule: the SF Symbol only (or just the fallback
/// glyph), centred in its slot. The name ("Previous", "Next") is the VoiceOver label; the tooltip
/// and the VoiceOver hint name its arrow keys.
fn nav_button(mtm: MainThreadMarker, chevron: commands::Chevron) -> NavButton {
    let button = GlassButton::symbol(mtm, chevron.symbol, chevron.name, chevron.glyph, NAV_SYMBOL);
    let ns = button.button();
    // The capsule is the glass; a second glass bezel inside it would stack glass on glass.
    ns.setBordered(false);
    // `GlassButton::symbol` keeps the name as a hidden title. A borderless button draws it next
    // to the image, so the title must be empty (or only the fallback glyph).
    let has_image = ns.image().is_some();
    ns.setTitle(&NSString::from_str(chevron.title(has_image)));
    if has_image {
        ns.setImagePosition(NSCellImagePosition::ImageOnly);
    }
    ns.setAlignment(NSTextAlignment::Center);
    ns.setAccessibilityLabel(Some(&NSString::from_str(chevron.name)));
    // The keys: "Previous (← or ⌘←)", and VoiceOver's hint "Left Arrow, or Command-Left Arrow".
    ns.setToolTip(Some(&NSString::from_str(&chevron.tooltip())));
    ns.setAccessibilityHelp(Some(&NSString::from_str(&chevron.spoken_shortcut())));
    // Older counts up (`›`, next); newer counts down (`‹`, previous).
    let action = if chevron.command() == CommandId::HistoryOlder {
        sel!(olderClicked:)
    } else {
        sel!(newerClicked:)
    };
    wire_button(ns, action);
    button
}

/// The history capsule, hidden until there are two copies: Liquid Glass on macOS 26+ (frosted
/// before), fully rounded, with `‹` Previous and `›` Next chevrons around the position label in
/// tabular digits. Fixed size ([`commands::NAV_SPAN`]), so it never jumps while stepping.
fn history_capsule(
    mtm: MainThreadMarker,
) -> (
    Retained<NSView>,
    NavButton,
    Retained<NSTextField>,
    NavButton,
) {
    let height = commands::CHIP_PILL_H;
    let capsule = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(commands::NAV_SPAN, height),
        ),
    );
    capsule.setHidden(true);
    let inside = glass::background(mtm, &capsule, height / 2.0).content;
    let previous = nav_button(mtm, commands::PREVIOUS_CHEVRON);
    let next = nav_button(mtm, commands::NEXT_CHEVRON);
    let count = widgets::label(mtm, NAV_FONT, &NSColor::secondaryLabelColor());
    // Tabular digits: "1 / 9" and "2 / 9" are the same width. 0.0 is NSFontWeightRegular.
    count.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
        NAV_FONT, 0.0,
    )));
    count.setAlignment(NSTextAlignment::Center);
    count.setStringValue(&NSString::from_str("20 / 20"));
    let count_h = count.fittingSize().height.ceil();
    previous.view().setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    count.setFrame(NSRect::new(
        NSPoint::new(commands::NAV_BUTTON, ((height - count_h) / 2.0).floor()),
        NSSize::new(commands::NAV_COUNT_W, count_h),
    ));
    next.view().setFrame(NSRect::new(
        NSPoint::new(commands::NAV_BUTTON + commands::NAV_COUNT_W, 0.0),
        NSSize::new(commands::NAV_BUTTON, height),
    ));
    inside.addSubview(previous.view());
    inside.addSubview(&count);
    inside.addSubview(next.view());
    (capsule, previous, count, next)
}

/// Header symbol button (More, Close); `label` is what VoiceOver reads.
fn header_symbol(
    mtm: MainThreadMarker,
    symbol: &str,
    label: &str,
    fallback: &str,
    action: Sel,
) -> GlassButton {
    let button = GlassButton::symbol(mtm, symbol, label, fallback, 13.0);
    wire_button(button.button(), action);
    button
}

/// ⌘Z or ⇧⌘Z while the table has a version to go to ([`commands::undo_key`]). Typed search
/// text keeps the keys.
fn undo_chord(event: &NSEvent) -> Option<CommandId> {
    if SEARCHING.with(Cell::get) && !current_query().is_empty() {
        return None;
    }
    let flags = event.modifierFlags();
    let chars = event.charactersIgnoringModifiers()?.to_string();
    VERSION_BAR.with(|slot| {
        commands::undo_key(
            &chars,
            flags.contains(NSEventModifierFlags::Command),
            flags.contains(NSEventModifierFlags::Shift),
            flags.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option),
            slot.borrow().as_ref(),
        )
    })
}

/// The version bar: a glass capsule under the well with undo `↶`, the version shown (a button
/// that pops the list of versions) and redo `↷`.
struct VersionRow {
    capsule: Retained<NSView>,
    undo: GlassButton,
    title: GlassButton,
    redo: GlassButton,
}

/// Width of the undo and redo slots in the version bar.
const VERSION_BUTTON: f64 = 32.0;

fn version_row(mtm: MainThreadMarker) -> VersionRow {
    let height = VERSION_H;
    let width = WIDTH - PAD * 2.0;
    let capsule = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
    );
    capsule.setHidden(true);
    let inside = glass::background(mtm, &capsule, height / 2.0).content;
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
        ns.setAccessibilityLabel(Some(&NSString::from_str(name)));
        ns.setToolTip(Some(&NSString::from_str(&format!("{name} ({keys})"))));
        wire_button(ns, action);
        button
    };
    let undo = step_button(
        "arrow.uturn.backward",
        "Undo table step",
        "↶",
        "⌘Z",
        sel!(tableUndoClicked:),
    );
    let redo = step_button(
        "arrow.uturn.forward",
        "Redo table step",
        "↷",
        "⇧⌘Z",
        sel!(tableRedoClicked:),
    );
    let title = GlassButton::pill(mtm, "", ButtonSize::Small);
    title.button().setBordered(false);
    title
        .button()
        .setToolTip(Some(&NSString::from_str("Versions of the table")));
    wire_button(title.button(), sel!(versionsClicked:));
    undo.view().setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(VERSION_BUTTON, height),
    ));
    title.view().setFrame(NSRect::new(
        NSPoint::new(VERSION_BUTTON, 0.0),
        NSSize::new(width - VERSION_BUTTON * 2.0, height),
    ));
    redo.view().setFrame(NSRect::new(
        NSPoint::new(width - VERSION_BUTTON, 0.0),
        NSSize::new(VERSION_BUTTON, height),
    ));
    inside.addSubview(undo.view());
    inside.addSubview(title.view());
    inside.addSubview(redo.view());
    VersionRow {
        capsule,
        undo,
        title,
        redo,
    }
}

/// Show the version bar at `y` with `bar`, or hide it (`None`).
fn place_version_row(y: f64, bar: Option<&commands::VersionBar>) {
    VERSION_ROW.with(|slot| {
        let borrowed = slot.borrow();
        let Some(row) = borrowed.as_ref() else {
            return;
        };
        row.capsule.setHidden(bar.is_none());
        let Some(bar) = bar else {
            return;
        };
        row.capsule.setFrame(NSRect::new(
            NSPoint::new(PAD, y),
            NSSize::new(WIDTH - PAD * 2.0, VERSION_H),
        ));
        row.undo.button().setEnabled(bar.can_undo());
        row.redo.button().setEnabled(bar.can_redo());
        row.title.set_title(&bar.title());
        row.title.set_accessibility_label(&bar.spoken());
    });
}

/// Pop a menu of `items` (a check mark on the `true` ones) under `anchor`; a pick runs its
/// command like a chip.
fn pop_menu(items: Vec<(Command, bool)>, anchor: &NSView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    for (index, (cmd, checked)) in items.iter().enumerate() {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(&cmd.title),
                Some(sel!(poppedClicked:)),
                &NSString::from_str(""),
            )
        };
        item.setTag(index as isize);
        if *checked {
            item.setState(NSControlStateValueOn);
        }
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                unsafe { item.setTarget(Some(delegate)) };
            }
        });
        menu.addItem(&item);
    }
    POPPED.with(|slot| set_commands(slot, items.into_iter().map(|(cmd, _)| cmd).collect()));
    let below = NSPoint::new(0.0, anchor.frame().size.height + 4.0);
    menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(anchor));
    focus_card();
}

/// The "Table ▾" chip's menu: the steps, undo and redo.
fn pop_table_menu() {
    let items: Vec<(Command, bool)> = TABLE_MENU.with(|slot| {
        slot.borrow()
            .iter()
            .map(|cmd| (cmd.clone(), false))
            .collect()
    });
    let index = SHOWN.with(|slot| {
        slot.borrow()
            .iter()
            .position(|cmd| cmd.id == CommandId::TableMenu)
    });
    let chip = index.and_then(|index| CHIPS.with(|slot| slot.borrow().get(index).cloned()));
    match chip {
        Some(chip) => pop_menu(items, chip.view()),
        None => {
            let field = FIELD.with(|slot| slot.borrow().clone());
            if let Some(field) = field {
                pop_menu(items, &field);
            }
        }
    }
}

/// The version bar's menu: every version, the one shown checked.
fn pop_versions_menu() {
    let Some(bar) = VERSION_BAR.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let anchor =
        VERSION_ROW.with(|slot| slot.borrow().as_ref().map(|row| row.title.view().retain()));
    if let Some(anchor) = anchor {
        pop_menu(bar.menu(), &anchor);
    }
}

fn wire_button(button: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: every action wired here is a LauncherDelegate method taking the sender;
            // DELEGATE keeps it alive for the app's lifetime.
            unsafe { widgets::set_target_action(button, delegate, action) };
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
    fn header_title_is_the_kind_and_the_hotkey() {
        assert_eq!(super::header_title("Rust"), "Rust · ⌃⌥⌘C");
        assert_eq!(super::header_title("Clipboard"), "Clipboard · ⌃⌥⌘C");
        // Checks stay in the kind part; no date or time follows.
        let title = super::header_title("Java · missing }");
        assert_eq!(title, "Java · missing } · ⌃⌥⌘C");
        assert!(!title.chars().any(|ch| ch.is_ascii_digit()), "{title}");
    }

    #[test]
    fn well_radius_is_concentric_with_the_panel() {
        let inset = super::PAD;
        assert_eq!(
            super::WELL_RADIUS,
            mac_ui::corners::concentric_radius(super::PANEL_RADIUS, inset)
        );
        assert_eq!(super::WELL_RADIUS, mac_ui::corners::MIN_RADIUS);
    }

    #[test]
    fn text_and_pictures_get_their_own_blur() {
        // Light enough to show the shape, the picture's blur stronger than the text's.
        assert_eq!(super::TEXT_BLUR_RADIUS, 6.0);
        assert_eq!(super::IMAGE_BLUR_RADIUS, 10.0);
        assert!(super::gaussian_blurs().is_some());
    }

    #[test]
    fn blur_filters_are_built_once() {
        let (text, image) = super::gaussian_blurs().expect("blurs");
        let (again_text, again_image) = super::gaussian_blurs().expect("blurs");
        let ptr = super::Retained::as_ptr;
        assert_eq!(ptr(&text), ptr(&again_text));
        assert_eq!(ptr(&image), ptr(&again_image));
        assert_ne!(ptr(&text), ptr(&image));
    }

    #[test]
    fn blur_hook_fails_closed() {
        let _force = ForceBlurOff::arm();
        assert!(super::gaussian_blurs().is_none());
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
            theme: crate::appearance::Theme::System,
            settings: crate::settings::Settings::default(),
            view: crate::commands::CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
            full: false,
            picture: None,
            table: None,
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
            theme: crate::appearance::Theme::System,
            settings: crate::settings::Settings::default(),
            view: crate::commands::CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
            full: false,
            picture: None,
            table: None,
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
}
