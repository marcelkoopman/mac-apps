use mac_ui::icon::Canvas;
use tray_icon::Icon;

use crate::format::FormatKind;

const SIZE: u32 = 32;
const DEFAULT_ACCENT: [u8; 4] = [96, 140, 255, 255];
const DEFAULT_TEAL: [u8; 4] = [45, 196, 176, 255];

pub fn menu_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    menu_icon_tinted(None)
}

pub fn menu_icon_tinted(accent: Option<[u8; 4]>) -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(icon_pixels(accent), SIZE, SIZE)?)
}

pub fn accent_for_kind(kind: Option<FormatKind>) -> Option<[u8; 4]> {
    kind.and_then(FormatKind::accent_rgba)
}

fn icon_pixels(accent: Option<[u8; 4]>) -> Vec<u8> {
    let accent = accent.unwrap_or(DEFAULT_ACCENT);
    let mut canvas = Canvas::new(SIZE, SIZE);
    canvas.fill_round_rect(2, 4, 28, 26, 6, accent);
    canvas.fill_round_rect(5, 8, 22, 20, 4, [36, 44, 68, 255]);
    canvas.fill_round_rect(7, 10, 18, 16, 3, [244, 246, 252, 255]);
    canvas.fill_round_rect(12, 5, 8, 6, 2, accent);
    draw_brace_left(&mut canvas, companion_accent(accent));
    draw_brace_right(&mut canvas, accent);
    canvas.into_rgba()
}

fn companion_accent(accent: [u8; 4]) -> [u8; 4] {
    if accent == DEFAULT_ACCENT {
        return DEFAULT_TEAL;
    }
    [
        accent[0].saturating_add(40),
        accent[1].saturating_add(20),
        accent[2].saturating_sub(10),
        255,
    ]
}

fn draw_brace_left(canvas: &mut Canvas, c: [u8; 4]) {
    for y in 14..26 {
        canvas.put(11, y, c);
    }
    canvas.put(12, 14, c);
    canvas.put(12, 25, c);
    canvas.put(10, 19, c);
    canvas.put(10, 20, c);
}

fn draw_brace_right(canvas: &mut Canvas, c: [u8; 4]) {
    for y in 14..26 {
        canvas.put(20, y, c);
    }
    canvas.put(19, 14, c);
    canvas.put(19, 25, c);
    canvas.put(21, 19, c);
    canvas.put(21, 20, c);
}

#[cfg(test)]
mod tests {
    use super::{accent_for_kind, icon_pixels, menu_icon, menu_icon_tinted};
    use crate::format::FormatKind;

    #[test]
    fn builds_rgba_icon() {
        assert!(menu_icon().is_ok());
    }

    #[test]
    fn builds_tinted_icon_for_rust() {
        let accent = FormatKind::Rust.accent_rgba();
        assert!(menu_icon_tinted(accent).is_ok());
    }

    #[test]
    fn pixels_follow_the_detected_accent() {
        let plain = icon_pixels(accent_for_kind(Some(FormatKind::Plain)));
        let text = icon_pixels(accent_for_kind(Some(FormatKind::Text)));
        let rust = icon_pixels(accent_for_kind(Some(FormatKind::Rust)));
        let json = icon_pixels(accent_for_kind(Some(FormatKind::Json)));
        let image = icon_pixels(accent_for_kind(Some(FormatKind::Image)));
        assert_ne!(plain, icon_pixels(None));
        assert_ne!(plain, text);
        assert_ne!(text, rust);
        assert_ne!(rust, json);
        assert_ne!(json, image);
    }
}
