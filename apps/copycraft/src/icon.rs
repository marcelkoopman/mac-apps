use mac_ui::icon::{Canvas, IconError};
use mac_ui::tray_icon::Icon;

const SIZE: u32 = 32;
const INK: [u8; 4] = [0, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// The menu bar icon: the clipboard glyph as a template image (black and clear only), which
/// macOS draws in the menu bar's own colour, light, dark or tinted. It is the same in every
/// state. The detected kind is in the tooltip and the VoiceOver label instead. Show it with
/// `template: true`.
pub fn menu_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(icon_pixels()?, SIZE, SIZE)?)
}

/// The flash frame of the blink after a copy: the same clipboard filled solid, braces still cut
/// out. Also a template (`template: true`).
pub fn flash_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(flash_pixels()?, SIZE, SIZE)?)
}

/// A clipboard with braces on the paper: the board, the clip and the paper are ink; the frame
/// between board and paper and the braces are cut out.
fn icon_pixels() -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, INK);
    canvas.fill_round_rect(5, 8, 22, 20, 4, CLEAR);
    canvas.fill_round_rect(7, 10, 18, 16, 3, INK);
    canvas.fill_round_rect(12, 5, 8, 6, 2, INK);
    draw_brace_left(&mut canvas, CLEAR);
    draw_brace_right(&mut canvas, CLEAR);
    Ok(canvas.into_rgba())
}

/// The board filled solid (the clip sits inside it), braces cut out: same outline as the glyph.
fn flash_pixels() -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, INK);
    draw_brace_left(&mut canvas, CLEAR);
    draw_brace_right(&mut canvas, CLEAR);
    Ok(canvas.into_rgba())
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
    use super::{SIZE, flash_icon, flash_pixels, icon_pixels, menu_icon};

    fn mask_only(pixels: &[u8]) -> bool {
        pixels
            .chunks(4)
            .all(|px| px == [0, 0, 0, 255] || px == [0, 0, 0, 0])
    }

    #[test]
    fn builds_the_template_icon() {
        assert!(menu_icon().is_ok());
        assert!(flash_icon().is_ok());
    }

    #[test]
    fn icon_is_black_and_clear_only() {
        let pixels = icon_pixels().unwrap();
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        assert!(mask_only(&pixels));
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
    fn icon_does_not_change() {
        // One glyph for every clipboard state: nothing to recolour per kind.
        assert_eq!(icon_pixels().unwrap(), icon_pixels().unwrap());
    }

    #[test]
    fn flash_glyph_is_a_filled_template_of_the_same_size() {
        let flash = flash_pixels().unwrap();
        let normal = icon_pixels().unwrap();
        assert_eq!(flash.len(), normal.len());
        assert!(mask_only(&flash));
        assert_ne!(flash, normal);
        let alpha = |x: usize, y: usize| flash[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(alpha(5, 18), 255, "frame is filled");
        assert_eq!(alpha(11, 19), 0, "left brace stays cut out");
        assert_eq!(alpha(20, 19), 0, "right brace stays cut out");
        assert_eq!(alpha(0, 0), 0, "corner");
    }
}
