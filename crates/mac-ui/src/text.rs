//! Attributed strings from Rust text: one base font and colour, then runs of font, colour or
//! background over byte ranges (or UTF-16 ranges) of the source text.
//!
//! Rust strings index bytes; `NSAttributedString` indexes UTF-16 code units. [`utf16_range`]
//! converts one to the other, and the typed setters keep each attribute paired with the value
//! type AppKit expects, so callers need no `unsafe`.

use std::ops::Range;

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackgroundColorAttributeName, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName,
};
use objc2_foundation::{NSMutableAttributedString, NSRange, NSString};

/// The UTF-16 range of the byte range `range` in `text`. `range` must lie on char boundaries.
pub fn utf16_range(text: &str, range: &Range<usize>) -> NSRange {
    NSRange {
        location: text[..range.start].encode_utf16().count(),
        length: text[range.start..range.end].encode_utf16().count(),
    }
}

/// An `NSMutableAttributedString` of `source` being built. It borrows `source` (no copy is kept
/// on the Rust side) to convert byte ranges.
pub struct AttrText<'a> {
    source: &'a str,
    attr: Retained<NSMutableAttributedString>,
    len: usize,
}

impl<'a> AttrText<'a> {
    /// `source` in `font` and `color`.
    pub fn new(source: &'a str, font: &NSFont, color: &NSColor) -> Self {
        let ns = NSString::from_str(source);
        let attr =
            NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
        let len = ns.length();
        let text = Self { source, attr, len };
        let all = text.all();
        text.font_utf16(all, font);
        text.color_utf16(all, color);
        text
    }

    /// The whole string as a UTF-16 range.
    pub fn all(&self) -> NSRange {
        NSRange {
            location: 0,
            length: self.len,
        }
    }

    /// Set the font of the byte range `range` of the source.
    pub fn font(&self, range: &Range<usize>, font: &NSFont) -> &Self {
        self.font_utf16(utf16_range(self.source, range), font)
    }

    /// Set the text colour of the byte range `range` of the source.
    pub fn color(&self, range: &Range<usize>, color: &NSColor) -> &Self {
        self.color_utf16(utf16_range(self.source, range), color)
    }

    /// Set the background colour of the byte range `range` of the source.
    pub fn background(&self, range: &Range<usize>, color: &NSColor) -> &Self {
        self.background_utf16(utf16_range(self.source, range), color)
    }

    /// [`font`](Self::font) for a range already in UTF-16 units, for callers that walk the text
    /// once and keep their own offsets.
    pub fn font_utf16(&self, range: NSRange, font: &NSFont) -> &Self {
        // SAFETY: NSFontAttributeName takes an NSFont value.
        unsafe {
            self.attr
                .addAttribute_value_range(NSFontAttributeName, font, range)
        };
        self
    }

    /// [`color`](Self::color) for a range already in UTF-16 units.
    pub fn color_utf16(&self, range: NSRange, color: &NSColor) -> &Self {
        // SAFETY: NSForegroundColorAttributeName takes an NSColor value.
        unsafe {
            self.attr
                .addAttribute_value_range(NSForegroundColorAttributeName, color, range)
        };
        self
    }

    /// [`background`](Self::background) for a range already in UTF-16 units.
    pub fn background_utf16(&self, range: NSRange, color: &NSColor) -> &Self {
        // SAFETY: NSBackgroundColorAttributeName takes an NSColor value.
        unsafe {
            self.attr
                .addAttribute_value_range(NSBackgroundColorAttributeName, color, range)
        };
        self
    }

    /// The built string.
    pub fn into_attributed(self) -> Retained<NSMutableAttributedString> {
        self.attr
    }
}

#[cfg(test)]
mod tests {
    use super::{AttrText, utf16_range};
    use objc2_app_kit::{NSColor, NSFont};

    #[test]
    fn byte_ranges_become_utf16_ranges() {
        let text = "é😀a";
        // é: 2 bytes, 1 unit; 😀: 4 bytes, 2 units; a: 1 byte, 1 unit.
        let r = utf16_range(text, &(0..2));
        assert_eq!((r.location, r.length), (0, 1));
        let r = utf16_range(text, &(2..6));
        assert_eq!((r.location, r.length), (1, 2));
        let r = utf16_range(text, &(6..7));
        assert_eq!((r.location, r.length), (3, 1));
        let r = utf16_range(text, &(7..7));
        assert_eq!((r.location, r.length), (4, 0));
    }

    #[test]
    fn builder_keeps_the_text_and_counts_utf16() {
        let source = "a😀b";
        let text = AttrText::new(
            source,
            &NSFont::systemFontOfSize(12.0),
            &NSColor::labelColor(),
        );
        assert_eq!(text.all().length, 4);
        text.color(&(1..5), &NSColor::systemRedColor())
            .background(&(5..6), &NSColor::systemYellowColor());
        let attr = text.into_attributed();
        assert_eq!(attr.string().to_string(), source);
    }
}
