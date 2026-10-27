//! The selection overlay: a dimmed, borderless window over every screen. Drag out an area;
//! click without dragging for the whole screen; Space switches to picking a window (the one
//! under the pointer lights up, a click takes it) and back; Esc or a right click cancels.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplicationActivationOptions, NSBezierPath, NSColor, NSCursor, NSEvent, NSResponder,
    NSRunningApplication, NSScreen, NSScreenSaverWindowLevel, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowSharingType, NSWorkspace,
};
use objc2_core_graphics::{CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString};

use crate::shot_area::{self, LeaveOut, Rect};

/// What the user picked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick {
    /// An area: `rect` relative to the top left of display `display`, in points.
    Area { display: u32, rect: Rect },
    /// The window with this `CGWindowID`.
    Window { id: u32 },
    /// The whole display `display`.
    Screen { display: u32 },
}

const ESCAPE: u16 = crate::keys::code::ESCAPE;
const SPACE: u16 = 0x31;
/// Window layer 0: normal app windows (no menu bar, Dock or desktop).
const NORMAL_LAYER: i64 = 0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Area,
    Window,
}

struct Screen {
    display: u32,
    /// The screen's frame (AppKit coordinates).
    frame: Rect,
    window: Retained<ShotWindow>,
    view: Retained<ShotView>,
}

struct Session {
    screens: Vec<Screen>,
    mode: Mode,
    start: Option<(f64, f64)>,
    now: Option<(f64, f64)>,
    /// Windows that can be picked, front to back, AppKit frames.
    targets: Vec<(u32, Rect)>,
    hover: Option<(u32, Rect)>,
    /// The app that was in front before the overlay (not this one): in front again when the
    /// overlay closes, so ⌘V works there at once.
    previous: Option<Retained<NSRunningApplication>>,
    done: Box<dyn FnOnce(Option<Pick>)>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    /// The overlay last closed, kept until the next one closes: it is closed from inside its
    /// own view's methods, which must not free the view under them.
    static RETIRED: RefCell<Vec<Screen>> = const { RefCell::new(Vec::new()) };
    /// The pick of the overlay just ordered out, handed over a few frames later
    /// ([`shot_area::AFTER_OVERLAY_SECS`]) so the capture cannot catch the overlay.
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

type Pending = (Box<dyn FnOnce(Option<Pick>)>, Option<Pick>);

define_class!(
    // SAFETY: NSWindow has no subclassing requirements; the overrides keep its signatures and
    // there are no ivars.
    #[unsafe(super(NSWindow, NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacUiShotWindow"]
    struct ShotWindow;

    impl ShotWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        #[unsafe(method(cancelOperation:))]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            finish(None);
        }
    }
);

define_class!(
    // SAFETY: NSView has no subclassing requirements; the overrides keep its signatures and
    // there are no ivars.
    #[unsafe(super(NSView, NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacUiShotView"]
    struct ShotView;

    impl ShotView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            match event.keyCode() {
                ESCAPE => finish(None),
                SPACE => toggle_mode(),
                _ => {}
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            pointer_down();
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            pointer_moved(true);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, _event: &NSEvent) {
            pointer_moved(false);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            pointer_up();
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            finish(None);
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            draw(self);
        }
    }
);

impl ShotView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's designated initialiser on a fresh allocation.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

fn point() -> (f64, f64) {
    let at = NSEvent::mouseLocation();
    (at.x, at.y)
}

fn ns_rect(rect: &Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, rect.y),
        NSSize::new(rect.width, rect.height),
    )
}

fn rect_of(rect: NSRect) -> Rect {
    Rect::new(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}

/// The `CGDirectDisplayID` of `screen`.
fn display_id(screen: &NSScreen) -> Option<u32> {
    let number = screen
        .deviceDescription()
        .objectForKey(&NSString::from_str("NSScreenNumber"))?;
    Some(number.downcast_ref::<NSNumber>()?.unsignedIntValue())
}

fn number(dict: &NSDictionary<AnyObject, AnyObject>, key: &str) -> Option<f64> {
    let key = NSString::from_str(key);
    let value = dict.objectForKey(&key)?;
    Some(value.downcast_ref::<NSNumber>()?.doubleValue())
}

/// The normal windows on screen, front to back, by id and AppKit frame, without those
/// `leave_out` names. Only ids, owners and bounds are read, never names.
fn on_screen_windows(main_height: f64, leave_out: &LeaveOut) -> Vec<(u32, Rect)> {
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    ) else {
        return Vec::new();
    };
    // A CFArray is toll-free bridged to NSArray; `downcast_ref` checks the class.
    let list: &objc2_core_foundation::CFType = list.as_ref();
    let list: &AnyObject = list.as_ref();
    let Some(list) = list.downcast_ref::<NSArray>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let bounds_key = NSString::from_str("kCGWindowBounds");
    for entry in list.iter() {
        let Some(info) = entry.downcast_ref::<NSDictionary>() else {
            continue;
        };
        let layer = number(info, "kCGWindowLayer").map(|layer| layer as i64);
        let owner = number(info, "kCGWindowOwnerPID").map(|pid| pid as i32);
        if layer != Some(NORMAL_LAYER) {
            continue;
        }
        let (Some(id), Some(bounds)) = (
            number(info, "kCGWindowNumber"),
            info.objectForKey(&bounds_key),
        ) else {
            continue;
        };
        if leave_out.leaves_out(owner, id as u32) {
            continue;
        }
        let Some(bounds) = bounds.downcast_ref::<NSDictionary>() else {
            continue;
        };
        let (Some(x), Some(y), Some(width), Some(height)) = (
            number(bounds, "X"),
            number(bounds, "Y"),
            number(bounds, "Width"),
            number(bounds, "Height"),
        ) else {
            continue;
        };
        let frame = shot_area::from_core_graphics(&Rect::new(x, y, width, height), main_height);
        out.push((id as u32, frame));
    }
    out
}

/// Show the overlay on every screen and hand what the user picks to `done` (`None`: cancelled),
/// a few frames after the overlay is gone. The windows `leave_out` names cannot be picked. A
/// selection already showing is cancelled first.
pub fn select(
    mtm: MainThreadMarker,
    leave_out: &LeaveOut,
    done: impl FnOnce(Option<Pick>) + 'static,
) {
    cancel_selection();
    let screens = NSScreen::screens(mtm);
    let main_height = screens
        .iter()
        .next()
        .map_or(0.0, |main| main.frame().size.height);
    // Read before the overlay exists, so it is never one of them.
    let targets = on_screen_windows(main_height, leave_out);
    // A pick of the previous overlay not handed over yet goes first.
    deliver();
    let mut shown = Vec::new();
    for screen in screens.iter() {
        let Some(display) = display_id(&screen) else {
            continue;
        };
        let frame = screen.frame();
        let window: Retained<ShotWindow> =
            crate::panel::borderless(ShotWindow::alloc(mtm), frame.size);
        crate::panel::keep_when_closed(&window);
        window.setFrame_display(frame, false);
        window.setOpaque(false);
        window.setHasShadow(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setLevel(NSScreenSaverWindowLevel);
        // Never in a capture or a screen share itself.
        window.setSharingType(NSWindowSharingType::None);
        window.setAcceptsMouseMovedEvents(true);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let view = ShotView::new(mtm, NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
        window.setContentView(Some(&view));
        shown.push(Screen {
            display,
            frame: rect_of(frame),
            window,
            view,
        });
    }
    if shown.is_empty() {
        done(None);
        return;
    }
    let own = super::own_pid();
    let previous = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .filter(|app| app.processIdentifier() != own);
    crate::panel::activate_app(mtm);
    let (x, y) = point();
    for screen in &shown {
        screen.window.orderFrontRegardless();
        if screen.frame.contains(x, y) {
            screen.window.makeKeyWindow();
            let view: &NSResponder = screen.view.as_ref();
            screen.window.makeFirstResponder(Some(view));
        }
    }
    NSCursor::crosshairCursor().set();
    SESSION.with(|slot| {
        *slot.borrow_mut() = Some(Session {
            screens: shown,
            mode: Mode::Area,
            start: None,
            now: None,
            targets,
            hover: None,
            previous,
            done: Box::new(done),
        })
    });
}

/// Close the overlay without a pick (Wipe, the lock screen); a pick still on its way becomes
/// none too. Nothing when none is showing.
pub fn cancel_selection() {
    finish(None);
    PENDING.with(|slot| {
        if let Some((_, pick)) = slot.borrow_mut().as_mut() {
            *pick = None;
        }
    });
}

/// Close the overlay without a pick, as if cancelled, when the app is no longer active or the
/// screens changed (it would hang over them otherwise). A pick on its way is kept.
pub fn dismiss_selection() {
    finish(None);
}

/// The window numbers of the overlay last shown (ordered out, kept until the next one closes),
/// for a capture to leave out.
pub(super) fn overlay_windows() -> Vec<u32> {
    RETIRED.with(|slot| {
        slot.borrow()
            .iter()
            .filter_map(|screen| u32::try_from(screen.window.windowNumber()).ok())
            .collect()
    })
}

fn redraw(session: &Session) {
    for screen in &session.screens {
        screen.view.setNeedsDisplay(true);
    }
}

fn toggle_mode() {
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().as_mut() {
            session.mode = match session.mode {
                Mode::Area => Mode::Window,
                Mode::Window => Mode::Area,
            };
            session.start = None;
            session.now = None;
            let (x, y) = point();
            session.hover = shot_area::window_at(&session.targets, x, y);
            redraw(session);
        }
    });
}

fn pointer_down() {
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().as_mut() {
            let at = point();
            session.start = Some(at);
            session.now = Some(at);
            redraw(session);
        }
    });
}

fn pointer_moved(dragging: bool) {
    SESSION.with(|slot| {
        if let Some(session) = slot.borrow_mut().as_mut() {
            NSCursor::crosshairCursor().set();
            let (x, y) = point();
            match session.mode {
                Mode::Area if dragging => session.now = Some((x, y)),
                Mode::Area => {}
                Mode::Window => session.hover = shot_area::window_at(&session.targets, x, y),
            }
            redraw(session);
        }
    });
}

fn pointer_up() {
    let pick = SESSION.with(|slot| {
        let slot = slot.borrow();
        let session = slot.as_ref()?;
        let end = point();
        match session.mode {
            Mode::Window => shot_area::window_at(&session.targets, end.0, end.1)
                .map(|(id, _)| Pick::Window { id }),
            Mode::Area => {
                let start = session.start?;
                let screen = session
                    .screens
                    .iter()
                    .find(|screen| screen.frame.contains(start.0, start.1))?;
                if shot_area::is_click(start, end) {
                    return Some(Pick::Screen {
                        display: screen.display,
                    });
                }
                let rect = shot_area::source_rect(&shot_area::dragged(start, end), &screen.frame)?;
                Some(Pick::Area {
                    display: screen.display,
                    rect,
                })
            }
        }
    });
    // A click in window mode beside every window does nothing; anything else ends here.
    let window_mode = SESSION.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|session| session.mode == Mode::Window)
    });
    if pick.is_some() || !window_mode {
        finish(pick);
    }
}

/// Close the overlay and hand `pick` to the caller.
fn finish(pick: Option<Pick>) {
    let Some(session) = SESSION.with(|slot| slot.borrow_mut().take()) else {
        return;
    };
    for screen in &session.screens {
        screen.window.orderOut(None);
    }
    NSCursor::arrowCursor().set();
    let Session {
        screens,
        done,
        previous,
        ..
    } = session;
    RETIRED.with(|slot| *slot.borrow_mut() = screens);
    // Back to the app the user was in, also on a cancel.
    if let Some(app) = previous {
        let _ = app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
    // A cancel needs no wait; a pick waits until the window server drew without the overlay.
    if pick.is_none() {
        done(None);
        return;
    }
    PENDING.with(|slot| *slot.borrow_mut() = Some((done, pick)));
    let wait = std::time::Duration::from_secs_f64(shot_area::AFTER_OVERLAY_SECS);
    let spawned = std::thread::Builder::new()
        .name("shot-overlay-gone".into())
        .spawn(move || {
            std::thread::sleep(wait);
            crate::actions::run_on_main(deliver);
        });
    if spawned.is_err() {
        deliver();
    }
}

/// Hand the pending pick to its caller.
fn deliver() {
    if let Some((done, pick)) = PENDING.with(|slot| slot.borrow_mut().take()) {
        done(pick);
    }
}

/// Dim the screen, leave the selection (or the window under the pointer) clear and outline it.
fn draw(view: &ShotView) {
    SESSION.with(|slot| {
        let slot = slot.borrow();
        let Some(session) = slot.as_ref() else {
            return;
        };
        let Some(screen) = session
            .screens
            .iter()
            .find(|screen| std::ptr::eq(&*screen.view, view))
        else {
            return;
        };
        let bounds = rect_of(view.bounds());
        let clear = match session.mode {
            Mode::Area => session
                .start
                .zip(session.now)
                .filter(|(start, now)| !shot_area::is_click(*start, *now))
                .map(|(start, now)| shot_area::dragged(start, now)),
            Mode::Window => session.hover.map(|(_, frame)| frame),
        }
        .and_then(|global| global.intersection(&screen.frame))
        .map(|global| {
            Rect::new(
                global.x - screen.frame.x,
                global.y - screen.frame.y,
                global.width,
                global.height,
            )
        });
        NSColor::colorWithWhite_alpha(0.0, 0.3).setFill();
        match clear {
            None => NSBezierPath::fillRect(ns_rect(&bounds)),
            Some(hole) => {
                for part in around(&bounds, &hole) {
                    NSBezierPath::fillRect(ns_rect(&part));
                }
                if session.mode == Mode::Window {
                    NSColor::systemBlueColor()
                        .colorWithAlphaComponent(0.2)
                        .setFill();
                    NSBezierPath::fillRect(ns_rect(&hole));
                }
                NSColor::whiteColor().setStroke();
                NSBezierPath::strokeRect(ns_rect(&hole));
            }
        }
    });
}

/// The four bands of `bounds` around `hole` (inside it): below, above, left and right.
fn around(bounds: &Rect, hole: &Rect) -> [Rect; 4] {
    let top = hole.y + hole.height;
    [
        Rect::new(bounds.x, bounds.y, bounds.width, hole.y - bounds.y),
        Rect::new(bounds.x, top, bounds.width, bounds.y + bounds.height - top),
        Rect::new(bounds.x, hole.y, hole.x - bounds.x, hole.height),
        Rect::new(
            hole.x + hole.width,
            hole.y,
            bounds.x + bounds.width - hole.x - hole.width,
            hole.height,
        ),
    ]
}
