//! The standard pasteboard type names, read without `unsafe` at the call site.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{ClassType, DowncastTarget};
use objc2_app_kit::{
    NSPasteboard, NSPasteboardReading, NSPasteboardType, NSPasteboardTypeFileURL,
    NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF,
    NSPasteboardURLReadingFileURLsOnlyKey,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

/// The AppKit pasteboard constants used here, read in one place.
struct Names {
    string: &'static NSPasteboardType,
    png: &'static NSPasteboardType,
    tiff: &'static NSPasteboardType,
    file_url: &'static NSPasteboardType,
    file_urls_only: &'static NSString,
}

fn names() -> Names {
    // SAFETY: immutable constants AppKit exports for the life of the process.
    unsafe {
        Names {
            string: NSPasteboardTypeString,
            png: NSPasteboardTypePNG,
            tiff: NSPasteboardTypeTIFF,
            file_url: NSPasteboardTypeFileURL,
            file_urls_only: NSPasteboardURLReadingFileURLsOnlyKey,
        }
    }
}

/// `NSPasteboardTypeString`: plain text.
pub fn string() -> &'static NSPasteboardType {
    names().string
}

/// `NSPasteboardTypePNG`: PNG image data.
pub fn png() -> &'static NSPasteboardType {
    names().png
}

/// `NSPasteboardTypeTIFF`: TIFF image data.
pub fn tiff() -> &'static NSPasteboardType {
    names().tiff
}

/// `NSPasteboardTypeFileURL`: a file URL.
pub fn file_url() -> &'static NSPasteboardType {
    names().file_url
}

/// The objects of class `T` (an `NSPasteboardReading` class such as `NSURL`, `NSImage` or
/// `NSFilePromiseReceiver`) that `pasteboard` holds, in its order; with `file_urls_only`, only
/// file URLs (`NSPasteboardURLReadingFileURLsOnlyKey`). Empty when there are none.
pub fn read_objects<T>(pasteboard: &NSPasteboard, file_urls_only: bool) -> Vec<Retained<T>>
where
    T: ClassType + DowncastTarget + NSPasteboardReading,
{
    let classes: Retained<NSArray<AnyClass>> = NSArray::from_slice(&[T::class()]);
    let options = file_urls_only.then(|| {
        let yes = NSNumber::numberWithBool(true);
        let yes: &AnyObject = &yes;
        NSDictionary::from_slices(&[names().file_urls_only], &[yes])
    });
    // SAFETY: `T` conforms to NSPasteboardReading, so it can read from the pasteboard; the only
    // option, NSPasteboardURLReadingFileURLsOnlyKey, takes an NSNumber boolean.
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&classes, options.as_deref()) };
    objects
        .map(|objects| {
            objects
                .iter()
                .filter_map(|object| object.downcast::<T>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Run `f` with access to a security-scoped file URL, such as one Finder put on the pasteboard.
///
/// Access is released again after `f` returns. Without a scope (a plain path) `f` still runs.
pub fn with_security_scope<R>(url: &objc2_foundation::NSURL, f: impl FnOnce() -> R) -> R {
    // SAFETY: balanced by the stop call below whenever it returns true.
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    let result = f();
    if started {
        // SAFETY: balances the successful start call above.
        unsafe { url.stopAccessingSecurityScopedResource() };
    }
    result
}
