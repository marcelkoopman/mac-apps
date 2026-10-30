//! The first-run welcome window: the hotkey, the screenshot hotkey, paste-first (⌘V or a drop), the offline promise, what
//! stays out of history, "Keep clipboard history" (off on a new install), Open at Login, and buttons for Settings and "Get Started". Shown once (see
//! [`crate::settings::should_show_welcome`]) and again from the status menu.

use std::cell::{Cell, RefCell};

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyObject, Sel};
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSAccessibility, NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua,
    NSApplication, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor,
    NSControlStateValueOff, NSControlStateValueOn, NSEvent, NSFont, NSLayoutAttribute, NSStackView,
    NSStackViewDistribution, NSTextAlignment, NSTextField, NSUserInterfaceLayoutOrientation,
    NSView, NSWindow, NSWindowDelegate, NSWindowLevel, NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSArray, NSEdgeInsets, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use mac_ui::{panel, widgets};

use crate::commands::CommandId;
use crate::launcher::{self, UserEvent};
use crate::macos_login;

const WIDTH: f64 = 420.0;
const PAD: f64 = 24.0;
const ICON: f64 = 96.0;
/// Outer part of the icon's radius that fades to the window colour.
const ICON_FADE: f64 = 0.25;
/// Edge colour of `assets/icon.icns`, sRGB #07112D: the mean of the outer 3 px ring of the
/// 1024 px rep (the tile is a soft gradient, #030921 in the corners to #0E1B3F at the
/// edges). Made with `colorWithSRGBRed`, the image's colour space, with nothing over it.
const ICON_BACKGROUND: (f64, f64, f64) = (7.0 / 255.0, 17.0 / 255.0, 45.0 / 255.0);
/// The app icon as `assets/icon.icns` (the glass design `assets/AppIcon.icon` is drawn after).
/// The bundle no longer ships this file: its icon is compiled from `AppIcon.icon`
/// (`scripts/build_app_icon.sh`), so the artwork is built into the binary for this window.
const ICON_ICNS: &[u8] = include_bytes!("../assets/icon.icns");
/// Width of the labels and the button row: the window width minus the side margins.
const INNER_W: f64 = WIDTH - PAD * 2.0;
/// Escape (`kVK_Escape`).
const ESCAPE_KEY_CODE: u16 = 53;

thread_local! {
    static WINDOW: RefCell<Option<Retained<WelcomeWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<WelcomeDelegate>>> = const { RefCell::new(None) };
    static LOGIN_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static HISTORY_BOX: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    /// Level before [`crate::macos_launcher::raise_above_card`], restored on close.
    static LEVEL_BEFORE: Cell<Option<NSWindowLevel>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftWelcomeWindow"]
    struct WelcomeWindow;

    impl WelcomeWindow {
        /// Escape closes the window, like "Get Started" (Return is its key equivalent).
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
                // Not released while it is still closing.
                panel::release_after_close(Retained::into_super(window));
            }
            LOGIN_BOX.with(|slot| *slot.borrow_mut() = None);
            HISTORY_BOX.with(|slot| *slot.borrow_mut() = None);
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

        /// "Keep clipboard history": the same setting as in Settings › Privacy.
        #[unsafe(method(welcomeHistoryToggled:))]
        fn history_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            crate::settings::set_clipboard_history(button.state() == NSControlStateValueOn);
            launcher::emit(UserEvent::ClipboardHistoryChanged);
        }

        #[unsafe(method(welcomeLoginToggled:))]
        fn login_toggled(&self, sender: Option<&NSButton>) {
            let Some(button) = sender else {
                return;
            };
            let on = button.state() == NSControlStateValueOn;
            if let Err(message) = macos_login::set_enabled(on) {
                crate::diag::log(crate::diag::LogEvent::LoginItemFailed(crate::diag::ErrorKind::from_text(&message)));
                // Back to the real state first, so the box never shows a change that failed.
                refresh_login_box();
                mac_ui::dialog::alert(
                    self.mtm(),
                    crate::locale::t("login_change_failed"),
                    &crate::locale::tf("welcome_login_failed", &[&message]),
                );
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
            mac_ui::actions::connect(control, delegate, action);
        }
    });
}

fn push_button(mtm: MainThreadMarker, title: &str, action: Sel) -> Retained<NSButton> {
    let button = mac_ui::actions::push_button(mtm, title);
    button.setAccessibilityLabel(Some(&NSString::from_str(title)));
    wire(&button, action);
    button
}

/// A wrapping label as wide as the content: it grows downwards for long (Dutch) text and
/// large system text instead of being cut off.
fn text_line(mtm: MainThreadMarker, text: &str, font: &NSFont) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(font));
    label.setTextColor(Some(&NSColor::labelColor()));
    label.setSelectable(false);
    label.setPreferredMaxLayoutWidth(INNER_W);
    label
        .widthAnchor()
        .constraintEqualToConstant(INNER_W)
        .setActive(true);
    label
}

fn build(mtm: MainThreadMarker) {
    // Miniaturizable: ⌘M (the hidden Window menu, src/macos_main_menu.rs) minimises it.
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, 320.0));
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

    let delegate = WelcomeDelegate::new(mtm);
    // The delegate is stored in DELEGATE right below and outlives the window.
    window.setDelegate(Some(mac_ui::objc2::runtime::ProtocolObject::from_ref(
        &*delegate,
    )));
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    // The window matches the icon's dark navy, so it always uses the dark appearance (light
    // text and controls) and the title bar blends into the same colour.
    // SAFETY: NSAppearanceNameDarkAqua is an immutable AppKit string constant.
    let dark = unsafe { NSAppearanceNameDarkAqua };
    window.setAppearance(NSAppearance::appearanceNamed(dark).as_deref());
    window.setTitlebarAppearsTransparent(true);
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
        ICON_BACKGROUND.0,
        ICON_BACKGROUND.1,
        ICON_BACKGROUND.2,
        1.0,
    )));

    let icon = widgets::image_view(mtm);
    if let Some(image) = feathered_icon()
        .or_else(|| mac_ui::image::from_bytes(ICON_ICNS))
        .or_else(|| NSApplication::sharedApplication(mtm).applicationIconImage())
    {
        icon.setImage(Some(&image));
    }
    icon.widthAnchor()
        .constraintEqualToConstant(ICON)
        .setActive(true);
    icon.heightAnchor()
        .constraintEqualToConstant(ICON)
        .setActive(true);
    icon.setAccessibilityElement(false);

    let heading = text_line(mtm, title, &NSFont::boldSystemFontOfSize(20.0));
    heading.setAlignment(NSTextAlignment::Center);

    let body_font = NSFont::systemFontOfSize(NSFont::systemFontSize());
    let points = [
        crate::locale::tf("welcome_hotkey", &[&crate::settings::hotkey_label()]),
        // The chord as stored (changed in Settings › General), like the card's above.
        crate::locale::tf(
            "welcome_screenshot",
            &[&crate::settings::screenshot_hotkey_label()],
        ),
        crate::locale::t("welcome_history").to_string(),
        crate::locale::t("welcome_local").to_string(),
        crate::locale::t("welcome_sensitive").to_string(),
        crate::locale::tf("welcome_way_back", &[&crate::settings::hotkey_label()]),
    ];
    let point_views: Vec<Retained<NSTextField>> = points
        .iter()
        .map(|point| text_line(mtm, &format!("•  {point}"), &body_font))
        .collect();

    let history = mac_ui::actions::checkbox(mtm, crate::locale::t("clipboard_history"));
    history.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t(
        "clipboard_history",
    ))));
    wire(&history, sel!(welcomeHistoryToggled:));
    HISTORY_BOX.with(|slot| *slot.borrow_mut() = Some(history.clone()));
    refresh_history_box();
    let history_note = text_line(
        mtm,
        crate::locale::t("welcome_history_choice"),
        &NSFont::systemFontOfSize(NSFont::smallSystemFontSize()),
    );
    history_note.setTextColor(Some(&NSColor::secondaryLabelColor()));

    let login = mac_ui::actions::checkbox(mtm, crate::locale::t("open_at_login"));
    login.setAccessibilityLabel(Some(&NSString::from_str(crate::locale::t("open_at_login"))));
    wire(&login, sel!(welcomeLoginToggled:));
    LOGIN_BOX.with(|slot| *slot.borrow_mut() = Some(login.clone()));
    refresh_login_box();

    let settings = push_button(mtm, crate::locale::t("settings"), sel!(welcomeSettings:));
    let start = push_button(mtm, crate::locale::t("welcome_start"), sel!(welcomeStart:));
    // Return triggers it and draws it as the default button.
    start.setKeyEquivalent(&NSString::from_str("\r"));
    // Settings on the left, "Get Started" on the right, across the full content width.
    let settings_view: &NSView = &settings;
    let start_view: &NSView = &start;
    let buttons =
        NSStackView::stackViewWithViews(&NSArray::from_slice(&[settings_view, start_view]), mtm);
    buttons.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
    buttons.setDistribution(NSStackViewDistribution::EqualSpacing);
    buttons
        .widthAnchor()
        .constraintEqualToConstant(INNER_W)
        .setActive(true);

    let icon_view: &NSView = &icon;
    let heading_view: &NSView = &heading;
    let history_view: &NSView = &history;
    let history_note_view: &NSView = &history_note;
    let login_view: &NSView = &login;
    let buttons_view: &NSView = &buttons;
    let mut views: Vec<&NSView> = vec![icon_view, heading_view];
    for view in &point_views {
        let view: &NSView = view;
        views.push(view);
    }
    views.push(history_view);
    views.push(history_note_view);
    views.push(login_view);
    views.push(buttons_view);
    let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setAlignment(NSLayoutAttribute::CenterX);
    stack.setSpacing(10.0);
    stack.setEdgeInsets(NSEdgeInsets {
        top: PAD,
        left: PAD,
        bottom: PAD,
        right: PAD,
    });
    stack.setCustomSpacing_afterView(14.0, &icon);
    stack.setCustomSpacing_afterView(16.0, &heading);
    if let Some(last) = point_views.last() {
        stack.setCustomSpacing_afterView(18.0, last);
    }
    stack.setCustomSpacing_afterView(4.0, &history);
    stack.setCustomSpacing_afterView(14.0, &history_note);
    stack.setCustomSpacing_afterView(20.0, &login);

    // The content decides the height; the width stays WIDTH.
    let height = stack.fittingSize().height.ceil().max(200.0);
    window.setContentSize(NSSize::new(WIDTH, height));
    stack.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(WIDTH, height),
    ));
    stack.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    window.setContentView(Some(&stack));
    window.center();

    // Tab moves through both checkboxes and both buttons; focus starts on "Get Started".
    window.setAutorecalculatesKeyViewLoop(true);
    window.setInitialFirstResponder(Some(&start));

    WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));
    raise_above_card(&window);
    panel::bring_to_front(&window);
}

/// The history checkbox follows the setting (also when Settings changes it).
pub fn refresh_history_box() {
    let on = crate::settings::clipboard_history();
    HISTORY_BOX.with(|slot| {
        if let Some(button) = slot.borrow().as_ref() {
            button.setState(if on {
                NSControlStateValueOn
            } else {
                NSControlStateValueOff
            });
        }
    });
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

/// The icon with its edge faded to transparent over the outer quarter (see
/// [`crate::image_edit::feather_radial`]): the tile's own vignette is lighter in the middle,
/// so no flat window colour matches its edge and a square would show. Rendered once, at 3×.
/// `icon.icns` itself is untouched.
fn feathered_icon() -> Option<Retained<mac_ui::objc2_app_kit::NSImage>> {
    use crate::image_edit::{Codec, ImageOp, Resize, feather_radial};
    let codec = crate::macos_image_edit::ImageIoCodec;
    let full = codec.decode(ICON_ICNS)?;
    let small = match ImageOp::Resize(Resize::Longest((ICON * 3.0) as u32)).apply(full) {
        Ok(image) | Err(image) => image,
    };
    let png = codec.encode_png(&feather_radial(small, ICON_FADE))?;
    let image = mac_ui::image::from_bytes(&png)?;
    image.setSize(NSSize::new(ICON, ICON));
    Some(image)
}
