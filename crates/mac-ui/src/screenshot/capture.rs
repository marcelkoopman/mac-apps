//! One screenshot through `SCScreenshotManager`, as PNG bytes.

use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{AnyThread, msg_send};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSDictionary, NSError, NSPoint, NSRect, NSSize};
use zeroize::Zeroizing;

use crate::shot_area::{Rect, pixels};

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

/// Capture `target`, leaving out the windows of process `leave_out` (CopyCraft's own, unless
/// the user includes them), without the pointer. `done` gets the PNG bytes, on a system queue.
pub fn capture(
    target: Target,
    leave_out: Option<i32>,
    done: impl FnOnce(Result<Zeroizing<Vec<u8>>, Failed>) + Send + 'static,
) {
    let done: Done = Arc::new(Mutex::new(Some(Box::new(done))));
    let Some(content_class) = AnyClass::get(c"SCShareableContent") else {
        finish(&done, Err(Failed::Unavailable));
        return;
    };
    let on_content = done.clone();
    let handler = RcBlock::new(move |content: *mut AnyObject, error: *mut NSError| {
        // SAFETY: `content` and `error` are null or objects the system keeps alive for the
        // duration of this call; nothing is kept past it except the retained filter and
        // configuration.
        let (content, code) = unsafe { (content.as_ref(), error.as_ref().map(|e| e.code())) };
        match content.and_then(|content| prepare(content, target, leave_out)) {
            Some((filter, config)) => shoot(&filter, &config, on_content.clone()),
            None => finish(&on_content, Err(failure(code))),
        }
    });
    // SAFETY: `+[SCShareableContent getShareableContentExcludingDesktopWindows:
    // onScreenWindowsOnly:completionHandler:]` takes two BOOLs and a block of
    // `(SCShareableContent *, NSError *)`, which `handler` is; the system copies the block.
    unsafe {
        let _: () = msg_send![
            content_class,
            getShareableContentExcludingDesktopWindows: false,
            onScreenWindowsOnly: true,
            completionHandler: &*handler
        ];
    }
}

/// The content filter and stream configuration for `target` from the shareable `content`.
fn prepare(
    content: &AnyObject,
    target: Target,
    leave_out: Option<i32>,
) -> Option<(Retained<AnyObject>, Retained<AnyObject>)> {
    let filter_class = AnyClass::get(c"SCContentFilter")?;
    let config_class = AnyClass::get(c"SCStreamConfiguration")?;
    // SAFETY: documented ScreenCaptureKit selectors (macOS 14 and later) with their argument and
    // return types: `displays`/`windows` are arrays of SCDisplay/SCWindow, `displayID` and
    // `windowID` are 32-bit ids, `owningApplication` is a nullable SCRunningApplication whose
    // `processID` is a pid_t. The filter is made with SCContentFilter's initialisers on a fresh
    // allocation; `pointPixelScale` is a float and `contentRect` a CGRect. The configuration
    // takes size_t sizes, a CGRect source rect and a BOOL.
    unsafe {
        let displays: Retained<NSArray<AnyObject>> = msg_send![content, displays];
        let windows: Retained<NSArray<AnyObject>> = msg_send![content, windows];
        let allocated: Allocated<AnyObject> = msg_send![filter_class, alloc];
        let (filter, source): (Retained<AnyObject>, Option<Rect>) = match target {
            Target::Window { id } => {
                let window = windows.iter().find(|window| {
                    let window_id: u32 = msg_send![&**window, windowID];
                    window_id == id
                })?;
                let filter: Retained<AnyObject> =
                    msg_send![allocated, initWithDesktopIndependentWindow: &*window];
                (filter, None)
            }
            Target::Area { display, .. } | Target::Screen { display } => {
                let screen = displays.iter().find(|candidate| {
                    let display_id: u32 = msg_send![&**candidate, displayID];
                    display_id == display
                })?;
                let excluded: Vec<Retained<AnyObject>> = windows
                    .iter()
                    .filter(|window| {
                        let app: Option<Retained<AnyObject>> =
                            msg_send![&**window, owningApplication];
                        app.is_some_and(|app| {
                            let pid: i32 = msg_send![&*app, processID];
                            Some(pid) == leave_out
                        })
                    })
                    .collect();
                let excluded = NSArray::from_retained_slice(&excluded);
                let filter: Retained<AnyObject> = msg_send![
                    allocated,
                    initWithDisplay: &*screen,
                    excludingWindows: &*excluded
                ];
                let source = match target {
                    Target::Area { rect, .. } => Some(rect),
                    _ => None,
                };
                (filter, source)
            }
        };
        let scale: f32 = msg_send![&*filter, pointPixelScale];
        let scale = if scale > 0.0 { f64::from(scale) } else { 1.0 };
        let whole: NSRect = msg_send![&*filter, contentRect];
        let config: Retained<AnyObject> = msg_send![config_class, new];
        let (width, height) = match source {
            Some(rect) => {
                let rect = NSRect::new(
                    NSPoint::new(rect.x, rect.y),
                    NSSize::new(rect.width, rect.height),
                );
                let _: () = msg_send![&*config, setSourceRect: rect];
                (rect.size.width, rect.size.height)
            }
            None => (whole.size.width, whole.size.height),
        };
        let _: () = msg_send![&*config, setWidth: pixels(width, scale)];
        let _: () = msg_send![&*config, setHeight: pixels(height, scale)];
        let _: () = msg_send![&*config, setShowsCursor: false];
        Some((filter, config))
    }
}

/// Take the picture and hand its PNG bytes to `done`.
fn shoot(filter: &AnyObject, config: &AnyObject, done: Done) {
    let Some(manager) = AnyClass::get(c"SCScreenshotManager") else {
        finish(&done, Err(Failed::Unavailable));
        return;
    };
    let handler = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        // SAFETY: `image` and `error` are null or kept alive by the system for this call; the
        // image is encoded to PNG before it returns. An empty dictionary is a valid property
        // list for any file type.
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
        let result = png
            .map(|data| Zeroizing::new(data.to_vec()))
            .ok_or_else(|| failure(code));
        finish(&done, result);
    });
    // SAFETY: `+[SCScreenshotManager captureImageWithFilter:configuration:completionHandler:]`
    // takes an SCContentFilter, an SCStreamConfiguration and a block of `(CGImageRef, NSError *)`,
    // which `handler` is; the system copies the block.
    unsafe {
        let _: () = msg_send![
            manager,
            captureImageWithFilter: filter,
            configuration: config,
            completionHandler: &*handler
        ];
    }
}
