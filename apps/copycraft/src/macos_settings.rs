//! Settings window: hotkey, date order, blur, Open at Login.

use std::cell::{Cell, RefCell};

use global_hotkey::hotkey::{HotKey, Modifiers};
use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSEvent,
    NSEventModifierFlags, NSPopUpButton, NSTextField, NSView, NSWindow, NSWindowDelegate,
    NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use mac_ui::{panel, widgets};

use crate::hotkey;
use crate::launcher::{self, UserEvent};
use crate::macos_login;
use crate::settings;

const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 300.0;
const PAD: f64 = 20.0;
const ROW: f64 = 28.0;
const GAP: f64 = 16.0;
const RADIUS: f64 = 16.0;

thread_local! {
    static WINDOW: RefCell<Option<Retained<SettingsWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<SettingsDelegate>>> = const { RefCell::new(None) };
    static HOTKEY_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static DATE_POPUP: RefCell<Option<Retained<NSPopUpButton>>> = const { RefCell::new(None) };
    static BLUR_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static LOGIN_NOTE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static RECORDING: Cell<bool> = const { Cell::new(false) };
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
            RECORDING.set(false);
            WINDOW.with(|slot| *slot.borrow_mut() = None);
            clear_controls();
        }
    }

    impl SettingsDelegate {
        #[unsafe(method(changeHotkey:))]
        fn change_hotkey(&self, _sender: Option<&NSButton>) {
            RECORDING.set(true);
            set_hotkey_text("Press a shortcut… (Esc cancels)");
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

        #[unsafe(method(blurToggled:))]
        fn blur_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            settings::set_blur(button.state() == NSControlStateValueOn);
            launcher::emit(UserEvent::SettingsChanged);
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
    LOGIN_BOX.with(|s| *s.borrow_mut() = None);
    LOGIN_NOTE.with(|s| *s.borrow_mut() = None);
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
        set_hotkey_text("Need ⌃ or ⌘ (not Option alone) — try again");
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
    window.setTitle(&NSString::from_str("Copycraft Settings"));
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

    place_label(mtm, &body, "Hotkey", PAD, y);
    let value = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
    value.setStringValue(&NSString::from_str(&settings::hotkey_label()));
    value.setFrame(NSRect::new(
        NSPoint::new(PAD + 110.0, y),
        NSSize::new(180.0, ROW),
    ));
    body.addSubview(&value);
    HOTKEY_LABEL.with(|slot| *slot.borrow_mut() = Some(value));
    let change = GlassButton::pill(mtm, "Change…", ButtonSize::Small);
    change.set_accessibility_label("Change hotkey");
    let change_w = change.width_within(100.0);
    change.view().setFrame(NSRect::new(
        NSPoint::new(WIDTH - PAD - change_w, y),
        NSSize::new(change_w, ROW),
    ));
    wire(change.button(), sel!(changeHotkey:));
    body.addSubview(change.view());

    y -= ROW + GAP;
    place_label(mtm, &body, "Date order", PAD, y);
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(
            NSPoint::new(PAD + 110.0, y - 2.0),
            NSSize::new(200.0, ROW + 4.0),
        ),
        false,
    );
    popup.removeAllItems();
    popup.addItemWithTitle(&NSString::from_str("dd/mm/yyyy"));
    popup.addItemWithTitle(&NSString::from_str("mm/dd/yyyy"));
    popup.selectItemAtIndex(isize::from(settings::load().date_month_first));
    wire(popup.as_ref(), sel!(dateOrderChanged:));
    body.addSubview(&popup);
    DATE_POPUP.with(|slot| *slot.borrow_mut() = Some(popup));

    y -= ROW + GAP;
    let blur = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str("Blur masked content"),
            None,
            None,
            mtm,
        )
    };
    blur.setState(if settings::load().blur {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
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
            &NSString::from_str("Open at Login"),
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

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    panel::bring_to_front(&window);
}

fn place_label(mtm: MainThreadMarker, parent: &NSView, title: &str, x: f64, y: f64) {
    let field = widgets::label(mtm, 13.0, &NSColor::labelColor());
    field.setStringValue(&NSString::from_str(title));
    field.setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(100.0, ROW)));
    parent.addSubview(&field);
}

fn refresh_all() {
    set_hotkey_text(&settings::hotkey_label());
    DATE_POPUP.with(|slot| {
        if let Some(popup) = slot.borrow().as_ref() {
            popup.selectItemAtIndex(isize::from(settings::load().date_month_first));
        }
    });
    BLUR_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(if settings::load().blur {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    });
    refresh_login_row();
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
        macos_login::LoginStatus::RequiresApproval => {
            "Waiting for approval in System Settings › Login Items"
        }
        macos_login::LoginStatus::Unavailable => "Open at Login needs macOS 13 or later",
        macos_login::LoginStatus::NotFound => "Open at Login is not available for this build",
        _ => "Starts Copycraft when you log in (off by default)",
    };
    LOGIN_NOTE.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setStringValue(&NSString::from_str(note));
        }
    });
}

/// Close the Settings window (Wipe, Quit).
pub fn close() {
    RECORDING.set(false);
    if let Some(window) = WINDOW.with(|slot| slot.borrow_mut().take()) {
        window.close();
    }
    clear_controls();
    DELEGATE.with(|slot| *slot.borrow_mut() = None);
}
