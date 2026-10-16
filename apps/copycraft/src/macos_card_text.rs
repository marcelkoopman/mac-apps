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
    numbers: bool,
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
        if numbers {
            insert_gutters(&attr, body);
        }
        attr
    } else {
        plain(
            body,
            &NSFont::systemFontOfSize(zoomed(mac_ui::metrics::FONT_BODY)),
            &crate::appearance::caption_color(),
        )
    };
    if let Some(storage) = mac_ui::widgets::text_storage(text) {
        storage.setAttributedString(&attr);
    } else {
        text.setString(&NSString::from_str(body));
    }
}

/// A line number in front of every line of `attr` (the painted `body`), in a dim color. The
/// colors of the text stay as they are; offsets of `body` move by [`crate::commands::gutter_range`].
fn insert_gutters(attr: &NSMutableAttributedString, body: &str) {
    let starts = crate::commands::numbered_line_starts(body);
    let count = starts.len();
    let font = editor_font();
    let color = NSColor::tertiaryLabelColor();
    // From the end, so the earlier offsets stay valid.
    for (index, start) in starts.iter().enumerate().rev() {
        let label = crate::commands::gutter_label(index + 1, count);
        let gutter = mac_ui::text::AttrText::new(&label, &font, &color).into_attributed();
        attr.insertAttributedString_atIndex(&gutter, *start);
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
    if let Some(storage) = mac_ui::widgets::text_storage(text) {
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

thread_local! {
    /// The zoom (percent) of the text being painted, and the one asked for in this session.
    static FONT_ZOOM: std::cell::Cell<u32> = const { std::cell::Cell::new(100) };
    static FONT_ZOOM_SETTING: std::cell::Cell<u32> = const { std::cell::Cell::new(100) };
}

/// Paint at this zoom (percent); 100 for everything that must keep its metrics (tables).
pub(crate) fn set_font_zoom(zoom: u32) {
    FONT_ZOOM.with(|slot| slot.set(zoom));
}

/// The zoom asked for on text and code, kept for the session (never written anywhere).
pub(crate) fn font_zoom_setting() -> u32 {
    FONT_ZOOM_SETTING.with(std::cell::Cell::get)
}

pub(crate) fn set_font_zoom_setting(zoom: u32) {
    FONT_ZOOM_SETTING.with(|slot| slot.set(zoom));
}

/// `size` points at the zoom being painted.
fn zoomed(size: f64) -> f64 {
    size * f64::from(FONT_ZOOM.with(std::cell::Cell::get)) / 100.0
}

/// Code on the card: SF Mono (the system monospaced font) at the shared code size.
fn editor_font() -> Retained<NSFont> {
    mac_ui::fonts::monospace(zoomed(mac_ui::style::FONT_CODE), &[])
}

/// The host of a valid link: the code font, semibold.
fn link_host_font() -> Retained<NSFont> {
    NSFont::monospacedSystemFontOfSize_weight(
        mac_ui::style::FONT_CODE,
        mac_ui::fonts::semibold_weight(),
    )
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
    let attr = AttrText::new(&display, &font, &tone_color(kind, TokenKind::Text));
    let mut offset = 0usize;
    for (token_kind, len) in spans {
        let range = NSRange {
            location: offset,
            length: len,
        };
        attr.color_utf16(range, &tone_color(kind, token_kind));
        if token_kind == TokenKind::LinkHost {
            attr.font_utf16(range, &link_host_font());
        }
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
        TokenKind::LinkScheme => NSColor::secondaryLabelColor(),
        TokenKind::LinkHost => NSColor::linkColor(),
        TokenKind::LinkRest => NSColor::linkColor().colorWithAlphaComponent(0.65),
    }
}

/// How a CSV or TSV cell is drawn. Headers take the system accent, separators the secondary
/// label, and every value (dates, numbers, text) the ordinary label colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsvTone {
    Accent,
    Secondary,
    Label,
}

fn csv_tone(kind: TokenKind) -> CsvTone {
    match kind {
        TokenKind::Key => CsvTone::Accent,
        TokenKind::Punct => CsvTone::Secondary,
        _ => CsvTone::Label,
    }
}

fn tone_color(format: FormatKind, kind: TokenKind) -> Retained<NSColor> {
    if !matches!(format, FormatKind::Csv | FormatKind::Tsv) {
        return color_for(kind);
    }
    match csv_tone(kind) {
        CsvTone::Accent => NSColor::controlAccentColor(),
        CsvTone::Secondary => NSColor::secondaryLabelColor(),
        CsvTone::Label => NSColor::labelColor(),
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
    let Some(storage) = mac_ui::widgets::text_storage(text) else {
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
    if let (Some(from), Some(to)) = (
        mac_ui::widgets::text_container(text),
        mac_ui::widgets::text_container(frozen),
    ) {
        to.setLineFragmentPadding(from.lineFragmentPadding());
    }
    if let Some(frozen_storage) = mac_ui::widgets::text_storage(frozen) {
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
    let layout = mac_ui::widgets::layout_manager(text)?;
    let container = mac_ui::widgets::text_container(text)?;
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

    #[test]
    fn csv_headers_use_the_accent_and_values_stay_plain() {
        use super::{CsvTone, csv_tone};
        use crate::highlight::TokenKind;
        assert_eq!(csv_tone(TokenKind::Key), CsvTone::Accent);
        assert_eq!(csv_tone(TokenKind::Number), CsvTone::Label);
        assert_eq!(csv_tone(TokenKind::String), CsvTone::Label);
        assert_eq!(csv_tone(TokenKind::Text), CsvTone::Label);
        assert_eq!(csv_tone(TokenKind::Punct), CsvTone::Secondary);
    }
}
