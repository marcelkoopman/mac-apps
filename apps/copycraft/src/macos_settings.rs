//! Settings window: hotkey, date order, language, privacy filter, blur, Open at Login, and the
//! symbol list.

use std::cell::{Cell, RefCell};

use global_hotkey::hotkey::{HotKey, Modifiers};
use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventModifierFlags, NSPopUpButton, NSTextField, NSView,
    NSWindow, NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use mac_ui::{panel, widgets};

use crate::hotkey;
use crate::launcher::{self, UserEvent};
use crate::macos_login;
use crate::settings;

const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 468.0;
const PAD: f64 = 20.0;
const ROW: f64 = 28.0;
const GAP: f64 = 16.0;
const RADIUS: f64 = 16.0;

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
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
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

    unsafe impl NSObjectProtocol for SettingsDelegate {}

    unsafe impl NSWindowDelegate for SettingsDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSObject) {
            // Closing ends editing, but save here too so a list left in the field is kept.
            save_symbols_field();
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
        }

        #[unsafe(method(restoreSymbols:))]
        fn restore_symbols(&self, _sender: Option<&NSButton>) {
            settings::restore_symbol_catalog();
            show_symbol_catalog();
            crate::macos_symbols::reload();
        }

        #[unsafe(method(loginToggled:))]
        fn login_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            if let Err(message) = macos_login::set_enabled(on) {
                eprintln!("copycraft: open at login: {message}");
            }
            refresh_login_row();
        }
    }
);

impl SettingsDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars);
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
}

fn wire(control: &NSButton, action: mac_ui::objc2::runtime::Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: DELEGATE keeps the target alive while the window is open.
            unsafe {
                control.setTarget(Some(delegate));
                control.setAction(Some(action));
            }
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
        mods |= Modifiers::SUPER;
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
    let window: Retained<SettingsWindow> = unsafe {
        msg_send![
            SettingsWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(crate::locale::t("settings_title")));
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();

    let delegate = SettingsDelegate::new(mtm);
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let body = panel::rounded_glass(mtm, &content, RADIUS).content;

    let mut y = HEIGHT - PAD - ROW;

    place_label(mtm, &body, crate::locale::t("hotkey"), PAD, y);
    let value = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
    value.setStringValue(&NSString::from_str(&settings::hotkey_label()));
    value.setFrame(NSRect::new(
        NSPoint::new(PAD + 110.0, y),
        NSSize::new(180.0, ROW),
    ));
    body.addSubview(&value);
    HOTKEY_LABEL.with(|slot| *slot.borrow_mut() = Some(value));
    let change = GlassButton::pill(mtm, crate::locale::t("change_hotkey"), ButtonSize::Small);
    change.set_accessibility_label(crate::locale::t("change_hotkey_a11y"));
    let change_w = change.width_within(100.0);
    change.view().setFrame(NSRect::new(
        NSPoint::new(WIDTH - PAD - change_w, y),
        NSSize::new(change_w, ROW),
    ));
    wire(change.button(), sel!(changeHotkey:));
    body.addSubview(change.view());

    y -= ROW + GAP;
    place_label(mtm, &body, crate::locale::t("date_order"), PAD, y);
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(PAD + 110.0, y - 2.0),
            NSSize::new(200.0, ROW + 4.0),
        ),
        false,
    );
    popup.removeAllItems();
    popup.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_dmy")));
    popup.addItemWithTitle(&NSString::from_str(crate::locale::t("date_order_mdy")));
    popup.selectItemAtIndex(isize::from(settings::load().date_month_first));
    wire(popup.as_ref(), sel!(dateOrderChanged:));
    body.addSubview(&popup);
    DATE_POPUP.with(|slot| *slot.borrow_mut() = Some(popup));

    y -= ROW + GAP;
    place_label(mtm, &body, crate::locale::t("language"), PAD, y);
    let language = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(PAD + 110.0, y - 2.0),
            NSSize::new(200.0, ROW + 4.0),
        ),
        false,
    );
    language.removeAllItems();
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_system")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_nl")));
    language.addItemWithTitle(&NSString::from_str(crate::locale::t("language_en")));
    language.selectItemAtIndex(language_index(settings::language()));
    wire(language.as_ref(), sel!(languageChanged:));
    body.addSubview(&language);

    y -= ROW + GAP;
    let privacy = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(crate::locale::t("privacy_filter")),
            None,
            None,
            mtm,
        )
    };
    privacy.setState(state_of(settings::privacy_filter()));
    privacy.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, ROW),
    ));
    wire(&privacy, sel!(privacyFilterToggled:));
    body.addSubview(&privacy);
    PRIVACY_BOX.with(|slot| *slot.borrow_mut() = Some(privacy));

    y -= ROW + GAP;
    let blur = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(crate::locale::t("blur_masked")),
            None,
            None,
            mtm,
        )
    };
    blur.setState(state_of(settings::load().blur));
    // The blur only styles the privacy filter's mask.
    blur.setEnabled(settings::privacy_filter());
    blur.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, ROW),
    ));
    wire(&blur, sel!(blurToggled:));
    body.addSubview(&blur);
    BLUR_BOX.with(|slot| *slot.borrow_mut() = Some(blur));

    y -= ROW + GAP;
    let login = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(crate::locale::t("open_at_login")),
            None,
            None,
            mtm,
        )
    };
    login.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, ROW),
    ));
    wire(&login, sel!(loginToggled:));
    body.addSubview(&login);
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login));

    y -= ROW;
    let note = widgets::label(mtm, 11.0, &NSColor::secondaryLabelColor());
    note.setFrame(NSRect::new(
        NSPoint::new(PAD + 24.0, y - 2.0),
        NSSize::new(WIDTH - PAD * 2.0 - 24.0, ROW),
    ));
    body.addSubview(&note);
    LOGIN_NOTE.with(|slot| *slot.borrow_mut() = Some(note));
    refresh_login_row();

    y -= GAP + ROW;
    place_label(mtm, &body, crate::locale::t("symbols"), PAD, y);
    y -= ROW;
    let field = widgets::plain_field(mtm, 13.0, "");
    field.setBezeled(true);
    field.setDrawsBackground(true);
    field.setStringValue(&NSString::from_str(&settings::symbol_catalog_text()));
    field.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, ROW),
    ));
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

    y -= GAP + ROW;
    let restore = GlassButton::pill(mtm, crate::locale::t("symbols_restore"), ButtonSize::Small);
    restore.set_accessibility_label(crate::locale::t("symbols_restore"));
    let restore_w = restore.width_within(240.0);
    restore.view().setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(restore_w, ROW),
    ));
    wire(restore.button(), sel!(restoreSymbols:));
    body.addSubview(restore.view());

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
}

fn raise_above_card(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::raise_above_card(window, previous));
}

fn restore_level(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::restore_above_card(window, previous));
}

fn place_label(mtm: MainThreadMarker, parent: &NSView, title: &str, x: f64, y: f64) {
    let field = widgets::label(mtm, 13.0, &NSColor::labelColor());
    field.setStringValue(&NSString::from_str(title));
    field.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(100.0, ROW)));
    parent.addSubview(&field);
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
