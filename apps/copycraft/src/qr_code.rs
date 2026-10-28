//! Text ▾ › QR Code (and the chip on a link card): the text as a QR code, as a new picture
//! copy. Core Image draws the code ([`crate::macos_qr`], one pixel per module); this crops it to
//! its modules and draws it large, black on white, with a quiet zone of four modules. Offline;
//! the text and the picture are never logged.

use crate::image_edit::Rgba;

/// The most bytes a QR code holds at error correction level M (version 40, byte mode).
pub const MAX_BYTES: usize = 2331;
/// The white margin around the code, in modules (the standard's quiet zone).
pub const QUIET_ZONE: usize = 4;
/// The picture's side aims at about this many pixels.
const TARGET_SIDE: usize = 1024;
/// A module is never smaller than this many pixels.
const MIN_MODULE: usize = 4;

/// Whether the text may become a QR code: not empty, not too long, and nothing that stays out
/// of history (a token, key or URL password, or a long text still being checked). A QR code
/// of a secret would be the same secret in a picture.
pub fn offered(text: &str) -> bool {
    use crate::sensitivity::{Record, admission, labeling, record_when};
    !text.trim().is_empty()
        && text.len() <= MAX_BYTES
        && record_when(admission(&labeling(text))) == Record::Now
}

/// The square of modules in `dark` (`width` × `height`, row by row, `true` for a dark pixel):
/// the bounding box of its dark pixels. `None` when there are none or the box is not square.
pub fn modules(dark: &[bool], width: usize, height: usize) -> Option<(Vec<bool>, usize)> {
    if width == 0 || dark.len() != width.checked_mul(height)? {
        return None;
    }
    let mut left = usize::MAX;
    let mut top = usize::MAX;
    let mut right = 0;
    let mut bottom = 0;
    for (at, _) in dark.iter().enumerate().filter(|(_, on)| **on) {
        let (x, y) = (at % width, at / width);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    if left == usize::MAX || right - left != bottom - top {
        return None;
    }
    let side = right - left + 1;
    let square = (top..=bottom)
        .flat_map(|y| (left..=right).map(move |x| dark[y * width + x]))
        .collect();
    Some((square, side))
}

/// The pixels per module for a code of `side` modules.
pub fn module_pixels(side: usize) -> usize {
    (TARGET_SIDE / (side + 2 * QUIET_ZONE)).max(MIN_MODULE)
}

/// The code of `side` × `side` `modules` as an opaque picture: black modules on white, with
/// the quiet zone, each module [`module_pixels`] wide.
pub fn picture(modules: &[bool], side: usize) -> Option<Rgba> {
    if side == 0 || modules.len() != side * side {
        return None;
    }
    let scale = module_pixels(side);
    let full = (side + 2 * QUIET_ZONE) * scale;
    let mut pixels = vec![255u8; full * full * 4];
    for (at, _) in modules.iter().enumerate().filter(|(_, on)| **on) {
        let (mx, my) = (at % side + QUIET_ZONE, at / side + QUIET_ZONE);
        for y in my * scale..(my + 1) * scale {
            let row = (y * full + mx * scale) * 4;
            for pixel in pixels[row..row + scale * 4].chunks_mut(4) {
                pixel[..3].fill(0);
            }
        }
    }
    Rgba::new(full, full, pixels)
}

#[cfg(test)]
#[path = "qr_code_tests.rs"]
mod tests;
