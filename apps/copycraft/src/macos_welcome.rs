//! The first-run welcome window: the hotkey, the offline promise, the privacy filter, Open at
//! Login, and buttons for Settings and "Get started". Shown once (see
//! [`crate::settings::should_show_welcome`]) and again from the status menu.

use std::cell::{Cell, RefCell};

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSApplication, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSFont, NSLineBreakMode, NSTextAlignment, NSTextField, NSView,
    NSWindow, NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use mac_ui::{panel, widgets};

use crate::commands::CommandId;
use crate::launcher::{self, UserEvent};
use crate::macos_login;

const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 360.0;
const PAD: f64 = 24.0;
const RADIUS: f64 = 16.0;
const ICON: f64 = 64.0;
const BUTTON_W: f64 = 140.0;
const BUTTON_H: f64 = 28.0;
/// Escape (`kVK_Escape`).
const ESCAPE_KEY_CODE: u16 = 53;

thread_local! {
    static WINDOW: RefCell<Option<Retained<WelcomeWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<WelcomeDelegate>>> = const { RefCell::new(None) };
    static LOGIN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    /// Level before [`crate::macos_launcher::raise_above_card`], restored on close.
    static LEVEL_BEFORE: Cell<Option<NSWindowLevel>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftWelcomeWindow"]
    struct WelcomeWindow;

    impl WelcomeWindow {
        /// Escape closes the window, like "Get started" (Return is its key equivalent).
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if event.keyCode() == ESCAPE_KEY_CODE {
                close();
                true
            } else {
                // SAFETY: forwards the event to the superclass implementation of the same
                // selector, with the same argument and return types.
                let handled: bool =
                    unsafe { msg_send![super(self), performKeyEquivalent: event] };
                handled
            }
        }

        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            close();
        }
    }
);

struct DelegateIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftWelcomeDelegate"]
    #[ivars = DelegateIvars]
    struct WelcomeDelegate;

    // SAFETY: NSObject conforms to NSObjectProtocol and this subclass changes nothing about it.
    unsafe impl NSObjectProtocol for WelcomeDelegate {}

    // SAFETY: the methods below use the NSWindowDelegate selectors and signatures.
    unsafe impl NSWindowDelegate for WelcomeDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSObject) {
            crate::settings::mark_welcome_seen();
            let window = WINDOW.with(|slot| slot.borrow_mut().take());
            if let Some(window) = window {
                restore_level(&window);
            }
            LOGIN_BOX.with(|slot| *slot.borrow_mut() = None);
        }
    }

    impl WelcomeDelegate {
        #[unsafe(method(welcomeStart:))]
        fn start(&self, _sender: Option<&NSButton>) {
            close();
        }

        #[unsafe(method(welcomeSettings:))]
        fn open_settings(&self, _sender: Option<&NSButton>) {
            launcher::emit(UserEvent::Run(CommandId::Settings));
        }

        #[unsafe(method(welcomeLoginToggled:))]
        fn login_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            if let Err(message) = macos_login::set_enabled(on) {
                eprintln!("copycraft: open at login: {message}");
            }
            refresh_login_box();
        }
    }
);

impl WelcomeDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars);
        // SAFETY: NSObject's `init` on the freshly allocated instance; the class declares no other
        // initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Open (or bring to front) the welcome window.
pub fn show() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    panel::activate_app(mtm);
    if let Some(window) = WINDOW.with(|slot| slot.borrow().clone()) {
        raise_above_card(&window);
        panel::bring_to_front(&window);
        return;
    }
    build(mtm);
}

/// Close and open it again so its text follows the new language.
pub fn reopen() {
    if WINDOW.with(|slot| slot.borrow().is_none()) {
        return;
    }
    close();
    show();
}

/// Close the welcome window (Get started, Escape). Marks it as seen.
pub fn close() {
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let Some(window) = window {
        // `windowWillClose:` marks it seen, restores the level and clears the slots.
        window.close();
    }
}

fn wire(control: &NSButton, action: Sel) {
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            // SAFETY: the delegate implements `action` taking the sender, and DELEGATE keeps it
            // alive for as long as the window exists.
            unsafe { widgets::set_target_action(control, delegate, action) };
        }
    });
}

fn push_button(mtm: MainThreadMarker, title: &str, x: f64, action: Sel) -> Retained<NSButton> {
    // SAFETY: no target or action is passed here; `wire` sets them right below.
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm)
    };
    button.setFrame(NSRect::new(
        NSPoint::new(x, PAD),
        NSSize::new(BUTTON_W, BUTTON_H),
    ));
    button.setAccessibilityLabel(Some(&NSString::from_str(title)));
    wire(&button, action);
    button
}

fn text_line(
    mtm: MainThreadMarker,
    text: &str,
    size: f64,
    y: f64,
    height: f64,
) -> Retained<NSTextField> {
    let label = widgets::label(mtm, size, &NSColor::labelColor());
    label.setStringValue(&NSString::from_str(text));
    label.setUsesSingleLineMode(false);
    label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    label.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, height),
    ));
    label
}

fn build(mtm: MainThreadMarker) {
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT));
    // SAFETY: initWithContentRect:styleMask:backing:defer: on a freshly allocated window, with
    // plain value arguments.
    let window: Retained<WelcomeWindow> = unsafe {
        msg_send![
            WelcomeWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    panel::keep_when_closed(&window);
    let title = crate::locale::t("welcome_title");
    window.setTitle(&NSString::from_str(title));
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();

    let delegate = WelcomeDelegate::new(mtm);
    // SAFETY: the delegate is stored in DELEGATE right below and outlives the window, so the
    // window's weak delegate reference stays valid.
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let body = panel::rounded_glass(mtm, &content, RADIUS).content;

    let mut y = HEIGHT - PAD - ICON;
    let icon = widgets::image_view(mtm);
    if let Some(image) = NSApplication::sharedApplication(mtm).applicationIconImage() {
        icon.setImage(Some(&image));
    }
    icon.setFrame(NSRect::new(
        NSPoint::new((WIDTH - ICON) / 2.0, y),
        NSSize::new(ICON, ICON),
    ));
    icon.setAccessibilityElement(false);
    body.addSubview(&icon);

    y -= 36.0;
    let heading = text_line(mtm, title, 20.0, y, 28.0);
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(20.0)));
    heading.setAlignment(NSTextAlignment::Center);
    body.addSubview(&heading);

    let points = [
        crate::locale::tf("welcome_hotkey", &[&crate::settings::hotkey_label()]),
        crate::locale::t("welcome_local").to_string(),
        crate::locale::t("welcome_sensitive").to_string(),
    ];
    for point in &points {
        y -= 40.0;
        body.addSubview(&text_line(mtm, &format!("•  {point}"), 13.0, y, 36.0));
    }

    y -= 36.0;
    // SAFETY: no target or action is passed here; `wire` sets them right below.
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
        NSSize::new(WIDTH - PAD * 2.0, BUTTON_H),
    ));
    login.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("open_at_login"))));
    wire(&login, sel!(welcomeLoginToggled:));
    body.addSubview(&login);
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login.clone()));
    refresh_login_box();

    let settings = push_button(
        mtm,
        crate::locale::t("settings"),
        PAD,
        sel!(welcomeSettings:),
    );
    body.addSubview(&settings);
    let start = push_button(
        mtm,
        crate::locale::t("welcome_start"),
        WIDTH - PAD - BUTTON_W,
        sel!(welcomeStart:),
    );
    // Return triggers it and draws it as the default button.
    start.setKeyEquivalent(&NSString::from_str("\r"));
    body.addSubview(&start);

    // Tab moves through the checkbox and both buttons; focus starts on "Get started".
    window.setAutorecalculatesKeyViewLoop(true);
    window.setInitialFirstResponder(Some(&start));

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
}

/// The checkbox follows the login item: on when registered (or awaiting approval), disabled
/// when the service is unavailable.
fn refresh_login_box() {
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
}

fn raise_above_card(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::raise_above_card(window, previous));
}

fn restore_level(window: &NSWindow) {
    LEVEL_BEFORE.with(|previous| crate::macos_launcher::restore_above_card(window, previous));
}
