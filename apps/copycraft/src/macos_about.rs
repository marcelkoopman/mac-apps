//! About Copycraft: name, bundle version, offline promise, GitHub URL as plain text (not opened).

use std::cell::{Cell, RefCell};

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use mac_ui::objc2_app_kit::{
    NSBackingStoreType, NSColor, NSView, NSWindow, NSWindowDelegate, NSWindowLevel,
    NSWindowStyleMask, NSWindowTabbingMode,
};
use mac_ui::objc2_foundation::{
    NSBundle, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use mac_ui::{panel, widgets};

const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 220.0;
const PAD: f64 = 20.0;
const RADIUS: f64 = 16.0;

/// Shown as plain text; the user may copy or open it themselves. Copycraft does not.
pub const GITHUB_URL: &str = "https://github.com/marcelkoopman/mac-apps";

thread_local! {
    static WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
    static DELEGATE: RefCell<Option<Retained<AboutDelegate>>> = const { RefCell::new(None) };
    /// Level before [`crate::macos_launcher::raise_above_card`], restored on close.
    static LEVEL_BEFORE: Cell<Option<NSWindowLevel>> = const { Cell::new(None) };
}

struct DelegateIvars;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CopycraftAboutDelegate"]
    #[ivars = DelegateIvars]
    struct AboutDelegate;

    unsafe impl NSObjectProtocol for AboutDelegate {}

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
);

impl AboutDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars);
        unsafe { msg_send![super(this), init] }
    }
}

fn bundle_version() -> String {
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
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, HEIGHT));
    let window = unsafe {
        msg_send![
            NSWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    let window: Retained<NSWindow> = window;
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(crate::locale::t("about_title")));
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    window.setSharingType(crate::macos_launcher::sharing_type());
    window.center();

    let delegate = AboutDelegate::new(mtm);
    unsafe { panel::set_delegate(&window, &*delegate) };
    DELEGATE.with(|slot| *slot.borrow_mut() = Some(delegate));

    let content = NSView::initWithFrame(NSView::alloc(mtm), frame);
    window.setContentView(Some(&content));
    let body = panel::rounded_glass(mtm, &content, RADIUS).content;

    let mut y = HEIGHT - PAD - 28.0;
    let name = widgets::label(mtm, 20.0, &NSColor::labelColor());
    name.setStringValue(&NSString::from_str("Copycraft"));
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

/// Close the About window (Wipe, Quit).
pub fn close() {
    if let Some(window) = WINDOW.with(|slot| slot.borrow_mut().take()) {
        restore_level(&window);
        window.close();
    }
    DELEGATE.with(|slot| *slot.borrow_mut() = None);
}
