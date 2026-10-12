//! Font lookup helpers.

use objc2::rc::Retained;
use objc2_app_kit::{NSFont, NSFontWeight, NSFontWeightSemibold};
use objc2_foundation::NSString;

/// First installed font from `preferred` at `size`, else the system monospaced font
/// (regular weight).
pub fn monospace(size: f64, preferred: &[&str]) -> Retained<NSFont> {
    for name in preferred {
        if let Some(font) = NSFont::fontWithName_size(&NSString::from_str(name), size) {
            return font;
        }
    }
    NSFont::monospacedSystemFontOfSize_weight(size, 0.0)
}

/// The semibold font weight (`NSFontWeightSemibold`).
pub fn semibold_weight() -> NSFontWeight {
    // SAFETY: a constant AppKit exports; reading it has no side effects.
    unsafe { NSFontWeightSemibold }
}
