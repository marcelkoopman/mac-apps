use mac_ui::icon::{Canvas, IconError};
use mac_ui::tray_icon::Icon;

use crate::format::FormatKind;

const SIZE: u32 = 32;
const DEFAULT_ACCENT: [u8; 4] = [96, 140, 255, 255];
const DEFAULT_TEAL: [u8; 4] = [45, 196, 176, 255];
const INK: [u8; 4] = [0, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// The neutral menu bar icon: the clipboard glyph as a template image (black and clear), which
/// macOS draws in the menu bar's own colour. Show it with `template: true`.
pub fn menu_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(template_pixels()?, SIZE, SIZE)?)
}

/// Menu bar icon for `accent` and whether it is a template image. Nothing detected (`None`) gives
/// the template glyph, which follows light, dark and tinted menu bars. A detected kind keeps its
/// colour, because the colour tells which kind was copied, so that icon is not a template.
pub fn menu_icon_for(accent: Option<[u8; 4]>) -> Result<(Icon, bool), Box<dyn std::error::Error>> {
    match accent {
        None => Ok((menu_icon()?, true)),
        Some(accent) => Ok((menu_icon_tinted(Some(accent))?, false)),
    }
}

pub fn menu_icon_tinted(accent: Option<[u8; 4]>) -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(icon_pixels(accent)?, SIZE, SIZE)?)
}

pub fn accent_for_kind(kind: Option<FormatKind>) -> Option<[u8; 4]> {
    kind.and_then(FormatKind::accent_rgba)
}

fn icon_pixels(accent: Option<[u8; 4]>) -> Result<Vec<u8>, IconError> {
    let accent = accent.unwrap_or(DEFAULT_ACCENT);
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, accent);
    canvas.fill_round_rect(5, 8, 22, 20, 4, [36, 44, 68, 255]);
    canvas.fill_round_rect(7, 10, 18, 16, 3, [244, 246, 252, 255]);
    canvas.fill_round_rect(12, 5, 8, 6, 2, accent);
    draw_brace_left(&mut canvas, companion_accent(accent));
    draw_brace_right(&mut canvas, accent);
    Ok(canvas.into_rgba())
}

/// The same clipboard as [`icon_pixels`] as a template mask: the board and the paper are ink,
/// the frame between them and the braces are cut out.
fn template_pixels() -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, INK);
    canvas.fill_round_rect(5, 8, 22, 20, 4, CLEAR);
    canvas.fill_round_rect(7, 10, 18, 16, 3, INK);
    canvas.fill_round_rect(12, 5, 8, 6, 2, INK);
    draw_brace_left(&mut canvas, CLEAR);
    draw_brace_right(&mut canvas, CLEAR);
    Ok(canvas.into_rgba())
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
    use super::{
        SIZE, accent_for_kind, icon_pixels, menu_icon, menu_icon_for, menu_icon_tinted,
        template_pixels,
    };
    use crate::format::FormatKind;

    #[test]
    fn template_glyph_is_black_and_clear_only() {
        let pixels = template_pixels().unwrap();
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        assert!(
            pixels
                .chunks(4)
                .all(|px| px == [0, 0, 0, 255] || px == [0, 0, 0, 0])
        );
        let alpha = |x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(alpha(3, 18), 255, "board");
        assert_eq!(alpha(5, 18), 0, "frame between board and paper");
        assert_eq!(alpha(15, 12), 255, "paper");
        assert_eq!(alpha(11, 19), 0, "left brace");
        assert_eq!(alpha(20, 19), 0, "right brace");
        assert_eq!(alpha(16, 6), 255, "clip");
        assert_eq!(alpha(0, 0), 0, "corner");
    }

    #[test]
    fn only_the_neutral_icon_is_a_template() {
        assert!(menu_icon_for(None).unwrap().1);
        let rust = accent_for_kind(Some(FormatKind::Rust));
        assert!(!menu_icon_for(rust).unwrap().1);
        let plain = accent_for_kind(Some(FormatKind::Plain));
        assert!(!menu_icon_for(plain).unwrap().1);
    }

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
        let pixels = |kind| icon_pixels(accent_for_kind(kind)).unwrap();
        let plain = pixels(Some(FormatKind::Plain));
        let text = pixels(Some(FormatKind::Text));
        let rust = pixels(Some(FormatKind::Rust));
        let json = pixels(Some(FormatKind::Json));
        let image = pixels(Some(FormatKind::Image));
        assert_ne!(plain, pixels(None));
        assert_ne!(plain, text);
        assert_ne!(text, rust);
        assert_ne!(rust, json);
        assert_ne!(json, image);
    }
}
