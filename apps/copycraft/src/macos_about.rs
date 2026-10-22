//! About CopyCraft: name, bundle version, offline promise, GitHub URL as plain text (not opened),
//! and buttons that open the privacy policy and the support page in the default browser.

use std::cell::{Cell, RefCell};

use mac_ui::button::{ButtonSize, GlassButton};
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use mac_ui::objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSView, NSWindow, NSWindowDelegate, NSWindowLevel,
    NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSBundle, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use mac_ui::{panel, widgets};

use crate::commands::CommandId;
use crate::launcher::{self, UserEvent};

const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 250.0;
/// Height of the row with the privacy and support buttons.
const LINK_H: f64 = mac_ui::metrics::BUTTON_H;
const PAD: f64 = 20.0;
const RADIUS: f64 = 16.0;

/// Shown as plain text; the user may copy or open it themselves. CopyCraft does not.
pub const GITHUB_URL: &str = "https://github.com/marcelkoopman/mac-apps";

thread_local! {
    static WINDOW: RefCell<Option<Retained<AboutWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<AboutDelegate>>> = const { RefCell::new(None) };
    /// Level before [`crate::macos_launcher::raise_above_card`], restored on close.
    static LEVEL_BEFORE: Cell<Option<NSWindowLevel>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftAboutWindow"]
    struct AboutWindow;

    impl AboutWindow {
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            if mac_ui::keys::is_command_chord(event, "o") {
                launcher::emit(UserEvent::Run(CommandId::ChooseFile));
                true
            } else if mac_ui::keys::is_command_chord(event, "s") {
                launcher::emit(UserEvent::Run(CommandId::Save));
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
    #[name = "CopycraftAboutDelegate"]
    #[ivars = DelegateIvars]
    struct AboutDelegate;

    // SAFETY: NSObject conforms to NSObjectProtocol and this subclass changes nothing about it.
    unsafe impl NSObjectProtocol for AboutDelegate {}

    // SAFETY: the methods below use the NSWindowDelegate selectors and signatures.
    unsafe impl NSWindowDelegate for AboutDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSObject) {
            WINDOW.with(|slot| {
                if let Some(window) = slot.borrow().as_ref() {
                    restore_level(window);
                }
                *slot.borrow_mut() = None;
            });
        }
    }

    impl AboutDelegate {
        #[unsafe(method(openPrivacy:))]
        fn open_privacy(&self, _sender: Option<&NSObject>) {
            open_page(crate::url_policy::PRIVACY_URL);
        }

        #[unsafe(method(openSupport:))]
        fn open_support(&self, _sender: Option<&NSObject>) {
            open_page(crate::url_policy::SUPPORT_URL);
        }
    }
);

/// Open one of CopyCraft's own pages (privacy policy, support) in the default browser through
/// `NSWorkspace` ([`crate::macos_open::open_web_url`]): the browser loads it, CopyCraft makes no
/// connection. When the system cannot open it, an alert shows the address to open by hand.
pub(crate) fn open_page(url: &str) {
    if crate::macos_open::open_web_url(url) {
        return;
    }
    crate::diag::log(crate::diag::LogEvent::VisitFailed);
    if let Some(mtm) = MainThreadMarker::new() {
        mac_ui::dialog::alert(mtm, crate::locale::t("link_open_failed"), url);
    }
}

impl AboutDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars);
        // SAFETY: NSObject's `init` on the freshly allocated instance; the class declares no other
        // initializer.
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) fn bundle_version() -> String {
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString"))
        .and_then(|value| value.downcast_ref::<NSString>().map(|s| s.to_string()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Open (or bring to front) the About window.
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

fn build(mtm: MainThreadMarker) {
    // Miniaturizable: ⌘M (the hidden Window menu, src/macos_main_menu.rs) minimises it.
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT));
    // SAFETY: initWithContentRect:styleMask:backing:defer: on a freshly allocated window, with
    // plain value arguments.
    let window: Retained<AboutWindow> = unsafe {
        msg_send![
            AboutWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    mac_ui::panel::keep_when_closed(&window);
    window.setTitle(&NSString::from_str(crate::locale::t("about_title")));
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();

    let delegate = AboutDelegate::new(mtm);
    // The delegate is stored in DELEGATE right below and outlives the window.
    window.setDelegate(Some(mac_ui::objc2::runtime::ProtocolObject::from_ref(
        &*delegate,
    )));
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let body = panel::rounded_glass(mtm, &content, RADIUS).content;

    let mut y = HEIGHT - PAD - 28.0;
    let name = widgets::label(mtm, 20.0, &NSColor::labelColor());
    name.setStringValue(&NSString::from_str("CopyCraft"));
    name.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, 28.0),
    ));
    body.addSubview(&name);

    y -= 28.0;
    let version = widgets::label(mtm, 13.0, &NSColor::secondaryLabelColor());
    version.setStringValue(&NSString::from_str(&format!(
        "{} {}",
        crate::locale::t("version"),
        bundle_version()
    )));
    version.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, 22.0),
    ));
    body.addSubview(&version);

    y -= 40.0;
    let promise = widgets::label(mtm, 13.0, &NSColor::labelColor());
    promise.setStringValue(&NSString::from_str(crate::locale::t("offline_promise")));
    promise.setUsesSingleLineMode(false);
    promise.setLineBreakMode(mac_ui::objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    promise.setFrame(NSRect::new(
        NSPoint::new(PAD, y - 24.0),
        NSSize::new(WIDTH - PAD * 2.0, 48.0),
    ));
    body.addSubview(&promise);

    y -= 56.0;
    let url = widgets::label(mtm, 12.0, &NSColor::linkColor());
    url.setSelectable(true);
    url.setStringValue(&NSString::from_str(GITHUB_URL));
    url.setFrame(NSRect::new(
        NSPoint::new(PAD, y),
        NSSize::new(WIDTH - PAD * 2.0, 22.0),
    ));
    body.addSubview(&url);

    // Privacy policy and Support, side by side on the bottom row.
    let mut x = PAD;
    for (title, symbol, action) in [
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
    ] {
        let pill = GlassButton::pill(mtm, title, ButtonSize::Small);
        pill.set_symbol(symbol);
        pill.set_accessibility_label(title);
        let width = pill
            .width_within((WIDTH - PAD * 2.0) / 2.0 - 4.0)
            .max(LINK_H);
        pill.view().setFrame(NSRect::new(
            NSPoint::new(x, PAD),
            NSSize::new(width, LINK_H),
        ));
        x += width + mac_ui::metrics::BUTTON_GAP;
        DELEGATE.with(|slot| {
            if let Some(delegate) = slot.borrow().as_ref() {
                mac_ui::actions::connect(pill.button(), delegate, action);
            }
        });
        body.addSubview(pill.view());
    }

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

/// Close and open About again so its text follows the new language.
/// Reduce Transparency changed: cover or uncover the glass of the open window.
pub fn restyle() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if let Some(window) = WINDOW.with(|slot| slot.borrow().clone()) {
        mac_ui::glass::restyle_window(mtm, &window, RADIUS);
    }
}

pub fn reopen() {
    if WINDOW.with(|slot| slot.borrow().is_none()) {
        return;
    }
    close();
    show();
}

/// Close the About window (Wipe, Quit).
pub fn close() {
    if let Some(window) = WINDOW.with(|slot| slot.borrow_mut().take()) {
        restore_level(&window);
        window.close();
    }
    DELEGATE.with(|slot| *slot.borrow_mut() = None);
}
