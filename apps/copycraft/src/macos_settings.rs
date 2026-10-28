//! Settings window, in three tabs (a segmented control above one pane per tab):
//! General (hotkey, language, date order, Open at Login, then the Meldingen block with the four
//! notification switches and Announce confirmations), Privacy (Keep clipboard history with retention, sensitive-copy
//! clearing and ask before clearing under it, privacy filter, blur, ignored apps) and Display
//! (card theme, pages, symbols). The feedback mail, privacy policy and support are in
//! About and `⋯` › Help ▸. ⌘1…⌘3 pick a tab, ⌃Tab/⌃⇧Tab and ⌘⇧]/⌘⇧[ step through them
//! (wrapping); the last tab is stored and opened again. Each tab's pane is built in its own
//! file (`general`, `privacy`, `display`), from the rows in `controls`.

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

use crate::settings_tabs::{Mods, NOTE_GAP, NOTE_H, TAB_BAR_H, Tab, WIDTH, keep_top};

mod controls;
mod display;
mod general;
mod privacy;

use controls::*;
use display::*;
use general::*;
use privacy::*;

pub use display::card_theme_changed;
pub use general::refresh_notifications;
pub use privacy::{refresh_history_controls, refresh_ignored_field};

const MARGIN: f64 = mac_ui::metrics::MARGIN;
const BUTTON_H: f64 = mac_ui::metrics::BUTTON_H;
const SECTION_GAP: f64 = mac_ui::metrics::SECTION_GAP;
/// Width of the name on a row that also holds a value (hotkey, language, date order).
const LABEL_W: f64 = 136.0;
/// How far a setting that depends on the one above it is indented.
const INDENT: f64 = 20.0;
const RADIUS: f64 = mac_ui::metrics::PANEL_RADIUS;

thread_local! {
    static WINDOW: RefCell<Option<Retained<SettingsWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<SettingsDelegate>>> = const { RefCell::new(None) };
    static HOTKEY_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    /// The screenshot hotkey's value on Algemeen.
    static SHOT_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    /// The notes under the two hotkey rows: they say when a chord is taken.
    static HOTKEY_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static SHOT_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    /// Privacy's "Show CopyCraft in screenshots".
    static SHOT_OWN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
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
    /// The four notification switches (dimmed while macOS refuses notifications), the note
    /// under them and *Open System Settings › Notifications…* (only while refused).
    static NOTIFY_BOXES: RefCell<Vec<Retained<NSButton>>> = const { RefCell::new(Vec::new()) };
    static NOTIFY_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static NOTIFY_OPEN: RefCell<Option<Retained<NSView>>> = const { RefCell::new(None) };
    static TABS: RefCell<Option<Retained<NSSegmentedControl>>> = const { RefCell::new(None) };
    /// One view per [`Tab`], in [`Tab::ALL`] order; only the selected one is visible.
    static PANES: RefCell<Vec<Retained<NSView>>> = const { RefCell::new(Vec::new()) };
    /// A tab the next build opens on instead of the stored last one ([`show_privacy`]).
    static OPEN_ON: Cell<Option<Tab>> = const { Cell::new(None) };
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    /// While [`RECORDING`]: the chord goes to the screenshot hotkey, not the card's.
    static RECORDING_SHOT: Cell<bool> = const { Cell::new(false) };
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
            // ⌃Tab can also arrive here rather than as a key equivalent.
            if let Some(step) = tab_step(event) {
                select_tab(current_tab().step(step).index());
                return;
            }
            // SAFETY: forwards the event to the superclass implementation of the same selector,
            // with the same argument and return types.
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            // The hotkey field is listening: every ⌘ chord is a hotkey to record, not a tab or
            // a menu key (⌘W, ⌘Z, ⌘V, …).
            // ⌘1…⌘3 pick a tab, ⌃Tab/⌘⇧] and ⌃⇧Tab/⌘⇧[ the next and previous one (wrapping).
            // ⌘W and ⌘M are the hidden Window menu's.
            let recording = RECORDING.get()
                && event
                    .modifierFlags()
                    .contains(NSEventModifierFlags::Command);
            if recording {
                handle_record_key(event);
                true
            } else if let Some(tab) = tab_for_chord(event) {
                select_tab(tab.index());
                true
            } else if let Some(step) = tab_step(event) {
                select_tab(current_tab().step(step).index());
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
            let window = WINDOW.with(|slot| slot.borrow_mut().take());
            if let Some(window) = window {
                restore_level(&window);
                // Not released while it is still closing.
                mac_ui::panel::release_after_close(Retained::into_super(window));
            }
            clear_controls();
        }
    }

    impl SettingsDelegate {
        #[unsafe(method(changeHotkey:))]
        fn change_hotkey(&self, _sender: Option<&NSButton>) {
            start_recording(false);
        }

        #[unsafe(method(changeScreenshotHotkey:))]
        fn change_screenshot_hotkey(&self, _sender: Option<&NSButton>) {
            start_recording(true);
        }

        #[unsafe(method(screenshotOwnToggled:))]
        fn screenshot_own_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_screenshot_includes_own(button.state() == NSControlStateValueOn);
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

        /// Shown while macOS refuses notifications: its Notifications pane, where CopyCraft can
        /// be allowed again (checked when Settings › General opens next).
        #[unsafe(method(openNotificationSettings:))]
        fn open_notification_settings(&self, _sender: Option<&NSButton>) {
            crate::system_settings::open(crate::system_settings::Pane::Notifications);
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

        #[unsafe(method(restoreSymbols:))]
        fn restore_symbols(&self, _sender: Option<&NSButton>) {
            // Your own characters are replaced: ask first, Cancel is the default (Return/Esc).
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let t = crate::locale::t;
            let answer = mac_ui::dialog::confirm_destructive(
                mtm,
                t("symbols_restore_title"),
                t("symbols_restore_message"),
                t("symbols_restore"),
                t("cancel"),
                None,
            );
            if !answer.confirmed {
                return;
            }
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
    SHOT_LABEL.with(|s| *s.borrow_mut() = None);
    HOTKEY_NOTE.with(|s| *s.borrow_mut() = None);
    SHOT_NOTE.with(|s| *s.borrow_mut() = None);
    SHOT_OWN_BOX.with(|s| *s.borrow_mut() = None);
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
    NOTIFY_BOXES.with(|s| s.borrow_mut().clear());
    NOTIFY_NOTE.with(|s| *s.borrow_mut() = None);
    NOTIFY_OPEN.with(|s| *s.borrow_mut() = None);
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
        if current_tab() == Tab::General {
            check_notifications();
        }
        return;
    }
    build(mtm);
}

/// Open the Settings window on the Privacy tab (the empty card's link).
pub fn show_privacy() {
    if WINDOW.with(|slot| slot.borrow().is_some()) {
        select_tab(Tab::Privacy.index());
    } else {
        OPEN_ON.set(Some(Tab::Privacy));
    }
    show();
}

fn build(mtm: MainThreadMarker) {
    // Miniaturizable: ⌘M (the hidden Window menu, src/macos_main_menu.rs) minimises it.
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    // The window opens at the height of the tab it shows; `show_pane` follows the tab.
    let shown = OPEN_ON
        .take()
        .unwrap_or_else(|| Tab::from_stored(settings::settings_tab()));
    let height = shown.window_height();
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, height));
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
    // The open tab's name, as in System Settings; `show_pane` keeps it in step.
    window.setTitle(&NSString::from_str(shown.title()));
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
    // The tab bar and every pane hang from the top: when the window takes another tab's
    // height, its bottom edge moves and they stay put.
    let below_top = mac_ui::objc2_app_kit::NSAutoresizingMaskOptions::ViewMinYMargin;
    tabs.setFrame(NSRect::new(
        NSPoint::new(MARGIN, height - MARGIN - TAB_BAR_H),
        NSSize::new(WIDTH - MARGIN * 2.0, TAB_BAR_H),
    ));
    tabs.setAutoresizingMask(below_top);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            mac_ui::actions::connect(&tabs, delegate, sel!(tabChanged:));
        }
    });
    body.addSubview(&tabs);

    let pane_top = height - MARGIN - TAB_BAR_H - SECTION_GAP;
    let mut panes = Vec::with_capacity(Tab::ALL.len());
    for tab in Tab::ALL {
        let pane_h = tab.pane_height();
        let pane_frame = NSRect::new(
            NSPoint::new(0.0, pane_top - pane_h),
            NSSize::new(WIDTH, pane_h),
        );
        let pane = NSView::initWithFrame(NSView::alloc(mtm), pane_frame);
        pane.setAutoresizingMask(below_top);
        let left = match tab {
            Tab::General => build_general(mtm, &pane),
            Tab::Privacy => build_privacy(mtm, &pane),
            Tab::Display => build_display(mtm, &pane),
        };
        // Each pane is exactly as tall as its settings.
        debug_assert!(left.abs() < 0.5, "{tab:?} does not fill its pane");
        body.addSubview(&pane);
        panes.push(pane);
    }
    PANES.with(|slot| *slot.borrow_mut() = panes);
    TABS.with(|slot| *slot.borrow_mut() = Some(tabs));
    show_pane(shown);

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
}

/// The tab a ⌘1…⌘3 chord picks, if `event` is one.
fn tab_for_chord(event: &NSEvent) -> Option<Tab> {
    Tab::ALL
        .into_iter()
        .enumerate()
        .find(|(at, _)| mac_ui::keys::is_command_chord(event, &(at + 1).to_string()))
        .map(|(_, tab)| tab)
}

/// The next or previous tab `event` asks for (⌃Tab, ⌃⇧Tab, ⌘⇧], ⌘⇧[), if any.
fn tab_step(event: &NSEvent) -> Option<crate::settings_tabs::Step> {
    let flags = event.modifierFlags();
    let chars = event
        .charactersIgnoringModifiers()
        .map(|text| text.to_string())
        .unwrap_or_default();
    crate::settings_tabs::step_for_key(
        event.keyCode(),
        &chars,
        Mods {
            command: flags.contains(NSEventModifierFlags::Command),
            control: flags.contains(NSEventModifierFlags::Control),
            shift: flags.contains(NSEventModifierFlags::Shift),
            option: flags.contains(NSEventModifierFlags::Option),
        },
    )
}

/// The tab shown now (the segment selected), General before the window exists.
fn current_tab() -> Tab {
    TABS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|control| usize::try_from(control.selectedSegment()).ok())
            .and_then(Tab::at)
            .unwrap_or(Tab::General)
    })
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
        show_stored_hotkeys();
    }
    show_pane(tab);
}

/// Select `tab`'s segment, name the window after it, show only its pane and give the window
/// its height (no saving: also used while building).
fn show_pane(tab: Tab) {
    // Stored, so Settings opens on this tab again, also after quitting CopyCraft.
    settings::set_settings_tab(tab.index());
    WINDOW.with(|slot| {
        if let Some(window) = slot.borrow().as_ref() {
            window.setTitle(&NSString::from_str(tab.title()));
        }
    });
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
    fit_window(tab);
    if tab == Tab::General {
        check_notifications();
    }
}

/// Give the open window `tab`'s height, its top edge where it was (as System Settings does),
/// animated unless Reduce Motion is on. Not while building: the window opens at that height.
fn fit_window(tab: Tab) {
    WINDOW.with(|slot| {
        let Some(window) = slot.borrow().as_ref().map(|window| window.retain()) else {
            return;
        };
        let old = window.frame();
        let mut new = window.frameRectForContentRect(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(WIDTH, tab.window_height()),
        ));
        if (new.size.height - old.size.height).abs() < 0.5 {
            return;
        }
        new.origin.x = old.origin.x;
        new.origin.y = keep_top(old.origin.y, old.size.height, new.size.height);
        window.setFrame_display_animate(new, true, !mac_ui::motion::reduce_motion());
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

/// The hotkey values and notes again, after registering (not while recording).
pub(crate) fn refresh_hotkeys() {
    if !RECORDING.get() {
        show_stored_hotkeys();
    }
}

fn refresh_all() {
    show_stored_hotkeys();
    SHOT_OWN_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(state_of(settings::screenshot_includes_own()));
        }
    });
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
