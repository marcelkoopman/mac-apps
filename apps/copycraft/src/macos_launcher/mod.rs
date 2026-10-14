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
use mac_ui::objc2::runtime::{AnyClass, AnyObject, NSObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSAnimatablePropertyContainer, NSAnimationContext, NSApplication,
    NSApplicationDidResignActiveNotification, NSAttributedStringNSStringDrawing, NSBeep, NSBox,
    NSButton, NSColor, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSEvent,
    NSEventModifierFlags, NSFloatingWindowLevel, NSFocusRingType, NSFont, NSImage, NSImageView,
    NSLineBreakMode, NSMenu, NSMenuItem, NSMutableParagraphStyle, NSScrollView, NSSearchField,
    NSTextAlignment, NSTextField, NSTextFieldBezelStyle, NSTextView, NSView, NSWindow,
    NSWindowLevel, NSWindowOrderingMode, NSWindowSharingType,
};
use mac_ui::objc2_foundation::{
    NSArray, NSEdgeInsets, NSMouseInRect, NSMutableAttributedString, NSNotification,
    NSNotificationCenter, NSPoint, NSRange, NSRect, NSSize, NSString,
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

pub(crate) const WIDTH: f64 = 440.0;
/// The diff well is two columns, so the card grows while that view is showing.
pub(crate) const DIFF_WIDTH: f64 = 820.0;
use crate::card_layout::{
    HEADER_H, ITEM_FIND_H, PAD, PREVIEW_H, SEARCH_H, meta_block_height, place_sections,
};
pub(crate) const WELL_ACTION: f64 = mac_ui::metrics::BUTTON_H;
pub(crate) const WELL_INSET: f64 = mac_ui::metrics::SPACE_S;
pub(crate) const WELL_GAP: f64 = mac_ui::metrics::BUTTON_GAP;
pub(crate) const ITEM_COUNT_W: f64 = 56.0;
/// Gaussian blur (`CIGaussianBlur` radius, in points) over masked text in the well. The privacy
/// tradeoff: the blur should show the shape of what was copied (how many lines, their indentation
/// and length, where code blocks are) so you recognise it, but never the text. At 6 pt each glyph
/// of the 12 pt monospace payload (7 pt wide, 15 pt lines) is spread over about ±12 pt, so letters
/// and words run together into one grey band per line: unreadable, also for digits and short
/// words, while lines and indentation still show. Below about 4 pt short words and numbers start
/// to come through.
pub(crate) const TEXT_BLUR_RADIUS: f64 = 6.0;
/// The blur over a masked picture. Stronger than [`TEXT_BLUR_RADIUS`]: a picture is scaled to the
/// well, so text inside it (a screenshot, a photographed card or document) can be much larger
/// than 12 pt. At 10 pt the outline, the main shapes and colours stay recognisable, large text
/// does not.
pub(crate) const IMAGE_BLUR_RADIUS: f64 = 10.0;
const _: () = assert!(TEXT_BLUR_RADIUS >= 5.0 && TEXT_BLUR_RADIUS < IMAGE_BLUR_RADIUS);
/// Glass controls this close merge on macOS 26+ (`glass::group`). Below the button gap, so
/// neighbours only merge while they morph closer together.
pub(crate) const GLASS_MERGE: f64 = 4.0;
/// History chevron symbol size and the position label's font size.
pub(crate) const NAV_SYMBOL: f64 = mac_ui::metrics::FONT_CAPTION;
pub(crate) const NAV_FONT: f64 = mac_ui::metrics::FONT_CAPTION;
/// Corner radius of the launcher panel, its glass and the frosted fallback. Fixed:
/// objc2-app-kit 0.3.2 has no public API for the system window corner radius on macOS 26+
/// (`NSViewCornerConfiguration` is not bound), so the panel keeps the shared radius.
pub(crate) const PANEL_RADIUS: f64 = mac_ui::metrics::PANEL_RADIUS;
/// The well uses the shared 8 pt radius. It shares the window margin with the search field,
/// so the radius is not `concentric_radius(PANEL_RADIUS, PAD)` (that would clamp to 4).
pub(crate) const WELL_RADIUS: f64 = mac_ui::metrics::WELL_RADIUS;
/// What the card takes when it is dropped on it: one text file of any kind (plain, source code,
/// JSON, XML, CSV, YAML, Markdown, … all conform to `public.text`) or picture (`public.image`:
/// PNG, JPEG, HEIC, GIF, WebP, TIFF, …), also as a promised file (Photos); image data (a picture
/// dragged from Safari or Preview); or dropped text. Not PDFs, folders or several files.
pub(crate) const DROP_ACCEPT: mac_ui::drop::Accept = mac_ui::drop::Accept {
    file_types: &["public.text", "public.image"],
    images: true,
    text: true,
};
/// The drop outline sits this far inside the panel edge, concentric with it.
pub(crate) const DROP_INSET: f64 = 4.0;

/// What the well was last painted with: the text, its highlight, whether it was the excerpt
/// (not a placeholder), and whether it was a line diff (green and red lines).
pub(crate) struct Painted {
    pub(crate) text: String,
    pub(crate) highlight: Option<FormatKind>,
    pub(crate) payload: bool,
    pub(crate) diff_lines: bool,
}

pub(crate) fn drop_highlight() -> mac_ui::drop::Highlight {
    mac_ui::drop::Highlight {
        inset: DROP_INSET,
        corner_radius: corners::concentric_radius(PANEL_RADIUS, DROP_INSET),
        announcement: crate::locale::t("drop_to_open"),
    }
}

thread_local! {
    /// The hide button masked the entry on screen. Forgotten when another entry shows.
    static HIDDEN: Cell<bool> = const { Cell::new(false) };
    /// Hide/show on the well.
    static HIDE_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    /// Line numbers on/off on a plain text card, left of the eye.
    static NUMBERS_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    /// The card on screen can show line numbers ([`commands::offers_line_numbers`]).
    static OFFERS_NUMBERS: Cell<bool> = const { Cell::new(false) };
    /// The mouse is over the well's right edge: the action dock is shown.
    static DOCK_HOVERED: Cell<bool> = const { Cell::new(false) };
    /// Top of the well in window coordinates ([`place_content_actions`]).
    static WELL_TOP: Cell<f64> = const { Cell::new(0.0) };
    /// Bottom of the well in window coordinates.
    static WELL_BOTTOM: Cell<f64> = const { Cell::new(0.0) };
    /// The eye button's choice per item, kept in memory ([`commands::BlurChoices`]).
    static BLUR_CHOICES: RefCell<commands::BlurChoices> = RefCell::new(commands::BlurChoices::default());
}

thread_local! {
    /// The card on screen is the "Working…" placeholder of a card still being built.
    static PLACEHOLDER: Cell<bool> = const { Cell::new(false) };
    /// Height of the last real card, kept for the placeholder so the window does not jump.
    static LAST_HEIGHT: Cell<Option<f64>> = const { Cell::new(None) };
}

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
    static PAUSED: Cell<bool> = const { Cell::new(false) };
    static IGNORE_NEXT: Cell<bool> = const { Cell::new(false) };
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
    /// The line of copied JSON that does not parse ([`commands::WorkCard::error_line`]).
    static CARD_ERROR_LINE: Cell<Option<usize>> = const { Cell::new(None) };
    /// The error line the well was last painted with (with [`PAINTED`]).
    static PAINTED_ERROR_LINE: Cell<Option<usize>> = const { Cell::new(None) };
    static PAINTED: RefCell<Option<Painted>> = const { RefCell::new(None) };
    static LINK_PAGE: RefCell<Option<String>> = const { RefCell::new(None) };
    static WINDOW: RefCell<Option<Retained<LauncherWindow>>> = const { RefCell::new(None) };
    static FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static HEADER: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static META: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static WELL: RefCell<Option<Retained<NSBox>>> = const { RefCell::new(None) };
    static PREVIEW_TEXT: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
    static PREVIEW_SCROLL: RefCell<Option<Retained<NSScrollView>>> = const { RefCell::new(None) };
    /// Parent of [`PREVIEW_SCROLL`]: clips the tall floating first column. Clipping on the
    /// scroller itself breaks trackpad/wheel scrolling.
    static PREVIEW_CLIP: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
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
    /// "Tekens" / "Symbols", left of Wipe. Opens the character popover.
    static SYMBOLS: RefCell<Option<GlassButton>> = const { RefCell::new(None) };
    static CONTENT_ACTIONS: Cell<commands::ContentActions> = const {
        Cell::new(commands::ContentActions {
            copy: false,
            save: false,
            preview: false,
        })
    };
    /// Right and bottom insets of the text: room for the Copy/Save/Preview buttons and the Show all pill.
    static TEXT_GUTTER: Cell<(f64, f64)> = const { Cell::new((-1.0, -1.0)) };
    static COPY_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    static SAVE_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    static PREVIEW_BUTTON: RefCell<Option<WellAction>> = const { RefCell::new(None) };
    /// Busy wheel over the well while a save runs. Created on first use.
    static SPINNER: RefCell<Option<progress::Spinner>> = const { RefCell::new(None) };
    /// "Showing 200 of 23,220 rows" when the well holds a preview. Empty otherwise.
    static PREVIEW_NOTE: RefCell<String> = const { RefCell::new(String::new()) };
    /// One-line empty state, centred in the well.
    static EMPTY_LINE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
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
    /// The well is a side-by-side diff, so the columns are drawn and `+` / `-` sides are colored.
    static CARD_DIFF: Cell<bool> = const { Cell::new(false) };
    /// The diff well is showing both copies, so the card uses [`DIFF_WIDTH`].
    static DIFF_WIDE: Cell<bool> = const { Cell::new(false) };
    /// A diff stays masked until reveal when either side is sensitive, even with the privacy
    /// filter off. Other views keep the filter as the master switch.
    static DIFF_LOCK: Cell<bool> = const { Cell::new(false) };
    static CONTENT_KEY: Cell<u64> = const { Cell::new(0) };
    static DELEGATE: RefCell<Option<Retained<LauncherDelegate>>> = const { RefCell::new(None) };
    /// Our open panel is up. It can resign the app; a revealed card stays revealed.
    static FILE_PANEL: Cell<bool> = const { Cell::new(false) };
    /// The table's version capsule ([`commands::VersionBar`]); `None` hides it.
    static VERSION_BAR: RefCell<Option<commands::VersionBar>> = const { RefCell::new(None) };
    /// The version capsule's views.
    static VERSION_ROW: RefCell<Option<VersionRow>> = const { RefCell::new(None) };
    /// The "Table ▾" menu's commands ([`commands::table_menu`]).
    static TABLE_MENU: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// Which of [`TABLE_MENU`]'s items have a check mark.
    static TABLE_MENU_CHECKS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
    /// The commands of the menu popped last (table steps or versions), by item tag.
    static POPPED: RefCell<Vec<Command>> = const { RefCell::new(Vec::new()) };
    /// The window's one field editor, made on first use. It takes no drops (see
    /// `windowWillReturnFieldEditor:toObject:`).
    static FIELD_EDITOR: RefCell<Option<Retained<NSTextView>>> = const { RefCell::new(None) };
}

pub(crate) fn card_width() -> f64 {
    if DIFF_WIDE.get() { DIFF_WIDTH } else { WIDTH }
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

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, _event: &NSEvent) {
            update_well_dock_hover();
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // Tab and Shift-Tab from a focused button or the reveal cover reach the window.
            // NSWindow's own keyDown: moves along the key-view loop. Without it, Tab would stop
            // on the first button under Full Keyboard Access.
            if Key::from_key_code(event.keyCode()) == Some(Key::Tab) {
                // SAFETY: forwards the event to the superclass implementation of the same selector,
                // with the same argument and return types.
                let _: () = unsafe { msg_send![super(self), keyDown: event] };
                return;
            }
            on_key(event);
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if let Some(handled) = picker_key(event) {
                // The column picker: ⌘A, Return and Esc are its own while it is open.
                handled
            } else if picker_open() {
                // The card's chords (history, versions, find) wait until it is closed.
                // SAFETY: forwards the event to the superclass implementation of the same selector,
                // with the same argument and return types.
                let handled: bool =
                    unsafe { msg_send![super(self), performKeyEquivalent: event] };
                handled
            } else if is_item_find_chord(event) {
                if item_find_on() {
                    focus_item_field();
                }
                true
            } else if mac_ui::keys::is_command_chord(event, "o") {
                // ⌘O opens a file, also while a search field has the caret.
                launcher::emit(UserEvent::Run(CommandId::ChooseFile));
                true
            } else if mac_ui::keys::is_command_chord(event, "s") {
                // ⌘S saves what the card shows, also while a search field has the caret.
                launcher::emit(UserEvent::Run(CommandId::Save));
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
            if note_is_picker_field(note) {
                picker_query_changed();
                return;
            }
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
            if control_is_picker_field(control) {
                picker_field_command(command)
            } else if control_is_item_field(control) {
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

        #[unsafe(method(symbolsClicked:))]
        fn symbols_clicked(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            crate::macos_symbols::toggle(mtm, button);
        }

        #[unsafe(method(closeClicked:))]
        fn close_clicked(&self, _sender: Option<&NSButton>) {
            hide();
        }

        #[unsafe(method(clearClicked:))]
        fn clear_clicked(&self, _sender: Option<&NSButton>) {
            if crate::settings::confirm_clear()
                && let Some(mtm) = MainThreadMarker::new()
            {
                let t = crate::locale::t;
                let answer = mac_ui::dialog::confirm_destructive(
                    mtm,
                    t("clear_confirm_title"),
                    t("clear_confirm_message"),
                    t("clear_confirm_action"),
                    t("cancel"),
                    Some(t("clear_dont_ask")),
                );
                if answer.dont_ask_again {
                    crate::settings::set_confirm_clear(false);
                }
                if !answer.confirmed {
                    focus_card();
                    return;
                }
            }
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

        #[unsafe(method(previewClicked:))]
        fn preview_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::OpenPreview));
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
            if !well_is_masked() {
                return;
            }
            // Not before the labels are known: the meta line says "Checking…" meanwhile.
            if crate::sensitivity::meta_status(&card_meta()) == Some(crate::sensitivity::checking()) {
                NSBeep();
                return;
            }
            HIDDEN.set(false);
            REVEALED.set(true);
            layout(false);
        }

        #[unsafe(method(numbersClicked:))]
        fn numbers_clicked(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::ToggleLineNumbers));
        }

        #[unsafe(method(hideClicked:))]
        fn hide_clicked(&self, _sender: Option<&NSButton>) {
            toggle_hidden();
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
            let title = item.title().to_string();
            let cmd =
                POPPED.with(|slot| commands::popped_pick(&slot.borrow(), item.tag(), &title));
            if let Some(cmd) = cmd {
                run_command(cmd);
            }
        }

        #[unsafe(method(pickerToggled:))]
        fn picker_toggled(&self, sender: Option<&NSButton>) {
            let Some(check) = sender else {
                return;
            };
            let index = check.tag();
            if index < 0 {
                return;
            }
            let kept = check.state() == NSControlStateValueOn;
            picker_update(|picker| picker.set_kept(index as usize, kept));
        }

        #[unsafe(method(pickerAllClicked:))]
        fn picker_all_clicked(&self, _sender: Option<&NSButton>) {
            picker_update(ColumnPicker::keep_all);
        }

        #[unsafe(method(pickerNoneClicked:))]
        fn picker_none_clicked(&self, _sender: Option<&NSButton>) {
            picker_update(ColumnPicker::keep_none);
        }

        #[unsafe(method(pickerBackdropClicked:))]
        fn picker_backdrop_clicked(&self, _sender: Option<&NSButton>) {
            picker_backdrop_clicked();
        }

        #[unsafe(method(pickerCancelClicked:))]
        fn picker_cancel_clicked(&self, _sender: Option<&NSButton>) {
            close_picker();
        }

        #[unsafe(method(pickerApplyClicked:))]
        fn picker_apply_clicked(&self, _sender: Option<&NSButton>) {
            apply_picker();
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
        // SAFETY: `init` on a freshly allocated instance of a class that declares no other
        // initializer.
        unsafe { msg_send![this, init] }
    }
}

mod api;
mod complexity;
mod controls;
mod events;
mod find;
mod pager;
mod panels;
mod window;

pub use api::*;
pub use complexity::*;
use controls::*;
pub use events::*;
use find::*;
pub(crate) use pager::*;
pub use panels::*;
pub use window::*;

#[cfg(test)]
mod tests;

include!("../macos_column_picker.rs");
include!("../macos_frozen_column.rs");
