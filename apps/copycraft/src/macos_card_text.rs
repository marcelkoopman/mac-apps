#![cfg(target_os = "macos")]

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_app_kit::{NSColor, NSFont, NSTextView};
use mac_ui::objc2_foundation::{NSMutableAttributedString, NSRange, NSString};
use mac_ui::text::AttrText;

use crate::format::FormatKind;
use crate::highlight::{self, TokenKind};

/// `error_line`: a 1-based line of `body` to mark (copied JSON that stops parsing there).
pub(crate) fn paint(
    text: &NSTextView,
    body: &str,
    highlight: Option<FormatKind>,
    payload: bool,
    error_line: Option<usize>,
) {
    let attr = if payload {
        let attr = match highlight {
            Some(kind) => colored_capped(body, kind),
            None => plain(body, &editor_font(), &NSColor::labelColor()),
        };
        if let Some(range) = error_line.and_then(|line| crate::commands::line_range(body, line)) {
            let mark = NSColor::systemRedColor().colorWithAlphaComponent(0.22);
            // SAFETY: NSBackgroundColorAttributeName takes an NSColor value.
            unsafe {
                attr.addAttribute_value_range(
                    mac_ui::objc2_app_kit::NSBackgroundColorAttributeName,
                    &mark,
                    mac_ui::text::utf16_range(body, &range),
                );
            }
        }
        attr
    } else {
        plain(
            body,
            &NSFont::systemFontOfSize(13.0),
            &NSColor::secondaryLabelColor(),
        )
    };
    if let Some(storage) = unsafe { text.textStorage() } {
        storage.setAttributedString(&attr);
    } else {
        text.setString(&NSString::from_str(body));
    }
}

/// A side-by-side diff. Each row is A, [`crate::diff::COLUMN_SEP`], then B. A side that
/// starts with `+` is green, one that starts with `-` is red, and the rest stays gray.
pub(crate) fn paint_diff(text: &NSTextView, body: &str) {
    let font = editor_font();
    let mut display = String::new();
    let mut marks: Vec<(NSRange, bool)> = Vec::new();
    let mut location = 0usize;
    for line in body.split_inclusive('\n') {
        let newline = line.ends_with('\n');
        let line = line.trim_end_matches('\n');
        let (left, right) = line
            .split_once(crate::diff::COLUMN_SEP)
            .unwrap_or((line, ""));
        push_side(&mut display, &mut marks, &mut location, left);
        push_plain(&mut display, &mut location, " │ ");
        push_side(&mut display, &mut marks, &mut location, right);
        if newline {
            push_plain(&mut display, &mut location, "\n");
        }
    }
    let attr = AttrText::new(&display, &font, &NSColor::secondaryLabelColor());
    let green = NSColor::systemGreenColor();
    let red = NSColor::systemRedColor();
    for (range, added) in &marks {
        attr.color_utf16(*range, if *added { &green } else { &red });
    }
    let attr = attr.into_attributed();
    let green_bg = green.colorWithAlphaComponent(0.16);
    let red_bg = red.colorWithAlphaComponent(0.16);
    for (range, added) in &marks {
        let bg: &NSColor = if *added { &green_bg } else { &red_bg };
        // SAFETY: NSBackgroundColorAttributeName takes an NSColor value.
        unsafe {
            attr.addAttribute_value_range(
                mac_ui::objc2_app_kit::NSBackgroundColorAttributeName,
                bg,
                *range,
            );
        }
    }
    if let Some(storage) = unsafe { text.textStorage() } {
        storage.setAttributedString(&attr);
    } else {
        text.setString(&NSString::from_str(&display));
    }
}

fn push_plain(display: &mut String, location: &mut usize, text: &str) {
    display.push_str(text);
    *location += text.encode_utf16().count();
}

/// Color a column when it starts with `+` or `-`. `added` is true for `+`.
fn push_side(
    display: &mut String,
    marks: &mut Vec<(NSRange, bool)>,
    location: &mut usize,
    side: &str,
) {
    let start = *location;
    push_plain(display, location, side);
    let added = side.starts_with('+');
    if added || side.starts_with('-') {
        marks.push((
            NSRange {
                location: start,
                length: *location - start,
            },
            added,
        ));
    }
}

fn editor_font() -> Retained<NSFont> {
    const NAMES: [&str; 5] = [
        "Zed Mono",
        "Zed Mono Regular",
        "JetBrains Mono",
        "SF Mono",
        "Menlo",
    ];
    mac_ui::fonts::monospace(12.0, &NAMES)
}

fn plain(body: &str, font: &NSFont, color: &NSColor) -> Retained<NSMutableAttributedString> {
    AttrText::new(body, font, color).into_attributed()
}

fn colored(source: &str, kind: FormatKind) -> Retained<NSMutableAttributedString> {
    let font = editor_font();
    let mut display = String::new();
    let mut spans: Vec<(TokenKind, usize)> = Vec::new();
    for (token_kind, token) in highlight::tokens(source, kind) {
        if token.is_empty() {
            continue;
        }
        display.push_str(&token);
        spans.push((token_kind, token.encode_utf16().count()));
    }
    let attr = AttrText::new(&display, &font, &color_for(TokenKind::Text));
    let mut offset = 0usize;
    for (token_kind, len) in spans {
        attr.color_utf16(
            NSRange {
                location: offset,
                length: len,
            },
            &color_for(token_kind),
        );
        offset += len;
    }
    attr.into_attributed()
}

/// Syntax colors cost an attribute run per token, so a "Show all" card colors its first
/// [`HIGHLIGHT_CAP`] bytes (up to a line end) and shows the rest in the plain editor style.
const HIGHLIGHT_CAP: usize = 80_000;

fn colored_capped(body: &str, kind: FormatKind) -> Retained<NSMutableAttributedString> {
    if body.len() <= HIGHLIGHT_CAP {
        return colored(body, kind);
    }
    let split = highlight_split(body);
    let attr = colored(&body[..split], kind);
    attr.appendAttributedString(&plain(
        &body[split..],
        &editor_font(),
        &NSColor::labelColor(),
    ));
    attr
}

/// Byte index after the last line end within the first [`HIGHLIGHT_CAP`] bytes (or at the cap,
/// on a char boundary, when that stretch has no line end).
fn highlight_split(body: &str) -> usize {
    if body.len() <= HIGHLIGHT_CAP {
        return body.len();
    }
    let mut cap = HIGHLIGHT_CAP;
    while !body.is_char_boundary(cap) {
        cap -= 1;
    }
    body[..cap].rfind('\n').map_or(cap, |index| index + 1)
}

fn color_for(kind: TokenKind) -> Retained<NSColor> {
    match kind {
        TokenKind::Key | TokenKind::Function => NSColor::systemBlueColor(),
        TokenKind::String => NSColor::systemGreenColor(),
        TokenKind::Number => NSColor::systemOrangeColor(),
        TokenKind::Keyword => NSColor::systemPurpleColor(),
        TokenKind::Type => NSColor::systemTealColor(),
        TokenKind::Macro => NSColor::systemBlueColor(),
        TokenKind::Comment => NSColor::secondaryLabelColor(),
        TokenKind::Punct => NSColor::tertiaryLabelColor(),
        TokenKind::Text => NSColor::labelColor(),
    }
}

/// Fill `frozen` with the first column of the grid in `text` (`lines`, from
/// [`crate::dataframe::frozen_column`]): the same characters and attributes, the same insets,
/// as tall as `text` from its top, so each line sits where it sits in the grid.
pub(crate) fn copy_frozen_column(
    text: &NSTextView,
    frozen: &NSTextView,
    lines: &[crate::dataframe::FrozenLine],
) {
    // SAFETY (all three): used right away, while the text views keep them alive.
    let Some(storage) = (unsafe { text.textStorage() }) else {
        return;
    };
    let length = storage.length();
    let column = NSMutableAttributedString::new();
    let take = |location: usize, end: usize| {
        if end > location && end <= length {
            let part = storage.attributedSubstringFromRange(NSRange {
                location,
                length: end - location,
            });
            column.appendAttributedString(&part);
        }
    };
    for line in lines {
        take(line.start, line.end);
        if let Some(newline) = line.newline {
            take(newline, newline + 1);
        }
    }
    frozen.setTextContainerInset(text.textContainerInset());
    if let (Some(from), Some(to)) = (unsafe { text.textContainer() }, unsafe {
        frozen.textContainer()
    }) {
        to.setLineFragmentPadding(from.lineFragmentPadding());
    }
    if let Some(frozen_storage) = unsafe { frozen.textStorage() } {
        frozen_storage.setAttributedString(&column);
    }
    mac_ui::widgets::fit_text_view(frozen, None);
    let size = frozen.frame().size;
    let inset = text.textContainerInset();
    // fit_text_view adds left+right inset; the pin only needs the left inset so its right edge
    // lines up with the first column separator — the extra right inset was covering column 2.
    let pin_w =
        pin_width_from_grid(text, lines).unwrap_or_else(|| (size.width - inset.width).max(1.0));
    frozen.setFrame(mac_ui::objc2_foundation::NSRect::new(
        mac_ui::objc2_foundation::NSPoint::new(0.0, 0.0),
        mac_ui::objc2_foundation::NSSize::new(pin_w, text.frame().size.height.max(size.height)),
    ));
}

/// Right edge of the first column in `text` (view coords), matching the floating pin's width.
fn pin_width_from_grid(text: &NSTextView, lines: &[crate::dataframe::FrozenLine]) -> Option<f64> {
    // SAFETY: layout objects are used immediately while the text view keeps them.
    let layout = unsafe { text.layoutManager() }?;
    let container = unsafe { text.textContainer() }?;
    let inset = text.textContainerInset();
    layout.ensureLayoutForTextContainer(&container);
    let mut max_x = 0.0_f64;
    for line in lines {
        if line.end <= line.start {
            continue;
        }
        let char_range = NSRange {
            location: line.start,
            length: line.end - line.start,
        };
        // SAFETY: actualCharacterRange null is allowed; layout/container live for this call.
        let glyphs = unsafe {
            layout
                .glyphRangeForCharacterRange_actualCharacterRange(char_range, std::ptr::null_mut())
        };
        if glyphs.length == 0 {
            continue;
        }
        let rect = layout.boundingRectForGlyphRange_inTextContainer(glyphs, &container);
        max_x = max_x.max(rect.origin.x + rect.size.width);
    }
    if max_x <= 0.0 {
        return None;
    }
    // Container coords → view coords (left inset).
    Some((max_x + inset.width).max(1.0))
}

#[cfg(test)]
mod tests {
    use super::{HIGHLIGHT_CAP, highlight_split};

    #[test]
    fn short_text_is_not_split() {
        assert_eq!(highlight_split("a\nb"), 3);
    }

    #[test]
    fn long_text_splits_after_a_line_end_within_the_cap() {
        let line = "x".repeat(99) + "\n";
        let body = line.repeat(HIGHLIGHT_CAP / 100 + 10);
        let split = highlight_split(&body);
        assert!(split <= HIGHLIGHT_CAP);
        assert_eq!(&body[split - 1..split], "\n");
    }

    #[test]
    fn a_cap_inside_a_character_moves_back_to_its_start() {
        let body = "é".repeat(HIGHLIGHT_CAP);
        let split = highlight_split(&body);
        assert!(body.is_char_boundary(split));
        assert!(split <= HIGHLIGHT_CAP);
    }
}
