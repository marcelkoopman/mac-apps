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
