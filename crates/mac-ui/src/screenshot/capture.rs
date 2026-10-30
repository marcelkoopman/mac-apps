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
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
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
            // duration of this call; nothing is kept past it except the retained filter and
            // configuration. The ScreenCaptureKit calls are the typed bindings of documented
            // selectors (macOS 14 and later) on the live `content`; `owningApplication` is
            // nullable. SCContentFilter's initialisers get a fresh allocation; the configuration
            // takes pixel sizes, a source rect in points and a BOOL.
            let (prepared, code) = unsafe {
                let code = error.as_ref().map(|e| e.code());
                // The content filter and stream configuration for `target`.
                let prepared =
                    content.as_ref().and_then(
                        |content| -> Option<(
                            Retained<SCContentFilter>,
                            Retained<SCStreamConfiguration>,
                        )> {
                            let owner = |window: &SCWindow| {
                                window.owningApplication().map(|app| app.processID())
                            };
                            let windows = content.windows();
                            let (filter, source) = match target {
                                Target::Window { id } => {
                                    let window =
                                        windows.iter().find(|window| window.windowID() == id)?;
                                    if leave_out.leaves_out(owner(&window), id) {
                                        return None;
                                    }
                                    let filter = SCContentFilter::initWithDesktopIndependentWindow(
                                        SCContentFilter::alloc(),
                                        &window,
                                    );
                                    (filter, None)
                                }
                                Target::Area { display, .. } | Target::Screen { display } => {
                                    let screen = content
                                        .displays()
                                        .iter()
                                        .find(|candidate| candidate.displayID() == display)?;
                                    let excluded: Vec<Retained<SCWindow>> = windows
                                        .iter()
                                        .filter(|window| {
                                            leave_out.leaves_out(owner(window), window.windowID())
                                        })
                                        .collect();
                                    let excluded = NSArray::from_retained_slice(&excluded);
                                    let filter = SCContentFilter::initWithDisplay_excludingWindows(
                                        SCContentFilter::alloc(),
                                        &screen,
                                        &excluded,
                                    );
                                    let source = match target {
                                        Target::Area { rect, .. } => Some(rect),
                                        _ => None,
                                    };
                                    (filter, source)
                                }
                            };
                            let scale = filter.pointPixelScale();
                            let scale = if scale > 0.0 { f64::from(scale) } else { 1.0 };
                            let whole = filter.contentRect();
                            let config = SCStreamConfiguration::new();
                            let (width, height) = match source {
                                Some(rect) => {
                                    let rect = NSRect::new(
                                        NSPoint::new(rect.x, rect.y),
                                        NSSize::new(rect.width, rect.height),
                                    );
                                    config.setSourceRect(rect);
                                    (rect.size.width, rect.size.height)
                                }
                                None => (whole.size.width, whole.size.height),
                            };
                            config.setWidth(pixels(width, scale));
                            config.setHeight(pixels(height, scale));
                            config.setShowsCursor(false);
                            Some((filter, config))
                        },
                    );
                (prepared, code)
            };
            match prepared {
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
