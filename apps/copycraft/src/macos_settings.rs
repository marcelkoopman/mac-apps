//! Settings window, in five tabs (a segmented control above one pane per tab):
//! General (hotkey, language, date order, Open at Login), Privacy (Keep clipboard history with
//! retention, sensitive-copy clearing and ask before clearing under it, privacy filter, blur,
//! ignored apps), Display (card theme, pages, announce, symbols),
//! Notifications (four switches) and Feedback (mail, privacy policy, support). ⌘1…⌘5 pick a tab;
//! the last tab is remembered for the session only.

use std::cell::{Cell, RefCell};

use global_hotkey::hotkey::{HotKey, Modifiers};
use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSBox, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventModifierFlags, NSMenuItem, NSPopUpButton,
    NSSegmentDistribution, NSSegmentSwitchTracking, NSSegmentedControl, NSTextField, NSView,
    NSWindow, NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use mac_ui::{panel, widgets};

use crate::commands::CommandId;
use crate::hotkey;
use crate::launcher::{self, UserEvent};
use crate::macos_login;
use crate::settings;

const WIDTH: f64 = 480.0;
const MARGIN: f64 = mac_ui::metrics::MARGIN;
const BUTTON_H: f64 = mac_ui::metrics::BUTTON_H;
const NOTE_GAP: f64 = 2.0;
const NOTE_H: f64 = 16.0;
/// One control plus its one-line explanation.
const SETTING_H: f64 = BUTTON_H + NOTE_GAP + NOTE_H;
/// A labelled setting with a field and a button under the explanation.
const FIELD_BLOCK_H: f64 =
    SETTING_H + mac_ui::metrics::SPACE_S + BUTTON_H + mac_ui::metrics::SPACE_S + BUTTON_H;
const SECTION_GAP: f64 = mac_ui::metrics::SECTION_GAP;
/// Width of the name on a row that also holds a value (hotkey, language, date order).
const LABEL_W: f64 = 136.0;
/// The tab bar: one segmented control across the top of the window.
const TAB_BAR_H: f64 = BUTTON_H;
/// Algemeen: four settings, three gaps.
const GENERAL_H: f64 = 4.0 * SETTING_H + 3.0 * mac_ui::metrics::SPACE_S;
/// How far a setting that depends on the one above it is indented.
const INDENT: f64 = 20.0;
/// The settings under "Keep clipboard history": retention, sensitive copies and ask before
/// clearing, one row each without a note (the note is the tooltip), after a gap.
const HISTORY_SUB_H: f64 =
    mac_ui::metrics::SPACE_S + 3.0 * BUTTON_H + 2.0 * mac_ui::style::SPACE_XS;
/// Privacy: three settings ("Keep clipboard history", the privacy filter, blur), the rows under
/// history, the ignored-apps block, three gaps.
const PRIVACY_H: f64 =
    3.0 * SETTING_H + HISTORY_SUB_H + 3.0 * mac_ui::metrics::SPACE_S + FIELD_BLOCK_H;
/// The symbols on Weergave: the labelled setting, then the field with Restore on the same row.
const FIELD_ROW_BLOCK_H: f64 = SETTING_H + mac_ui::metrics::SPACE_S + BUTTON_H;
/// Weergave: the card theme row (popup and preview, no note), the pages and announce settings,
/// three gaps and the symbols block.
const DISPLAY_H: f64 =
    BUTTON_H + 2.0 * SETTING_H + 3.0 * mac_ui::metrics::SPACE_S + FIELD_ROW_BLOCK_H;
/// Width of the card theme popup; the preview takes the rest of the row.
const THEME_POPUP_W: f64 = 176.0;
/// Meldingen: the four switches in two rows of two, one gap and a note.
const NOTIFY_H: f64 = 2.0 * BUTTON_H + mac_ui::metrics::SPACE_S + NOTE_GAP + NOTE_H;
/// Feedback: four buttons stacked one per row: the two mail buttons, a wider gap, then the
/// privacy policy and support links.
const FEEDBACK_H: f64 =
    4.0 * BUTTON_H + 2.0 * mac_ui::metrics::SPACE_S + mac_ui::metrics::SECTION_GAP;
/// Width of every Feedback button: the widest title, capped at half the window.
const FEEDBACK_BUTTON_MAX_W: f64 = WIDTH / 2.0;
/// Every tab gets the height of the tallest one, so the window keeps one size.
const PANE_H: f64 = max(
    max(max(GENERAL_H, PRIVACY_H), max(DISPLAY_H, NOTIFY_H)),
    FEEDBACK_H,
);
const HEIGHT: f64 = MARGIN + TAB_BAR_H + SECTION_GAP + PANE_H + MARGIN;
const RADIUS: f64 = mac_ui::metrics::PANEL_RADIUS;

const fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

const _: () = assert!(SETTING_H == 46.0);
const _: () = assert!(FIELD_BLOCK_H == 118.0);
const _: () = assert!(FIELD_ROW_BLOCK_H == 82.0);
const _: () = assert!(GENERAL_H == 208.0);
const _: () = assert!(HISTORY_SUB_H == 100.0);
const _: () = assert!(PRIVACY_H == 380.0);
const _: () = assert!(DISPLAY_H == 226.0);
const _: () = assert!(NOTIFY_H == 82.0);
const _: () = assert!(FEEDBACK_H == 144.0);
const _: () = assert!(PANE_H == 380.0);
/// Fits a 13" MacBook Air (about 560 pt for a window) with room to spare.
const _: () = assert!(HEIGHT == 456.0);

/// The tabs of the window, in the order of the segments.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    General,
    Privacy,
    Display,
    Notifications,
    Feedback,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::General,
        Tab::Privacy,
        Tab::Display,
        Tab::Notifications,
        Tab::Feedback,
    ];

    fn index(self) -> usize {
        Tab::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    fn at(index: usize) -> Option<Tab> {
        Tab::ALL.get(index).copied()
    }

    /// The segment title (literal keys, for the locale scan test).
    fn title(self) -> &'static str {
        match self {
            Tab::General => crate::locale::t("group_general"),
            Tab::Privacy => crate::locale::t("group_privacy"),
            Tab::Display => crate::locale::t("group_display"),
            Tab::Notifications => crate::locale::t("group_notifications"),
            Tab::Feedback => crate::locale::t("group_feedback"),
        }
    }
}

/// The notification switches in the order of their checkboxes (row by row), with the title key.
/// The checkbox's tag is the index.
const NOTIFY_SWITCHES: [crate::notifications::Switch; 4] = [
    crate::notifications::Switch::Ai,
    crate::notifications::Switch::Ocr,
    crate::notifications::Switch::Errors,
    crate::notifications::Switch::Secret,
];

/// The checkbox title of a notification switch (literal keys, for the locale scan test).
fn notify_title(switch: crate::notifications::Switch) -> &'static str {
    use crate::notifications::Switch;
    match switch {
        Switch::Ai => crate::locale::t("notify_switch_ai"),
        Switch::Ocr => crate::locale::t("notify_switch_ocr"),
        Switch::Errors => crate::locale::t("notify_switch_errors"),
        Switch::Secret => crate::locale::t("notify_switch_secret"),
    }
}

thread_local! {
    static WINDOW: RefCell<Option<Retained<SettingsWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<SettingsDelegate>>> = const { RefCell::new(None) };
    static HOTKEY_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static DATE_POPUP: RefCell<Option<Retained<NSPopUpButton>>> = const { RefCell::new(None) };
    static THEME_POPUP: RefCell<Option<Retained<NSPopUpButton>>> = const { RefCell::new(None) };
    /// The card theme preview: its plate and the sample text on it.
    static THEME_PLATE: RefCell<Option<Retained<NSBox>>> = const { RefCell::new(None) };
    static THEME_SAMPLE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static PRIVACY_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static HISTORY_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static BLUR_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    /// Under "Keep clipboard history": the retention name and popup, sensitive copies, ask
    /// before clearing.
    static RETENTION_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static RETENTION_POPUP: RefCell<Option<Retained<NSPopUpButton>>> =
        const { RefCell::new(None) };
    static SENSITIVE_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static ASK_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static SYMBOLS_FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static IGNORED_FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static IGNORED_ADD: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static TABS: RefCell<Option<Retained<NSSegmentedControl>>> = const { RefCell::new(None) };
    /// One view per [`Tab`], in [`Tab::ALL`] order; only the selected one is visible.
    static PANES: RefCell<Vec<Retained<NSView>>> = const { RefCell::new(Vec::new()) };
    /// Index of the last selected tab; kept after closing, for this session only.
    static SELECTED_TAB: Cell<usize> = const { Cell::new(0) };
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    /// Level before [`crate::macos_launcher::raise_above_card`], restored on close.
    static LEVEL_BEFORE: Cell<Option<NSWindowLevel>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftSettingsWindow"]
    struct SettingsWindow;

    impl SettingsWindow {
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if RECORDING.get() {
                handle_record_key(event);
                return;
            }
            // SAFETY: forwards the event to the superclass implementation of the same selector,
            // with the same argument and return types.
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            // ⌘1…⌘5 pick a tab, unless the hotkey field is listening.
            let tab = if RECORDING.get() { None } else { tab_for_chord(event) };
            let open = mac_ui::keys::is_command_chord(event, "o");
            let save = mac_ui::keys::is_command_chord(event, "s");
            if let Some(tab) = tab {
                select_tab(tab.index());
                true
            } else if open || save {
                if RECORDING.get() {
                    // The hotkey field is listening: ⌘O and ⌘S are chords, not Open or Save.
                    handle_record_key(event);
                } else if open {
                    launcher::emit(UserEvent::Run(CommandId::ChooseFile));
                } else {
                    launcher::emit(UserEvent::Run(CommandId::Save));
                }
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

struct DelegateIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftSettingsDelegate"]
    #[ivars = DelegateIvars]
    struct SettingsDelegate;

    // SAFETY: NSObject conforms to NSObjectProtocol and this subclass changes nothing about it.
    unsafe impl NSObjectProtocol for SettingsDelegate {}

    // SAFETY: the methods below use the NSWindowDelegate selectors and signatures.
    unsafe impl NSWindowDelegate for SettingsDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSObject) {
            // Closing ends editing, but save here too so a list left in the field is kept.
            save_symbols_field();
            save_ignored_field();
            RECORDING.set(false);
            WINDOW.with(|slot| {
                if let Some(window) = slot.borrow().as_ref() {
                    restore_level(window);
                }
                *slot.borrow_mut() = None;
            });
            clear_controls();
        }
    }

    impl SettingsDelegate {
        #[unsafe(method(changeHotkey:))]
        fn change_hotkey(&self, _sender: Option<&NSButton>) {
            RECORDING.set(true);
            set_hotkey_text(crate::locale::t("press_shortcut"));
            WINDOW.with(|slot| {
                if let Some(window) = slot.borrow().as_ref() {
                    window.makeKeyAndOrderFront(None);
                }
            });
        }

        #[unsafe(method(tabChanged:))]
        fn tab_changed(&self, sender: Option<&NSSegmentedControl>) {
            let Some(control) = sender else {
                return;
            };
            if let Ok(index) = usize::try_from(control.selectedSegment()) {
                select_tab(index);
            }
        }

        #[unsafe(method(dateOrderChanged:))]
        fn date_order_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else {
                return;
            };
            settings::set_date_month_first(popup.indexOfSelectedItem() == 1);
            launcher::emit(UserEvent::SettingsChanged);
        }

        /// Card theme: store it, repaint the preview, and restyle an open card right away.
        #[unsafe(method(cardThemeChanged:))]
        fn card_theme_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else {
                return;
            };
            // The tag is the theme's place in `CardTheme::ALL`; separators sit between groups.
            let Some(item) = popup.selectedItem() else {
                return;
            };
            let index = usize::try_from(item.tag()).unwrap_or(0);
            let theme = crate::card_theme::CardTheme::at(index);
            if theme == settings::card_theme() {
                return;
            }
            settings::set_card_theme(theme);
            paint_theme_preview();
            crate::macos_launcher::apply_card_theme();
        }

        #[unsafe(method(languageChanged:))]
        fn language_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else {
                return;
            };
            let choice = match popup.indexOfSelectedItem() {
                1 => settings::Language::Nl,
                2 => settings::Language::En,
                _ => settings::Language::System,
            };
            if choice == settings::language() {
                return;
            }
            settings::set_language(choice);
            crate::locale::apply();
            launcher::emit(UserEvent::LanguageChanged);
        }

        /// "Keep clipboard history": watch the clipboard, or only take what is pasted or dropped.
        #[unsafe(method(clipboardHistoryToggled:))]
        fn clipboard_history_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_clipboard_history(button.state() == NSControlStateValueOn);
            refresh_history_controls();
            launcher::emit(UserEvent::ClipboardHistoryChanged);
        }

        #[unsafe(method(privacyFilterToggled:))]
        fn privacy_filter_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            settings::set_privacy_filter(on);
            BLUR_BOX.with(|slot| {
                if let Some(blur) = slot.borrow().as_ref() {
                    blur.setEnabled(on);
                }
            });
            // The open card and table window mask (on) or unmask (off) at once.
            launcher::emit(UserEvent::PrivacyFilterChanged);
        }

        #[unsafe(method(pageLongTextToggled:))]
        fn page_long_text_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_page_long_text(button.state() == NSControlStateValueOn);
            launcher::emit(UserEvent::SettingsChanged);
        }

        #[unsafe(method(announceToggled:))]
        fn announce_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_announce_confirmations(button.state() == NSControlStateValueOn);
        }

        /// Retention: the same choice as ⋯ › Keep History, run as that command so the app's
        /// timer follows.
        #[unsafe(method(retentionChanged:))]
        fn retention_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else {
                return;
            };
            let Some(minutes) = usize::try_from(popup.indexOfSelectedItem())
                .ok()
                .and_then(|index| settings::HISTORY_MINUTES.get(index))
            else {
                return;
            };
            if *minutes != settings::load().history_minutes {
                launcher::emit(UserEvent::Run(CommandId::KeepHistory(*minutes)));
            }
        }

        /// Clear sensitive copies after 60 s: the ⋯ menu's toggle, so a running timer stops too.
        #[unsafe(method(clearSensitiveToggled:))]
        fn clear_sensitive_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            if (button.state() == NSControlStateValueOn) != settings::load().clear_sensitive {
                launcher::emit(UserEvent::Run(CommandId::ClearSensitive));
            }
        }

        #[unsafe(method(confirmClearToggled:))]
        fn confirm_clear_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_confirm_clear(button.state() == NSControlStateValueOn);
        }

        /// One of the four notification switches; its tag is the index in [`NOTIFY_SWITCHES`].
        #[unsafe(method(notifyToggled:))]
        fn notify_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let Some(switch) = usize::try_from(button.tag())
                .ok()
                .and_then(|index| NOTIFY_SWITCHES.get(index))
            else {
                return;
            };
            settings::set_notify(*switch, button.state() == NSControlStateValueOn);
        }

        #[unsafe(method(blurToggled:))]
        fn blur_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_blur(button.state() == NSControlStateValueOn);
            launcher::emit(UserEvent::SettingsChanged);
        }

        #[unsafe(method(controlTextDidEndEditing:))]
        fn symbols_edited(&self, _notification: &NSNotification) {
            save_symbols_field();
            save_ignored_field();
        }

        #[unsafe(method(suggestFeature:))]
        fn suggest_feature(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::SuggestFeature));
        }

        #[unsafe(method(reportProblem:))]
        fn report_problem(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::ReportProblem));
        }

        #[unsafe(method(openPrivacy:))]
        fn open_privacy(&self, _sender: Option<&NSButton>) {
            crate::macos_about::open_page(crate::url_policy::PRIVACY_URL);
        }

        #[unsafe(method(openSupport:))]
        fn open_support(&self, _sender: Option<&NSButton>) {
            crate::macos_about::open_page(crate::url_policy::SUPPORT_URL);
        }

        #[unsafe(method(restoreSymbols:))]
        fn restore_symbols(&self, _sender: Option<&NSButton>) {
            settings::restore_symbol_catalog();
            show_symbol_catalog();
            crate::macos_symbols::reload();
        }

        #[unsafe(method(addFrontmostApp:))]
        fn add_frontmost(&self, _sender: Option<&NSButton>) {
            // Keep an id the user just typed, then append the last other frontmost app.
            save_ignored_field();
            if let Some(bundle) = crate::capture::last_foreign() {
                let _ = settings::ignore_app(&bundle);
            }
            refresh_ignored_field();
        }

        #[unsafe(method(loginToggled:))]
        fn login_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            if let Err(message) = macos_login::set_enabled(on) {
                crate::diag::log(crate::diag::LogEvent::LoginItemFailed(crate::diag::ErrorKind::from_text(&message)));
            }
            refresh_login_row();
        }
    }
);

impl SettingsDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars);
        // SAFETY: NSObject's `init` on the freshly allocated instance; the class declares no other
        // initializer.
        unsafe { msg_send![super(this), init] }
    }
}

fn clear_controls() {
    TABS.with(|s| *s.borrow_mut() = None);
    PANES.with(|s| s.borrow_mut().clear());
    HOTKEY_LABEL.with(|s| *s.borrow_mut() = None);
    DATE_POPUP.with(|s| *s.borrow_mut() = None);
    THEME_POPUP.with(|s| *s.borrow_mut() = None);
    THEME_PLATE.with(|s| *s.borrow_mut() = None);
    THEME_SAMPLE.with(|s| *s.borrow_mut() = None);
    BLUR_BOX.with(|s| *s.borrow_mut() = None);
    RETENTION_LABEL.with(|s| *s.borrow_mut() = None);
    RETENTION_POPUP.with(|s| *s.borrow_mut() = None);
    SENSITIVE_BOX.with(|s| *s.borrow_mut() = None);
    ASK_BOX.with(|s| *s.borrow_mut() = None);
    PRIVACY_BOX.with(|s| *s.borrow_mut() = None);
    HISTORY_BOX.with(|s| *s.borrow_mut() = None);
    LOGIN_BOX.with(|s| *s.borrow_mut() = None);
    LOGIN_NOTE.with(|s| *s.borrow_mut() = None);
    SYMBOLS_FIELD.with(|s| *s.borrow_mut() = None);
    IGNORED_FIELD.with(|s| *s.borrow_mut() = None);
    IGNORED_ADD.with(|s| *s.borrow_mut() = None);
}

fn wire(control: &NSButton, action: mac_ui::objc2::runtime::Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(control, delegate, action);
        }
    });
}

fn set_hotkey_text(text: &str) {
    HOTKEY_LABEL.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(text));
        }
    });
}

fn handle_record_key(event: &NSEvent) {
    let key_code = event.keyCode();
    if key_code == mac_ui::keys::code::ESCAPE {
        RECORDING.set(false);
        set_hotkey_text(&settings::hotkey_label());
        return;
    }
    let flags = event.modifierFlags();
    let mut mods = Modifiers::empty();
    if flags.contains(NSEventModifierFlags::Control) {
        mods |= Modifiers::CONTROL;
    }
    if flags.contains(NSEventModifierFlags::Option) {
        mods |= Modifiers::ALT;
    }
    if flags.contains(NSEventModifierFlags::Shift) {
        mods |= Modifiers::SHIFT;
    }
    if flags.contains(NSEventModifierFlags::Command) {
        mods |= Modifiers::META;
    }
    let Some(code) = hotkey::code_from_key_code(key_code) else {
        return;
    };
    let chord = HotKey::new(Some(mods), code);
    if !hotkey::is_safe(&chord) {
        set_hotkey_text(crate::locale::t("hotkey_rejected"));
        return;
    }
    if settings::set_hotkey(&chord) {
        RECORDING.set(false);
        set_hotkey_text(&settings::hotkey_label());
        launcher::emit(UserEvent::HotkeyChanged);
    }
}

/// Open (or bring to front) the Settings window.
pub fn show() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    panel::activate_app(mtm);
    if let Some(window) = WINDOW.with(|slot| slot.borrow().clone()) {
        raise_above_card(&window);
        panel::bring_to_front(&window);
        refresh_all();
        return;
    }
    build(mtm);
}

fn build(mtm: MainThreadMarker) {
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT));
    // SAFETY: initWithContentRect:styleMask:backing:defer: on a freshly allocated window, with
    // plain value arguments.
    let window: Retained<SettingsWindow> = unsafe {
        msg_send![
            SettingsWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    mac_ui::panel::keep_when_closed(&window);
    window.setTitle(&NSString::from_str(crate::locale::t("settings_title")));
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();

    let delegate = SettingsDelegate::new(mtm);
    // The delegate is stored in DELEGATE right below and outlives the window.
    window.setDelegate(Some(mac_ui::objc2::runtime::ProtocolObject::from_ref(
        &*delegate,
    )));
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let body = panel::rounded_glass(mtm, &content, RADIUS).content;

    let tabs = NSSegmentedControl::new(mtm);
    tabs.setSegmentCount(Tab::ALL.len() as isize);
    for (index, tab) in Tab::ALL.iter().enumerate() {
        tabs.setLabel_forSegment(&NSString::from_str(tab.title()), index as isize);
    }
    tabs.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
    tabs.setSegmentDistribution(NSSegmentDistribution::FillEqually);
    tabs.setFrame(NSRect::new(
        NSPoint::new(MARGIN, HEIGHT - MARGIN - TAB_BAR_H),
        NSSize::new(WIDTH - MARGIN * 2.0, TAB_BAR_H),
    ));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(&tabs, delegate, sel!(tabChanged:));
        }
    });
    body.addSubview(&tabs);

    let pane_frame = NSRect::new(NSPoint::new(0.0, MARGIN), NSSize::new(WIDTH, PANE_H));
    let mut panes = Vec::with_capacity(Tab::ALL.len());
    for tab in Tab::ALL {
        let pane = NSView::initWithFrame(NSView::alloc(mtm), pane_frame);
        let left = match tab {
            Tab::General => build_general(mtm, &pane),
            Tab::Privacy => build_privacy(mtm, &pane),
            Tab::Display => build_display(mtm, &pane),
            Tab::Notifications => build_notifications(mtm, &pane),
            Tab::Feedback => build_feedback(mtm, &pane),
        };
        // Each tab fits the shared pane height; the tallest fills it.
        debug_assert!(left > -0.5, "{tab:?} is taller than the pane");
        body.addSubview(&pane);
        panes.push(pane);
    }
    PANES.with(|slot| *slot.borrow_mut() = panes);
    TABS.with(|slot| *slot.borrow_mut() = Some(tabs));
    show_pane(Tab::at(SELECTED_TAB.get()).unwrap_or(Tab::General));

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
}

/// Algemeen: hotkey, language, date order and Open at Login. Returns the unused height.
fn build_general(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = PANE_H;
    let value_x = MARGIN + LABEL_W + mac_ui::metrics::BUTTON_GAP;

    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("hotkey"), y);
    let value = widgets::label(
        mtm,
        mac_ui::metrics::FONT_BODY,
        &crate::appearance::caption_color(),
    );
    value.setStringValue(&NSString::from_str(&settings::hotkey_label()));
    let change = GlassButton::pill(mtm, crate::locale::t("change_hotkey"), ButtonSize::Small);
    change.set_symbol("keyboard");
    change.set_accessibility_label(crate::locale::t("change_hotkey_a11y"));
    let change_w = change.width_within(140.0).max(BUTTON_H);
    let change_x = WIDTH - MARGIN - change_w;
    value.setFrame(NSRect::new(
        NSPoint::new(value_x, y),
        NSSize::new(
            (change_x - mac_ui::metrics::BUTTON_GAP - value_x).max(40.0),
            BUTTON_H,
        ),
    ));
    body.addSubview(&value);
    HOTKEY_LABEL.with(|slot| *slot.borrow_mut() = Some(value));
    change.view().setFrame(NSRect::new(
        NSPoint::new(change_x, y),
        NSSize::new(change_w, BUTTON_H),
    ));
    wire(change.button(), sel!(changeHotkey:));
    body.addSubview(change.view());
    place_explanation(mtm, body, crate::locale::t("hotkey_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("language"), y);
    let language = popup(mtm, y);
    language.removeAllItems();
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_system")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_nl")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_en")));
    language.selectItemAtIndex(language_index(settings::language()));
    wire(language.as_ref(), sel!(languageChanged:));
    body.addSubview(&language);
    place_explanation(mtm, body, crate::locale::t("language_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, body, crate::locale::t("date_order"), y);
    let date_order = popup(mtm, y);
    date_order.removeAllItems();
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_dmy")));
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_mdy")));
    date_order.selectItemAtIndex(isize::from(settings::load().date_month_first));
    wire(date_order.as_ref(), sel!(dateOrderChanged:));
    body.addSubview(&date_order);
    DATE_POPUP.with(|slot| *slot.borrow_mut() = Some(date_order));
    place_explanation(mtm, body, crate::locale::t("date_order_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let login = checkbox(mtm, crate::locale::t("open_at_login"), y);
    wire(&login, sel!(loginToggled:));
    body.addSubview(&login);
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login));
    gap(&mut top, NOTE_GAP);
    let note = place_note(
        mtm,
        body,
        crate::locale::t("login_note"),
        take(&mut top, NOTE_H),
    );
    LOGIN_NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    refresh_login_row();
    top
}

/// Privacy: Keep clipboard history, the privacy filter, blur and the ignored apps (greyed out
/// while history is off). Returns the unused height.
fn build_privacy(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = PANE_H;
    let y = take(&mut top, BUTTON_H);
    let history = checkbox(mtm, crate::locale::t("clipboard_history"), y);
    history.setState(state_of(settings::clipboard_history()));
    wire(&history, sel!(clipboardHistoryToggled:));
    body.addSubview(&history);
    HISTORY_BOX.with(|slot| *slot.borrow_mut() = Some(history));
    place_explanation(
        mtm,
        body,
        crate::locale::t("clipboard_history_note"),
        &mut top,
    );
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
    let blur = checkbox(mtm, crate::locale::t("blur_masked"), y);
    blur.setState(state_of(settings::load().blur));
    // The blur only styles the privacy filter's mask.
    blur.setEnabled(settings::privacy_filter());
    wire(&blur, sel!(blurToggled:));
    body.addSubview(&blur);
    BLUR_BOX.with(|slot| *slot.borrow_mut() = Some(blur));
    place_explanation(mtm, body, crate::locale::t("blur_note"), &mut top);

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
fn place_history_settings(mtm: MainThreadMarker, body: &NSView, top: &mut f64) {
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
    sensitive.setToolTip(Some(&NSString::from_str(crate::locale::t(
        "detail_clear_sensitive",
    ))));
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
fn retention_title(minutes: u32) -> String {
    if minutes == 0 {
        crate::locale::t("keep_until_cleared").to_string()
    } else {
        crate::locale::tf("keep_after_last_copy", &[&minutes])
    }
}

/// A checkbox for a setting that depends on the one above it, [`INDENT`] in.
fn indented_checkbox(mtm: MainThreadMarker, title: &str, y: f64) -> Retained<NSButton> {
    let button = checkbox(mtm, title, y);
    button.setFrame(NSRect::new(
        NSPoint::new(MARGIN + INDENT, y),
        NSSize::new(WIDTH - MARGIN * 2.0 - INDENT, BUTTON_H),
    ));
    button
}

/// Meldingen: the four notification switches. Returns the unused height.
fn build_notifications(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = PANE_H;
    let switches = settings::notify_switches();
    let column = (WIDTH - MARGIN * 2.0) / 2.0;
    for (row_index, pair) in NOTIFY_SWITCHES.chunks(2).enumerate() {
        let y = take(&mut top, BUTTON_H);
        for (column_index, switch) in pair.iter().enumerate() {
            let at = row_index * 2 + column_index;
            let box_ = checkbox(mtm, notify_title(*switch), y);
            box_.setFrame(NSRect::new(
                NSPoint::new(MARGIN + column * column_index as f64, y),
                NSSize::new(column, BUTTON_H),
            ));
            box_.setTag(at as isize);
            box_.setState(state_of(switches.is_on(*switch)));
            wire(&box_, sel!(notifyToggled:));
            body.addSubview(&box_);
        }
        if row_index == 0 {
            gap(&mut top, mac_ui::metrics::SPACE_S);
        }
    }
    place_explanation(mtm, body, crate::locale::t("notify_note"), &mut top);
    top
}

/// Weergave: the card theme, long text in pages, Announce confirmations and the symbols.
/// Returns the unused height.
fn build_display(mtm: MainThreadMarker, body: &NSView) -> f64 {
    let mut top = PANE_H;
    let y = take(&mut top, BUTTON_H);
    place_card_theme(mtm, body, y);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let pages = checkbox(mtm, crate::locale::t("page_long_text"), y);
    pages.setState(state_of(settings::load().page_long_text));
    wire(&pages, sel!(pageLongTextToggled:));
    body.addSubview(&pages);
    place_explanation(mtm, body, crate::locale::t("page_long_text_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let announce = checkbox(mtm, crate::locale::t("announce_confirmations"), y);
    announce.setState(state_of(settings::announce_confirmations()));
    wire(&announce, sel!(announceToggled:));
    body.addSubview(&announce);
    place_explanation(
        mtm,
        body,
        crate::locale::t("announce_confirmations_note"),
        &mut top,
    );
    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_label(
        mtm,
        body,
        crate::locale::t("symbols"),
        take(&mut top, BUTTON_H),
        WIDTH - MARGIN * 2.0,
    );
    place_explanation(mtm, body, crate::locale::t("symbols_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    // The field and Restore share one row, so the card theme fits in the pane.
    let y = take(&mut top, BUTTON_H);
    let restore = GlassButton::pill(mtm, crate::locale::t("symbols_restore"), ButtonSize::Small);
    restore.set_symbol("arrow.counterclockwise");
    restore.set_accessibility_label(crate::locale::t("symbols_restore"));
    let restore_w = restore.width_within(200.0).max(BUTTON_H);
    restore.view().setFrame(NSRect::new(
        NSPoint::new(WIDTH - MARGIN - restore_w, y),
        NSSize::new(restore_w, BUTTON_H),
    ));
    wire(restore.button(), sel!(restoreSymbols:));
    body.addSubview(restore.view());
    let field = text_field(mtm, y);
    field.setFrameSize(NSSize::new(
        WIDTH - MARGIN * 2.0 - mac_ui::metrics::BUTTON_GAP - restore_w,
        BUTTON_H,
    ));
    field.setStringValue(&NSString::from_str(&settings::symbol_catalog_text()));
    field.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "symbols_field_a11y",
    ))));
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: SettingsDelegate implements controlTextDidEndEditing: with the protocol
            // signature. DELEGATE keeps it alive while the window is open; the field holds it
            // weakly.
            unsafe { widgets::set_text_delegate(&field, delegate) };
        }
    });
    body.addSubview(&field);
    SYMBOLS_FIELD.with(|slot| *slot.borrow_mut() = Some(field));
    top
}

/// The name of a card theme in the popup (literal keys, for the locale scan test).
fn card_theme_name(theme: crate::card_theme::CardTheme) -> &'static str {
    use crate::card_theme::CardTheme;
    match theme {
        CardTheme::System => crate::locale::t("card_theme_system"),
        CardTheme::Light => crate::locale::t("card_theme_light"),
        CardTheme::Dark => crate::locale::t("card_theme_dark"),
        CardTheme::Night => crate::locale::t("card_theme_night"),
        CardTheme::SolarizedLight => crate::locale::t("card_theme_solarized_light"),
        CardTheme::SolarizedDark => crate::locale::t("card_theme_solarized_dark"),
        CardTheme::HighContrast => crate::locale::t("card_theme_high_contrast"),
        CardTheme::Sepia => crate::locale::t("card_theme_sepia"),
        CardTheme::Nord => crate::locale::t("card_theme_nord"),
        CardTheme::GruvboxDark => crate::locale::t("card_theme_gruvbox_dark"),
        CardTheme::Dracula => crate::locale::t("card_theme_dracula"),
        CardTheme::Monokai => crate::locale::t("card_theme_monokai"),
        CardTheme::CatppuccinMocha => crate::locale::t("card_theme_catppuccin_mocha"),
        CardTheme::TokyoNight => crate::locale::t("card_theme_tokyo_night"),
        CardTheme::Ocean => crate::locale::t("card_theme_ocean"),
        CardTheme::Forest => crate::locale::t("card_theme_forest"),
    }
}

/// The theme names, a separator between the basic, classic and colourful groups, each item
/// tagged with its place in [`crate::card_theme::CardTheme::ALL`]; the stored theme selected.
fn fill_theme_popup(mtm: MainThreadMarker, popup: &NSPopUpButton) {
    popup.removeAllItems();
    let menu = popup.menu();
    let mut group = None;
    for (index, theme) in crate::card_theme::CardTheme::ALL.into_iter().enumerate() {
        if group.is_some_and(|previous| previous != theme.group())
            && let Some(menu) = &menu
        {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        group = Some(theme.group());
        popup.addItemWithTitle(&NSString::from_str(card_theme_name(theme)));
        if let Some(item) = popup.lastItem() {
            item.setTag(index as isize);
        }
    }
    select_card_theme(popup);
}

/// Select the stored theme by its tag (the separators make item positions differ).
fn select_card_theme(popup: &NSPopUpButton) {
    popup.selectItemWithTag(settings::card_theme().index() as isize);
}

/// "Card theme", its popup, and a small preview plate to the right of it, on one row at `y`.
fn place_card_theme(mtm: MainThreadMarker, body: &NSView, y: f64) {
    place_name(mtm, body, crate::locale::t("card_theme"), y);
    let popup = popup(mtm, y);
    let x = popup.frame().origin.x;
    popup.setFrameSize(NSSize::new(THEME_POPUP_W, BUTTON_H));
    fill_theme_popup(mtm, &popup);
    popup.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("card_theme"))));
    wire(popup.as_ref(), sel!(cardThemeChanged:));
    body.addSubview(&popup);
    THEME_POPUP.with(|slot| *slot.borrow_mut() = Some(popup));

    let plate_x = x + THEME_POPUP_W + mac_ui::metrics::BUTTON_GAP;
    let plate_w = WIDTH - MARGIN - plate_x;
    let plate = widgets::filled_box(
        mtm,
        mac_ui::style::SPACE_XS,
        &NSColor::textBackgroundColor(),
    );
    plate.setBorderColor(&NSColor::separatorColor());
    plate.setBorderWidth(1.0);
    plate.setFrame(NSRect::new(
        NSPoint::new(plate_x, y),
        NSSize::new(plate_w, BUTTON_H),
    ));
    body.addSubview(&plate);
    let sample = widgets::label(mtm, mac_ui::style::FONT_CODE, &NSColor::labelColor());
    let inset = mac_ui::metrics::SPACE_S;
    sample.setFrame(NSRect::new(
        NSPoint::new(plate_x + inset, y + (BUTTON_H - NOTE_H) / 2.0),
        NSSize::new(plate_w - inset * 2.0, NOTE_H),
    ));
    sample.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "card_theme_preview",
    ))));
    body.addSubview(&sample);
    THEME_PLATE.with(|slot| *slot.borrow_mut() = Some(plate));
    THEME_SAMPLE.with(|slot| *slot.borrow_mut() = Some(sample));
    paint_theme_preview();
}

/// The preview sample: "Aa" on the selection colour, then a JSON key and number, in the stored
/// theme on its background (System: the dynamic system colours, which follow the appearance).
fn paint_theme_preview() {
    const SAMPLE: &str = "Aa {\"k\": 42}";
    let palette = settings::card_theme().palette();
    let srgb = crate::appearance::srgb;
    let (background, text, key, number, selection) = match palette {
        Some(p) => (
            srgb(p.background),
            srgb(p.text),
            srgb(p.key),
            srgb(p.number),
            srgb(p.selection),
        ),
        None => (
            NSColor::textBackgroundColor(),
            NSColor::labelColor(),
            NSColor::systemBlueColor(),
            NSColor::systemOrangeColor(),
            NSColor::selectedTextBackgroundColor(),
        ),
    };
    THEME_PLATE.with(|slot| {
        if let Some(plate) = slot.borrow().as_ref() {
            plate.setFillColor(&background);
        }
    });
    THEME_SAMPLE.with(|slot| {
        if let Some(sample) = slot.borrow().as_ref() {
            let font = mac_ui::fonts::monospace(mac_ui::style::FONT_CODE, &[]);
            let attr = mac_ui::text::AttrText::new(SAMPLE, &font, &text);
            attr.background(&(0..2), &selection);
            attr.color(&(4..7), &key);
            attr.color(&(9..11), &number);
            sample.setAttributedStringValue(&attr.into_attributed());
        }
    });
}

/// Feedback: Suggest a feature, Report a problem, the privacy policy and support. Returns the
/// unused height.
fn build_feedback(mtm: MainThreadMarker, body: &NSView) -> f64 {
    // The privacy policy and support page open in the default browser (NSWorkspace).
    let buttons = [
        (
            crate::locale::t("feedback_feature_menu"),
            "lightbulb",
            sel!(suggestFeature:),
        ),
        (
            crate::locale::t("feedback_problem_menu"),
            "exclamationmark.bubble",
            sel!(reportProblem:),
        ),
        (
            crate::locale::t("privacy_policy"),
            "hand.raised",
            sel!(openPrivacy:),
        ),
        (
            crate::locale::t("support_link"),
            "questionmark.circle",
            sel!(openSupport:),
        ),
    ]
    .map(|(title, symbol, action)| {
        let pill = GlassButton::pill(mtm, title, ButtonSize::Small);
        pill.set_symbol(symbol);
        pill.set_accessibility_label(title);
        (pill, action)
    });
    // One width for all four: the widest title, so the column reads as one block.
    let width = buttons
        .iter()
        .map(|(pill, _)| pill.width_within(FEEDBACK_BUTTON_MAX_W))
        .fold(BUTTON_H, f64::max);
    let mut top = PANE_H;
    for (at, (pill, action)) in buttons.into_iter().enumerate() {
        match at {
            0 => {}
            // A wider gap between the mail buttons and the links.
            2 => gap(&mut top, mac_ui::metrics::SECTION_GAP),
            _ => gap(&mut top, mac_ui::metrics::SPACE_S),
        }
        let y = take(&mut top, BUTTON_H);
        pill.view().setFrame(NSRect::new(
            NSPoint::new(MARGIN, y),
            NSSize::new(width, BUTTON_H),
        ));
        wire(pill.button(), action);
        body.addSubview(pill.view());
    }
    top
}

/// The tab a ⌘1…⌘5 chord picks, if `event` is one.
fn tab_for_chord(event: &NSEvent) -> Option<Tab> {
    Tab::ALL
        .into_iter()
        .enumerate()
        .find(|(at, _)| mac_ui::keys::is_command_chord(event, &(at + 1).to_string()))
        .map(|(_, tab)| tab)
}

/// Show the pane of tab `index` (out of range: General) and hide the others. A field being
/// edited is saved first, and a hotkey being recorded is cancelled, so nothing keeps listening
/// on a hidden tab.
fn select_tab(index: usize) {
    let tab = Tab::at(index).unwrap_or(Tab::General);
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            // Ends editing, which saves through controlTextDidEndEditing:.
            window.makeFirstResponder(None);
        }
    });
    save_symbols_field();
    save_ignored_field();
    if RECORDING.get() {
        RECORDING.set(false);
        set_hotkey_text(&settings::hotkey_label());
    }
    show_pane(tab);
}

/// Select `tab`'s segment and show only its pane (no saving: also used while building).
fn show_pane(tab: Tab) {
    SELECTED_TAB.set(tab.index());
    TABS.with(|slot| {
        if let Some(control) = slot.borrow().as_ref() {
            control.setSelectedSegment(tab.index() as isize);
        }
    });
    PANES.with(|slot| {
        for (at, pane) in slot.borrow().iter().enumerate() {
            pane.setHidden(at != tab.index());
        }
    });
}

/// Reduce Transparency changed: cover or uncover the glass of the open window.
pub fn restyle() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(window) = WINDOW.with(|slot| slot.borrow().clone()) {
        mac_ui::glass::restyle_window(mtm, &window, RADIUS);
    }
}

fn raise_above_card(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::raise_above_card(window, previous));
}

fn restore_level(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::restore_above_card(window, previous));
}

fn take(top: &mut f64, height: f64) -> f64 {
    *top -= height;
    *top
}

fn gap(top: &mut f64, height: f64) {
    *top -= height;
}

fn place_name(mtm: MainThreadMarker, parent: &NSView, title: &str, y: f64) {
    place_label(mtm, parent, title, y, LABEL_W);
}

fn place_label(mtm: MainThreadMarker, parent: &NSView, title: &str, y: f64, width: f64) {
    let field = widgets::label(mtm, mac_ui::metrics::FONT_BODY, &NSColor::labelColor());
    field.setStringValue(&NSString::from_str(title));
    field.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(width, BUTTON_H),
    ));
    parent.addSubview(&field);
}

fn place_note(mtm: MainThreadMarker, parent: &NSView, text: &str, y: f64) -> Retained<NSTextField> {
    let note = widgets::label(
        mtm,
        mac_ui::metrics::FONT_CAPTION,
        &crate::appearance::caption_color(),
    );
    note.setStringValue(&NSString::from_str(text));
    note.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, NOTE_H),
    ));
    parent.addSubview(&note);
    note
}

fn place_explanation(mtm: MainThreadMarker, parent: &NSView, text: &str, top: &mut f64) {
    gap(top, NOTE_GAP);
    place_note(mtm, parent, text, take(top, NOTE_H));
}

fn checkbox(mtm: MainThreadMarker, title: &str, y: f64) -> Retained<NSButton> {
    let button = mac_ui::actions::checkbox(mtm, title);
    button.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, BUTTON_H),
    ));
    button
}

fn popup(mtm: MainThreadMarker, y: f64) -> Retained<NSPopUpButton> {
    let x = MARGIN + LABEL_W + mac_ui::metrics::BUTTON_GAP;
    NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(x, y),
            NSSize::new(WIDTH - MARGIN - x, BUTTON_H),
        ),
        false,
    )
}

fn text_field(mtm: MainThreadMarker, y: f64) -> Retained<NSTextField> {
    let field = widgets::plain_field(mtm, mac_ui::metrics::FONT_BODY, "");
    field.setBezeled(true);
    field.setDrawsBackground(true);
    field.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, BUTTON_H),
    ));
    field
}

fn place_pill(button: &GlassButton, y: f64, max_w: f64) {
    let width = button.width_within(max_w).max(BUTTON_H);
    button.view().setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(width, BUTTON_H),
    ));
}

fn language_index(language: settings::Language) -> isize {
    match language {
        settings::Language::System => 0,
        settings::Language::Nl => 1,
        settings::Language::En => 2,
    }
}

fn state_of(on: bool) -> isize {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

fn refresh_all() {
    set_hotkey_text(&settings::hotkey_label());
    THEME_POPUP.with(|slot| {
        if let Some(popup) = slot.borrow().as_ref() {
            select_card_theme(popup);
        }
    });
    paint_theme_preview();
    DATE_POPUP.with(|slot| {
        if let Some(popup) = slot.borrow().as_ref() {
            popup.selectItemAtIndex(isize::from(settings::load().date_month_first));
        }
    });
    PRIVACY_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(settings::privacy_filter()));
        }
    });
    BLUR_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(settings::load().blur));
            button.setEnabled(settings::privacy_filter());
        }
    });
    refresh_login_row();
    show_symbol_catalog();
    refresh_ignored_field();
    refresh_history_controls();
}

fn save_symbols_field() {
    let raw = SYMBOLS_FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| field.stringValue().to_string())
    });
    let Some(raw) = raw else {
        return;
    };
    settings::set_symbol_catalog(&raw);
    show_symbol_catalog();
    crate::macos_symbols::reload();
}

/// The ignore field follows the stored list, and Add follows the last other frontmost app.
pub fn refresh_ignored_field() {
    show_ignored_apps();
    refresh_ignore_button();
}

fn save_ignored_field() {
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

fn show_ignored_apps() {
    IGNORED_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            let text = settings::ignored_apps_text();
            if field.stringValue().to_string() != text {
                field.setStringValue(&NSString::from_str(&text));
            }
        }
    });
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

fn refresh_ignore_button() {
    let enabled = crate::history_mode::capture_controls_active(settings::clipboard_history())
        && crate::capture::last_foreign().is_some();
    IGNORED_ADD.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setEnabled(enabled);
        }
    });
}

fn show_symbol_catalog() {
    SYMBOLS_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            let text = settings::symbol_catalog_text();
            if field.stringValue().to_string() != text {
                field.setStringValue(&NSString::from_str(&text));
            }
        }
    });
}

fn refresh_login_row() {
    let status = macos_login::status();
    LOGIN_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(if status.is_on() {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
            button.setEnabled(status != macos_login::LoginStatus::Unavailable);
        }
    });
    let note = match status {
        macos_login::LoginStatus::RequiresApproval => crate::locale::t("login_approval"),
        macos_login::LoginStatus::Unavailable => crate::locale::t("login_unavailable"),
        macos_login::LoginStatus::NotFound => crate::locale::t("login_not_found"),
        _ => crate::locale::t("login_note"),
    };
    LOGIN_NOTE.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(note));
        }
    });
}

/// Close and open Settings again so every label follows the new language.
pub fn reopen() {
    if WINDOW.with(|slot| slot.borrow().is_none()) {
        return;
    }
    close();
    show();
}

/// Close the Settings window (Wipe, Quit).
pub fn close() {
    RECORDING.set(false);
    if let Some(window) = WINDOW.with(|slot| slot.borrow_mut().take()) {
        restore_level(&window);
        window.close();
    }
    clear_controls();
    DELEGATE.with(|slot| *slot.borrow_mut() = None);
}
