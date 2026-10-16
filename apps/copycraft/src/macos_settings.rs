//! Settings window: hotkey, date order, language, privacy filter, blur, Open at Login, the
//! symbol list and ignored apps.

use std::cell::{Cell, RefCell};

use global_hotkey::hotkey::{HotKey, Modifiers};
use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventModifierFlags, NSFont, NSPopUpButton, NSTextField,
    NSView, NSWindow, NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWindowTabbingMode,
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

const WIDTH: f64 = 420.0;
const MARGIN: f64 = mac_ui::metrics::MARGIN;
const BUTTON_H: f64 = mac_ui::metrics::BUTTON_H;
const NOTE_GAP: f64 = 2.0;
const NOTE_H: f64 = 16.0;
/// One control plus its one-line explanation.
const SETTING_H: f64 = BUTTON_H + NOTE_GAP + NOTE_H;
/// A labelled setting with a field and a button under the explanation.
const FIELD_BLOCK_H: f64 =
    SETTING_H + mac_ui::metrics::SPACE_S + BUTTON_H + mac_ui::metrics::SPACE_S + BUTTON_H;
const HEAD_H: f64 = 22.0;
const SECTION_GAP: f64 = mac_ui::metrics::SECTION_GAP;
/// Width of the name on a row that also holds a value (hotkey, language, date order).
const LABEL_W: f64 = 136.0;
/// Algemeen: heading, four settings, three gaps.
const GENERAL_H: f64 = HEAD_H + 4.0 * SETTING_H + 3.0 * mac_ui::metrics::SPACE_S;
/// Privacy: heading, two settings, the ignored-apps block, two gaps.
const PRIVACY_H: f64 = HEAD_H + 2.0 * SETTING_H + 2.0 * mac_ui::metrics::SPACE_S + FIELD_BLOCK_H;
/// Weergave: heading, the pages, announce and ask-before-clearing settings, three gaps and the
/// symbols block.
const DISPLAY_H: f64 = HEAD_H + 3.0 * SETTING_H + 3.0 * mac_ui::metrics::SPACE_S + FIELD_BLOCK_H;
/// Meldingen: heading, the four switches in two rows of two, one gap and a note.
const NOTIFY_H: f64 = HEAD_H + 2.0 * BUTTON_H + mac_ui::metrics::SPACE_S + NOTE_GAP + NOTE_H;
/// Feedback: heading and one row with the two mail buttons.
const FEEDBACK_H: f64 = HEAD_H + BUTTON_H;
const HEIGHT: f64 = MARGIN
    + GENERAL_H
    + SECTION_GAP
    + PRIVACY_H
    + SECTION_GAP
    + NOTIFY_H
    + SECTION_GAP
    + DISPLAY_H
    + SECTION_GAP
    + FEEDBACK_H
    + MARGIN;
const RADIUS: f64 = mac_ui::metrics::PANEL_RADIUS;

const _: () = assert!(SETTING_H == 46.0);
const _: () = assert!(FIELD_BLOCK_H == 118.0);
const _: () = assert!(GENERAL_H == 230.0);
const _: () = assert!(PRIVACY_H == 248.0);
const _: () = assert!(DISPLAY_H == 302.0);
const _: () = assert!(NOTIFY_H == 104.0);
const _: () = assert!(HEIGHT == 964.0 + SECTION_GAP + FEEDBACK_H);

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
    static PRIVACY_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static BLUR_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static SYMBOLS_FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static IGNORED_FIELD: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static IGNORED_ADD: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
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
            let open = mac_ui::keys::is_command_chord(event, "o");
            let save = mac_ui::keys::is_command_chord(event, "s");
            if open || save {
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

        #[unsafe(method(dateOrderChanged:))]
        fn date_order_changed(&self, sender: Option<&NSPopUpButton>) {
            let Some(popup) = sender else {
                return;
            };
            settings::set_date_month_first(popup.indexOfSelectedItem() == 1);
            launcher::emit(UserEvent::SettingsChanged);
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
    HOTKEY_LABEL.with(|s| *s.borrow_mut() = None);
    DATE_POPUP.with(|s| *s.borrow_mut() = None);
    BLUR_BOX.with(|s| *s.borrow_mut() = None);
    PRIVACY_BOX.with(|s| *s.borrow_mut() = None);
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

    let mut top = HEIGHT - MARGIN;
    let value_x = MARGIN + LABEL_W + mac_ui::metrics::BUTTON_GAP;

    place_heading(
        mtm,
        &body,
        crate::locale::t("group_general"),
        take(&mut top, HEAD_H),
    );

    let y = take(&mut top, BUTTON_H);
    place_name(mtm, &body, crate::locale::t("hotkey"), y);
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
    place_explanation(mtm, &body, crate::locale::t("hotkey_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, &body, crate::locale::t("language"), y);
    let language = popup(mtm, y);
    language.removeAllItems();
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_system")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_nl")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_en")));
    language.selectItemAtIndex(language_index(settings::language()));
    wire(language.as_ref(), sel!(languageChanged:));
    body.addSubview(&language);
    place_explanation(mtm, &body, crate::locale::t("language_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    place_name(mtm, &body, crate::locale::t("date_order"), y);
    let date_order = popup(mtm, y);
    date_order.removeAllItems();
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_dmy")));
    date_order.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_mdy")));
    date_order.selectItemAtIndex(isize::from(settings::load().date_month_first));
    wire(date_order.as_ref(), sel!(dateOrderChanged:));
    body.addSubview(&date_order);
    DATE_POPUP.with(|slot| *slot.borrow_mut() = Some(date_order));
    place_explanation(mtm, &body, crate::locale::t("date_order_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let login = checkbox(mtm, crate::locale::t("open_at_login"), y);
    wire(&login, sel!(loginToggled:));
    body.addSubview(&login);
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login));
    gap(&mut top, NOTE_GAP);
    let note = place_note(
        mtm,
        &body,
        crate::locale::t("login_note"),
        take(&mut top, NOTE_H),
    );
    LOGIN_NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    refresh_login_row();

    gap(&mut top, SECTION_GAP);
    place_heading(
        mtm,
        &body,
        crate::locale::t("group_privacy"),
        take(&mut top, HEAD_H),
    );

    let y = take(&mut top, BUTTON_H);
    let privacy = checkbox(mtm, crate::locale::t("privacy_filter"), y);
    privacy.setState(state_of(settings::privacy_filter()));
    wire(&privacy, sel!(privacyFilterToggled:));
    body.addSubview(&privacy);
    PRIVACY_BOX.with(|slot| *slot.borrow_mut() = Some(privacy));
    place_explanation(mtm, &body, crate::locale::t("privacy_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let blur = checkbox(mtm, crate::locale::t("blur_masked"), y);
    blur.setState(state_of(settings::load().blur));
    // The blur only styles the privacy filter's mask.
    blur.setEnabled(settings::privacy_filter());
    wire(&blur, sel!(blurToggled:));
    body.addSubview(&blur);
    BLUR_BOX.with(|slot| *slot.borrow_mut() = Some(blur));
    place_explanation(mtm, &body, crate::locale::t("blur_note"), &mut top);

    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_label(
        mtm,
        &body,
        crate::locale::t("ignored_apps"),
        take(&mut top, BUTTON_H),
        WIDTH - MARGIN * 2.0,
    );
    place_explanation(mtm, &body, crate::locale::t("ignored_note"), &mut top);
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
    refresh_ignore_button();

    gap(&mut top, SECTION_GAP);
    place_heading(
        mtm,
        &body,
        crate::locale::t("group_notifications"),
        take(&mut top, HEAD_H),
    );
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
    place_explanation(mtm, &body, crate::locale::t("notify_note"), &mut top);

    gap(&mut top, SECTION_GAP);
    place_heading(
        mtm,
        &body,
        crate::locale::t("group_display"),
        take(&mut top, HEAD_H),
    );
    let y = take(&mut top, BUTTON_H);
    let pages = checkbox(mtm, crate::locale::t("page_long_text"), y);
    pages.setState(state_of(settings::load().page_long_text));
    wire(&pages, sel!(pageLongTextToggled:));
    body.addSubview(&pages);
    place_explanation(
        mtm,
        &body,
        crate::locale::t("page_long_text_note"),
        &mut top,
    );
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let announce = checkbox(mtm, crate::locale::t("announce_confirmations"), y);
    announce.setState(state_of(settings::announce_confirmations()));
    wire(&announce, sel!(announceToggled:));
    body.addSubview(&announce);
    place_explanation(
        mtm,
        &body,
        crate::locale::t("announce_confirmations_note"),
        &mut top,
    );
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let y = take(&mut top, BUTTON_H);
    let ask = checkbox(mtm, crate::locale::t("confirm_clear"), y);
    ask.setState(state_of(settings::confirm_clear()));
    wire(&ask, sel!(confirmClearToggled:));
    body.addSubview(&ask);
    place_explanation(mtm, &body, crate::locale::t("confirm_clear_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    place_label(
        mtm,
        &body,
        crate::locale::t("symbols"),
        take(&mut top, BUTTON_H),
        WIDTH - MARGIN * 2.0,
    );
    place_explanation(mtm, &body, crate::locale::t("symbols_note"), &mut top);
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let field = text_field(mtm, take(&mut top, BUTTON_H));
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
    gap(&mut top, mac_ui::metrics::SPACE_S);
    let restore = GlassButton::pill(mtm, crate::locale::t("symbols_restore"), ButtonSize::Small);
    restore.set_symbol("arrow.counterclockwise");
    restore.set_accessibility_label(crate::locale::t("symbols_restore"));
    place_pill(&restore, take(&mut top, BUTTON_H), 240.0);
    wire(restore.button(), sel!(restoreSymbols:));
    body.addSubview(restore.view());

    gap(&mut top, SECTION_GAP);
    place_heading(
        mtm,
        &body,
        crate::locale::t("group_feedback"),
        take(&mut top, HEAD_H),
    );
    let row = take(&mut top, BUTTON_H);
    let mut x = MARGIN;
    for (title, symbol, action) in [
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
    ] {
        let pill = GlassButton::pill(mtm, title, ButtonSize::Small);
        pill.set_symbol(symbol);
        pill.set_accessibility_label(title);
        let width = pill
            .width_within((WIDTH - MARGIN * 2.0) / 2.0 - 4.0)
            .max(BUTTON_H);
        pill.view().setFrame(NSRect::new(
            NSPoint::new(x, row),
            NSSize::new(width, BUTTON_H),
        ));
        x += width + mac_ui::metrics::BUTTON_GAP;
        wire(pill.button(), action);
        body.addSubview(pill.view());
    }
    debug_assert!((top - MARGIN).abs() < 0.5);

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
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

fn place_heading(mtm: MainThreadMarker, parent: &NSView, title: &str, y: f64) {
    let field = widgets::label(mtm, mac_ui::metrics::FONT_TITLE, &NSColor::labelColor());
    let semibold = mac_ui::fonts::semibold_weight();
    field.setFont(Some(&NSFont::systemFontOfSize_weight(
        mac_ui::metrics::FONT_TITLE,
        semibold,
    )));
    field.setStringValue(&NSString::from_str(title));
    field.setFrame(NSRect::new(
        NSPoint::new(MARGIN, y),
        NSSize::new(WIDTH - MARGIN * 2.0, HEAD_H),
    ));
    parent.addSubview(&field);
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

fn refresh_ignore_button() {
    let enabled = crate::capture::last_foreign().is_some();
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
