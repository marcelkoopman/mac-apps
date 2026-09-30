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
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    fill_round_rect(&mut rgba, 2, 4, 28, 26, 6, accent);
    fill_round_rect(&mut rgba, 5, 8, 22, 20, 4, [36, 44, 68, 255]);
    fill_round_rect(&mut rgba, 7, 10, 18, 16, 3, [244, 246, 252, 255]);
    fill_round_rect(&mut rgba, 12, 5, 8, 6, 2, accent);
    draw_brace_left(&mut rgba, companion_accent(accent));
    draw_brace_right(&mut rgba, accent);
    rgba
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

fn put(rgba: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
        return;
    }
    let i = ((y as u32 * SIZE + x as u32) * 4) as usize;
    rgba[i] = color[0];
    rgba[i + 1] = color[1];
    rgba[i + 2] = color[2];
    rgba[i + 3] = color[3];
}

fn fill_round_rect(rgba: &mut [u8], x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 4]) {
    for py in y..y + h {
        for px in x..x + w {
            if inside_round_rect(px, py, x, y, w, h, r) {
                put(rgba, px, py, color);
            }
        }
    }
}

fn inside_round_rect(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    let cx = if px < x + r {
        px - (x + r)
    } else if px >= x + w - r {
        px - (x + w - 1 - r)
    } else {
        0
    };
    let cy = if py < y + r {
        py - (y + r)
    } else if py >= y + h - r {
        py - (y + h - 1 - r)
    } else {
        0
    };
    cx * cx + cy * cy <= r * r
}

fn draw_brace_left(rgba: &mut [u8], c: [u8; 4]) {
    for y in 14..26 {
        put(rgba, 11, y, c);
    }
    put(rgba, 12, 14, c);
    put(rgba, 12, 25, c);
    put(rgba, 10, 19, c);
    put(rgba, 10, 20, c);
}

fn draw_brace_right(rgba: &mut [u8], c: [u8; 4]) {
    for y in 14..26 {
        put(rgba, 20, y, c);
    }
    put(rgba, 19, 14, c);
    put(rgba, 19, 25, c);
    put(rgba, 21, 19, c);
    put(rgba, 21, 20, c);
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
