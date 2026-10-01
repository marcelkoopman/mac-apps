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
    use super::{Canvas, IconError, from_image_file, pixel_count};

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
