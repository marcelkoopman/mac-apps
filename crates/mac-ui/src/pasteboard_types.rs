//! The standard pasteboard type names, read without `unsafe` at the call site.

use objc2_app_kit::{
    NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF,
};

/// `NSPasteboardTypeString`: plain text.
pub fn string() -> &'static NSPasteboardType {
    // SAFETY: an immutable constant AppKit exports for the life of the process.
    unsafe { NSPasteboardTypeString }
}

/// `NSPasteboardTypePNG`: PNG image data.
pub fn png() -> &'static NSPasteboardType {
    // SAFETY: an immutable constant AppKit exports for the life of the process.
    unsafe { NSPasteboardTypePNG }
}

/// `NSPasteboardTypeTIFF`: TIFF image data.
pub fn tiff() -> &'static NSPasteboardType {
    // SAFETY: an immutable constant AppKit exports for the life of the process.
    unsafe { NSPasteboardTypeTIFF }
}

/// `NSPasteboardTypeFileURL`: a file URL.
pub fn file_url() -> &'static NSPasteboardType {
    // SAFETY: an immutable constant AppKit exports for the life of the process.
    unsafe { NSPasteboardTypeFileURL }
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
