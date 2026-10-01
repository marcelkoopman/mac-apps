//! Menu bar icon helpers: RGBA rasterizing, image files and SF Symbols.

use std::fmt;
use std::path::Path;

use tray_icon::{BadIcon, Icon};

/// RGBA8 pixel buffer for drawing small menu bar icons by hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    /// Fully transparent canvas.
    ///
    /// # Errors
    ///
    /// [`IconError::TooLarge`] when `width * height * 4` bytes do not fit in `usize`.
    pub fn new(width: u32, height: u32) -> Result<Self, IconError> {
        Self::filled(width, height, [0; 4])
    }

    /// Canvas filled with one color.
    ///
    /// # Errors
    ///
    /// [`IconError::TooLarge`] when `width * height * 4` bytes do not fit in `usize`.
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Result<Self, IconError> {
        let pixels = pixel_count(width, height).ok_or(IconError::TooLarge { width, height })?;
        Ok(Self {
            width,
            height,
            rgba: color.repeat(pixels),
        })
    }

    /// Set one pixel. Points outside the canvas are ignored.
    pub fn put(&mut self, x: i32, y: i32, color: [u8; 4]) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        // Cannot overflow or go past the buffer: x < width, y < height, and the
        // constructor checked that width * height * 4 fits in usize.
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.rgba[i..i + 4].copy_from_slice(&color);
    }

    /// Fill a `w`×`h` rectangle at (`x`, `y`) with corner radius `r`.
    ///
    /// Parts outside the canvas are skipped.
    pub fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 4]) {
        let max_x = i32::try_from(self.width).unwrap_or(i32::MAX);
        let max_y = i32::try_from(self.height).unwrap_or(i32::MAX);
        for py in y.max(0)..y.saturating_add(h).min(max_y) {
            for px in x.max(0)..x.saturating_add(w).min(max_x) {
                if inside_round_rect(px, py, x, y, w, h, r) {
                    self.put(px, py, color);
                }
            }
        }
    }

    /// Raw RGBA8 bytes, row by row.
    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// Convert to a tray icon of the canvas size.
    ///
    /// # Errors
    ///
    /// [`BadIcon`] when `tray_icon` rejects the size or pixel data.
    pub fn into_icon(self) -> Result<Icon, BadIcon> {
        Icon::from_rgba(self.rgba, self.width, self.height)
    }
}

/// Number of pixels, or `None` when the RGBA8 byte count overflows `usize`.
fn pixel_count(width: u32, height: u32) -> Option<usize> {
    let pixels = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    pixels.checked_mul(4)?;
    Some(pixels)
}

fn inside_round_rect(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32, r: i32) -> bool {
    // i64 so sums of i32 inputs cannot overflow; squares saturate, which keeps
    // the comparison correct because r * r always fits.
    let (px, py, x, y, w, h, r) = (
        i64::from(px),
        i64::from(py),
        i64::from(x),
        i64::from(y),
        i64::from(w),
        i64::from(h),
        i64::from(r),
    );
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
    cx.saturating_mul(cx).saturating_add(cy.saturating_mul(cy)) <= r * r
}

/// Why [`from_image_file`], [`Canvas::new`] or [`Canvas::filled`] failed.
#[derive(Debug)]
pub enum IconError {
    /// The image file could not be opened or read.
    Io(std::io::Error),
    /// The image file could not be decoded.
    Image(image::ImageError),
    /// `tray_icon` rejected the pixels.
    Icon(BadIcon),
    /// A [`Canvas`] of `width`×`height` RGBA8 pixels does not fit in memory (`usize`).
    TooLarge { width: u32, height: u32 },
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot open icon: {e}"),
            Self::Image(e) => write!(f, "cannot decode icon: {e}"),
            Self::Icon(e) => write!(f, "invalid icon: {e}"),
            Self::TooLarge { width, height } => {
                write!(f, "icon canvas {width}x{height} is too large")
            }
        }
    }
}

impl std::error::Error for IconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Image(e) => Some(e),
            Self::Icon(e) => Some(e),
            Self::TooLarge { .. } => None,
        }
    }
}

impl From<std::io::Error> for IconError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<image::ImageError> for IconError {
    fn from(e: image::ImageError) -> Self {
        Self::Image(e)
    }
}

impl From<BadIcon> for IconError {
    fn from(e: BadIcon) -> Self {
        Self::Icon(e)
    }
}

/// Decode an image file (format from the extension) into a tray icon at its own size.
///
/// mac-ui enables PNG only; formats the app enables on its own `image` dependency work too.
///
/// # Errors
///
/// - [`IconError::Io`] when the file cannot be opened.
/// - [`IconError::Image`] when the format is unknown or decoding fails.
/// - [`IconError::Icon`] when `tray_icon` rejects the decoded pixels.
pub fn from_image_file(path: &Path) -> Result<Icon, IconError> {
    let img = image::ImageReader::open(path)?.decode()?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok(Icon::from_rgba(img.into_raw(), w, h)?)
}

/// Luminance (0-255, Rec. 709 weights) at or below which [`template_mask`] makes a pixel clear.
pub const TEMPLATE_DARK: u8 = 80;
/// Luminance at or above which [`template_mask`] makes a pixel fully opaque.
pub const TEMPLATE_LIGHT: u8 = 140;

/// Turn full-colour RGBA8 artwork (light glyph on a dark tile) into a template image: black,
/// with the light parts opaque and the dark tile clear. Luminance between [`TEMPLATE_DARK`] and
/// [`TEMPLATE_LIGHT`] ramps the alpha, which keeps anti-aliased edges smooth, and the source
/// alpha still applies. A trailing partial pixel is dropped.
///
/// macOS draws a template image (`NSImage.isTemplate`) from its alpha only, in the menu bar's
/// own colour, so it adapts to light, dark and tinted menu bars.
pub fn template_mask(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 4);
    let (pixels, _partial) = rgba.as_chunks::<4>();
    for px in pixels {
        let luma =
            (2126 * u32::from(px[0]) + 7152 * u32::from(px[1]) + 722 * u32::from(px[2])) / 10_000;
        let (dark, light) = (u32::from(TEMPLATE_DARK), u32::from(TEMPLATE_LIGHT));
        let ramp = if luma <= dark {
            0
        } else if luma >= light {
            255
        } else {
            (luma - dark) * 255 / (light - dark)
        };
        let alpha = ramp * u32::from(px[3]) / 255;
        // alpha <= 255: ramp <= 255 and px[3] <= 255.
        out.extend_from_slice(&[0, 0, 0, alpha as u8]);
    }
    out
}

/// Crop a `width`×`height` RGBA8 image to the box around its non-clear pixels plus `pad`
/// pixels on each side (kept inside the image). An image without visible pixels, or whose
/// buffer does not match the size, comes back unchanged.
pub fn trim_clear(rgba: Vec<u8>, width: u32, height: u32, pad: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (width as usize, height as usize);
    if pixel_count(width, height).map(|n| n * 4) != Some(rgba.len()) {
        return (rgba, width, height);
    }
    let visible = |x: usize, y: usize| rgba[(y * w + x) * 4 + 3] > 0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if visible(x, y) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return (rgba, width, height);
    }
    let pad = pad as usize;
    let (x0, y0) = (min_x.saturating_sub(pad), min_y.saturating_sub(pad));
    let (x1, y1) = ((max_x + pad).min(w - 1), (max_y + pad).min(h - 1));
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        out.extend_from_slice(&rgba[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
    }
    // cw <= width and ch <= height, so both fit in u32.
    (out, cw as u32, ch as u32)
}

/// Decode an image file like [`from_image_file`] and turn it into a template image with
/// [`template_mask`], cropped to the glyph (plus a 1/16 margin) so it fills the menu bar height.
/// Show it with [`crate::tray::set_icon`] and `template: true`.
///
/// # Errors
///
/// The same as [`from_image_file`].
pub fn template_from_image_file(path: &Path) -> Result<Icon, IconError> {
    let (rgba, w, h) = template_rgba_from_image_file(path)?;
    Ok(Icon::from_rgba(rgba, w, h)?)
}

/// The pixels and size [`template_from_image_file`] makes into an icon, for tests and checks.
///
/// # Errors
///
/// [`IconError::Io`] or [`IconError::Image`], as for [`from_image_file`].
pub fn template_rgba_from_image_file(path: &Path) -> Result<(Vec<u8>, u32, u32), IconError> {
    let img = image::ImageReader::open(path)?.decode()?.to_rgba8();
    let (w, h) = img.dimensions();
    let pad = w.max(h) / 16;
    Ok(trim_clear(template_mask(img.as_raw()), w, h, pad))
}

/// SF Symbol by name, or `None` before macOS 11 or for an unknown symbol.
#[cfg(target_os = "macos")]
pub fn system_symbol(
    name: &str,
    description: &str,
) -> Option<objc2::rc::Retained<objc2_app_kit::NSImage>> {
    use objc2::{ClassType, sel};
    use objc2_app_kit::NSImage;
    use objc2_foundation::NSString;

    let selector = sel!(imageWithSystemSymbolName:accessibilityDescription:);
    if !NSImage::class().metaclass().responds_to(selector) {
        return None;
    }
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Canvas, IconError, TEMPLATE_DARK, TEMPLATE_LIGHT, from_image_file, pixel_count,
        template_from_image_file, template_mask, trim_clear,
    };

    #[test]
    fn trim_clear_crops_to_the_glyph_with_padding() {
        let mut canvas = Canvas::new(8, 6).unwrap();
        canvas.put(3, 2, [0, 0, 0, 255]);
        canvas.put(4, 3, [0, 0, 0, 255]);
        let (rgba, w, h) = trim_clear(canvas.clone().into_rgba(), 8, 6, 0);
        assert_eq!((w, h), (2, 2));
        assert_eq!(alphas(&rgba), vec![255, 0, 0, 255]);
        let (rgba, w, h) = trim_clear(canvas.into_rgba(), 8, 6, 1);
        assert_eq!((w, h), (4, 4));
        assert_eq!(rgba.len(), 4 * 4 * 4);
        assert_eq!(alphas(&rgba)[5], 255);
    }

    #[test]
    fn trim_clear_keeps_padding_inside_the_image() {
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.put(0, 0, [0, 0, 0, 255]);
        let (_, w, h) = trim_clear(canvas.into_rgba(), 4, 4, 2);
        assert_eq!((w, h), (3, 3));
    }

    #[test]
    fn trim_clear_leaves_empty_or_mismatched_images() {
        let clear = Canvas::new(4, 4).unwrap().into_rgba();
        assert_eq!(trim_clear(clear.clone(), 4, 4, 1), (clear, 4, 4));
        assert_eq!(trim_clear(vec![0; 8], 4, 4, 0), (vec![0; 8], 4, 4));
    }

    fn alphas(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).map(|px| px[3]).collect()
    }

    #[test]
    fn template_mask_keeps_light_parts_and_clears_the_dark_tile() {
        let src = [
            255, 255, 255, 255, // white glyph
            20, 30, 60, 255, // dark navy tile
            45, 196, 176, 255, // teal glyph
            0, 0, 0, 0, // transparent corner
            255, 255, 255, 128, // half-transparent white edge
        ];
        let out = template_mask(&src);
        assert_eq!(out.len(), src.len());
        assert!(out.chunks(4).all(|px| px[..3] == [0, 0, 0]));
        assert_eq!(alphas(&out), vec![255, 0, 255, 0, 128]);
    }

    #[test]
    fn template_mask_ramps_between_the_thresholds() {
        let grey = |v: u8| [v, v, v, 255];
        let at = |v: u8| template_mask(&grey(v))[3];
        assert_eq!(at(TEMPLATE_DARK), 0);
        assert_eq!(at(TEMPLATE_LIGHT), 255);
        let mid = at((TEMPLATE_DARK + TEMPLATE_LIGHT) / 2);
        assert!(mid > 100 && mid < 155, "{mid}");
        assert!(at(100) < at(120));
    }

    #[test]
    fn template_mask_drops_a_partial_pixel() {
        assert_eq!(template_mask(&[255, 255, 255, 255, 9, 9]).len(), 4);
        assert!(template_mask(&[]).is_empty());
    }

    #[test]
    fn missing_template_file_is_an_error() {
        assert!(template_from_image_file(std::path::Path::new("/nonexistent/icon.png")).is_err());
    }

    #[test]
    fn put_ignores_points_outside() {
        let mut canvas = Canvas::new(2, 2).unwrap();
        canvas.put(-1, 0, [1, 2, 3, 4]);
        canvas.put(2, 1, [1, 2, 3, 4]);
        canvas.put(1, 1, [9, 8, 7, 6]);
        assert_eq!(
            canvas.into_rgba(),
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 8, 7, 6]
        );
    }

    #[test]
    fn round_rect_skips_corners() {
        let mut canvas = Canvas::new(8, 8).unwrap();
        canvas.fill_round_rect(0, 0, 8, 8, 3, [255, 255, 255, 255]);
        let rgba = canvas.into_rgba();
        let alpha = |x: usize, y: usize| rgba[(y * 8 + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(4, 4), 255);
        assert_eq!(alpha(0, 4), 255);
    }

    #[test]
    fn filled_repeats_the_color() {
        let rgba = Canvas::filled(16, 16, [255, 80, 80, 255])
            .unwrap()
            .into_rgba();
        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert!(rgba.chunks(4).all(|px| px == [255, 80, 80, 255]));
    }

    #[test]
    fn canvas_builds_an_icon() {
        assert!(Canvas::new(32, 32).unwrap().into_icon().is_ok());
    }

    #[test]
    fn new_has_four_bytes_per_pixel() {
        let rgba = Canvas::new(32, 16).unwrap().into_rgba();
        assert_eq!(rgba.len(), 32 * 16 * 4);
        assert!(rgba.iter().all(|&b| b == 0));
    }

    #[test]
    fn empty_canvas_is_allowed() {
        assert!(Canvas::new(0, 0).unwrap().into_rgba().is_empty());
    }

    #[test]
    fn oversized_canvas_is_an_error() {
        assert!(matches!(
            Canvas::new(u32::MAX, u32::MAX),
            Err(IconError::TooLarge {
                width: u32::MAX,
                height: u32::MAX
            })
        ));
        assert!(matches!(
            Canvas::filled(u32::MAX, u32::MAX, [1, 2, 3, 4]),
            Err(IconError::TooLarge { .. })
        ));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn size_past_u32_is_counted_in_usize() {
        // 65536 * 65536 * 4 wrapped to 0 in u32 before.
        assert_eq!(pixel_count(65536, 65536), Some(1 << 32));
        assert_eq!(pixel_count(u32::MAX, u32::MAX), None);
    }

    #[test]
    fn round_rect_off_canvas_does_not_panic() {
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.fill_round_rect(i32::MAX - 1, i32::MIN, i32::MAX, i32::MAX, i32::MAX, [1; 4]);
        canvas.fill_round_rect(0, 0, 4, 4, i32::MAX, [2; 4]);
        canvas.fill_round_rect(-2, -2, i32::MAX, i32::MAX, 0, [7; 4]);
        assert!(canvas.into_rgba().chunks(4).all(|px| px == [7; 4]));
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(from_image_file(std::path::Path::new("/nonexistent/icon.png")).is_err());
    }
}
