//! One screenshot through `SCScreenshotManager`, as PNG bytes.

use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyClass;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSDictionary, NSError, NSPoint, NSRect, NSSize};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
    SCWindow,
};
use zeroize::Zeroizing;

use crate::shot_area::{LeaveOut, Rect, pixels};

/// What to capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// `rect`: points relative to the top left of the display `display` (a
    /// `CGDirectDisplayID`), see [`crate::shot_area::source_rect`].
    Area { display: u32, rect: Rect },
    /// The window with this `CGWindowID`, on its own.
    Window { id: u32 },
    /// All of the display `display`.
    Screen { display: u32 },
}

/// Why there is no picture. A kind only: nothing about the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failed {
    /// ScreenCaptureKit is missing.
    Unavailable,
    /// macOS refused (Screen Recording is off for this app).
    NotAllowed,
    /// The display or window was gone, or no picture came back.
    NoPicture,
}

/// `SCStreamErrorUserDeclined`: the user did not allow the capture.
const USER_DECLINED: isize = -3801;

type Done = Arc<Mutex<Option<Box<dyn FnOnce(Result<Zeroizing<Vec<u8>>, Failed>) + Send>>>>;

fn finish(done: &Done, result: Result<Zeroizing<Vec<u8>>, Failed>) {
    let f = done.lock().ok().and_then(|mut slot| slot.take());
    if let Some(f) = f {
        f(result);
    }
}

fn failure(code: Option<isize>) -> Failed {
    if code == Some(USER_DECLINED) {
        Failed::NotAllowed
    } else {
        Failed::NoPicture
    }
}

/// Capture `target`, leaving out the windows `leave_out` names (an app's own, unless the user
/// includes them; some always), and always the overlay and the thumbnail, without the pointer.
/// A window target that is left out is refused. `done` gets the PNG bytes, on a system queue.
/// Call on the main thread.
pub fn capture(
    target: Target,
    leave_out: LeaveOut,
    done: impl FnOnce(Result<Zeroizing<Vec<u8>>, Failed>) + Send + 'static,
) {
    let done: Done = Arc::new(Mutex::new(Some(Box::new(done))));
    let leave_out = leave_out
        .with(super::select::overlay_windows())
        .with(super::thumbnail::thumbnail_windows());
    // Looked up first: the typed classes below need ScreenCaptureKit's screenshot API (macOS 14).
    if AnyClass::get(c"SCShareableContent").is_none() {
        finish(&done, Err(Failed::Unavailable));
        return;
    }
    let on_content = done.clone();
    let handler = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: `content` and `error` are null or objects the system keeps alive for the
            // duration of this call. Only ScreenCaptureKit's typed getters are called on them;
            // the windows and displays they return are retained, so `Shareable` outlives the call.
            let (shareable, code) = unsafe {
                let code = error.as_ref().map(|e| e.code());
                let shareable = content.as_ref().map(|content| Shareable {
                    windows: content
                        .windows()
                        .iter()
                        .map(|window| {
                            let id = window.windowID();
                            let owner = window.owningApplication().map(|app| app.processID());
                            (window, id, owner)
                        })
                        .collect(),
                    displays: content
                        .displays()
                        .iter()
                        .map(|display| (display.displayID(), display))
                        .collect(),
                });
                (shareable, code)
            };
            match shareable.and_then(|shareable| prepare(&shareable, target, &leave_out)) {
                Some((filter, config)) => shoot(&filter, &config, on_content.clone()),
                None => finish(&on_content, Err(failure(code))),
            }
        },
    );
    // SAFETY: the handler is a block of `(SCShareableContent *, NSError *)`; the system copies it.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            false, true, &handler,
        );
    }
}

/// The shareable windows (with their `CGWindowID` and owning process id) and displays (with
/// their `CGDirectDisplayID`), read once from `SCShareableContent`.
struct Shareable {
    windows: Vec<(Retained<SCWindow>, u32, Option<i32>)>,
    displays: Vec<(u32, Retained<SCDisplay>)>,
}

/// What the filter shows: one window on its own, or a display without some windows.
enum Chosen<'a> {
    Window(&'a SCWindow),
    Display(&'a SCDisplay, Retained<NSArray<SCWindow>>),
}

/// The content filter and stream configuration for `target`.
fn prepare(
    shareable: &Shareable,
    target: Target,
    leave_out: &LeaveOut,
) -> Option<(Retained<SCContentFilter>, Retained<SCStreamConfiguration>)> {
    let (chosen, source) = match target {
        Target::Window { id } => {
            let (window, _, owner) = shareable.windows.iter().find(|(_, wid, _)| *wid == id)?;
            if leave_out.leaves_out(*owner, id) {
                return None;
            }
            (Chosen::Window(window), None)
        }
        Target::Area { display, .. } | Target::Screen { display } => {
            let (_, screen) = shareable.displays.iter().find(|(did, _)| *did == display)?;
            let excluded: Vec<Retained<SCWindow>> = shareable
                .windows
                .iter()
                .filter(|(_, wid, owner)| leave_out.leaves_out(*owner, *wid))
                .map(|(window, _, _)| window.clone())
                .collect();
            let source = match target {
                Target::Area { rect, .. } => Some(rect),
                _ => None,
            };
            (
                Chosen::Display(screen, NSArray::from_retained_slice(&excluded)),
                source,
            )
        }
    };
    // SAFETY: one of SCContentFilter's initialisers on a fresh allocation, with a window or a
    // display and windows from the shareable content; then plain getters on that filter (a float
    // scale and a CGRect).
    let (filter, scale, whole) = unsafe {
        let filter = match &chosen {
            Chosen::Window(window) => {
                SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window)
            }
            Chosen::Display(screen, excluded) => SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                screen,
                excluded,
            ),
        };
        let (scale, whole) = (filter.pointPixelScale(), filter.contentRect());
        (filter, scale, whole)
    };
    let scale = if scale > 0.0 { f64::from(scale) } else { 1.0 };
    let source = source.map(|rect| {
        NSRect::new(
            NSPoint::new(rect.x, rect.y),
            NSSize::new(rect.width, rect.height),
        )
    });
    let size = source.map_or(whole.size, |rect| rect.size);
    let (width, height) = (pixels(size.width, scale), pixels(size.height, scale));
    // SAFETY: a fresh configuration and its setters: a source rect in points, pixel sizes and a
    // BOOL.
    let config = unsafe {
        let config = SCStreamConfiguration::new();
        if let Some(rect) = source {
            config.setSourceRect(rect);
        }
        config.setWidth(width);
        config.setHeight(height);
        config.setShowsCursor(false);
        config
    };
    Some((filter, config))
}

/// Take the picture and hand its PNG bytes to `done`.
fn shoot(filter: &SCContentFilter, config: &SCStreamConfiguration, done: Done) {
    if AnyClass::get(c"SCScreenshotManager").is_none() {
        finish(&done, Err(Failed::Unavailable));
        return;
    }
    let handler = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        // The bitmap and the PNG AppKit makes cannot be wiped: in a pool of their own, so they
        // are freed as soon as the bytes are copied out (into the one buffer that is wiped and
        // moved on from here), not when the system's queue drains its pool.
        let result = autoreleasepool(|_| {
            // SAFETY: `image` and `error` are null or kept alive by the system for this call;
            // the image is encoded to PNG before it returns. An empty dictionary is a valid
            // property list for any file type.
            let (png, code) = unsafe {
                let png = image.as_ref().and_then(|image| {
                    NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), image)
                        .representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                });
                (png, error.as_ref().map(|e| e.code()))
            };
            png.map(|data| Zeroizing::new(data.to_vec()))
                .ok_or_else(|| failure(code))
        });
        finish(&done, result);
    });
    // SAFETY: a filter and configuration built from the shareable content, and a block of `(CGImageRef, NSError *)`,
    // which the system copies.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            filter,
            config,
            Some(&handler),
        );
    }
}
