#![cfg(target_os = "macos")]

use mac_ui::objc2::AnyThread;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_app_kit::{
    NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSTextView,
};
use mac_ui::objc2_foundation::{NSMutableAttributedString, NSRange, NSSize, NSString};

use crate::format::FormatKind;
use crate::highlight::{self, TokenKind};

pub(crate) fn configure_scrolling(text: &NSTextView) {
    const LARGE: f64 = 10_000_000.0;
    text.setHorizontallyResizable(true);
    text.setVerticallyResizable(true);
    text.setMaxSize(NSSize::new(LARGE, LARGE));
    if let Some(container) = unsafe { text.textContainer() } {
        container.setWidthTracksTextView(false);
        container.setHeightTracksTextView(false);
        container.setContainerSize(NSSize::new(LARGE, LARGE));
    }
}

/// Size the text view to everything that was painted, so the well can scroll
/// to the last line. `wrap_width` is the well width for prose and Markdown.
/// Other highlighted kinds keep long lines and scroll sideways as well as down.
pub(crate) fn fit_document(text: &NSTextView, wrap_width: Option<f64>) {
    let Some(container) = (unsafe { text.textContainer() }) else {
        return;
    };
    let Some(manager) = (unsafe { text.layoutManager() }) else {
        return;
    };
    const LARGE: f64 = 10_000_000.0;
    let inset = text.textContainerInset();
    if let Some(width) = wrap_width {
        text.setHorizontallyResizable(false);
        text.setVerticallyResizable(true);
        text.setMaxSize(NSSize::new(width.max(1.0), LARGE));
        container.setWidthTracksTextView(true);
        container.setHeightTracksTextView(false);
        let inner = (width - inset.width * 2.0).max(1.0);
        container.setContainerSize(NSSize::new(inner, LARGE));
        text.setFrameSize(NSSize::new(
            width.max(1.0),
            text.frame().size.height.max(1.0),
        ));
    } else {
        configure_scrolling(text);
    }
    manager.ensureLayoutForTextContainer(&container);
    let used = manager.usedRectForTextContainer(&container);
    let width = match wrap_width {
        Some(width) => width,
        None => used.size.width + inset.width * 2.0,
    };
    let height = used.size.height + inset.height * 2.0;
    text.setFrameSize(NSSize::new(width.max(1.0), height.max(1.0)));
}

pub(crate) fn paint(text: &NSTextView, body: &str, highlight: Option<FormatKind>, payload: bool) {
    let attr = if payload {
        match highlight {
            Some(kind) => colored_capped(body, kind),
            None => plain(body, &editor_font(), &NSColor::labelColor()),
        }
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
    let ns = NSString::from_str(body);
    let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
    let all = NSRange {
        location: 0,
        length: ns.length(),
    };
    unsafe {
        attr.addAttribute_value_range(NSFontAttributeName, font, all);
        attr.addAttribute_value_range(NSForegroundColorAttributeName, color, all);
    }
    attr
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
    let ns = NSString::from_str(&display);
    let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
    let all = NSRange {
        location: 0,
        length: ns.length(),
    };
    unsafe {
        attr.addAttribute_value_range(NSFontAttributeName, &font, all);
        attr.addAttribute_value_range(
            NSForegroundColorAttributeName,
            &color_for(TokenKind::Text),
            all,
        );
    }
    let mut offset = 0usize;
    for (token_kind, len) in spans {
        unsafe {
            attr.addAttribute_value_range(
                NSForegroundColorAttributeName,
                &color_for(token_kind),
                NSRange {
                    location: offset,
                    length: len,
                },
            );
        }
        offset += len;
    }
    attr
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
    let mut cap = HIGHLIGHT_CAP.min(body.len());
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
