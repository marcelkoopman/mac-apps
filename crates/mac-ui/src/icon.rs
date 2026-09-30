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
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0u8; (width * height * 4) as usize],
        }
    }

    /// Canvas filled with one color.
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            rgba: color.repeat((width * height) as usize),
        }
    }

    /// Set one pixel. Points outside the canvas are ignored.
    pub fn put(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&color);
    }

    /// Fill a `w`×`h` rectangle at (`x`, `y`) with corner radius `r`.
    pub fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: [u8; 4]) {
        for py in y..y + h {
            for px in x..x + w {
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

    pub fn into_icon(self) -> Result<Icon, BadIcon> {
        Icon::from_rgba(self.rgba, self.width, self.height)
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

/// Why [`from_image_file`] failed.
#[derive(Debug)]
pub enum IconError {
    Io(std::io::Error),
    Image(image::ImageError),
    Icon(BadIcon),
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "cannot open icon: {e}"),
            Self::Image(e) => write!(f, "cannot decode icon: {e}"),
            Self::Icon(e) => write!(f, "invalid icon: {e}"),
        }
    }
}

impl std::error::Error for IconError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Image(e) => Some(e),
            Self::Icon(e) => Some(e),
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
    use super::{Canvas, from_image_file};

    #[test]
    fn put_ignores_points_outside() {
        let mut canvas = Canvas::new(2, 2);
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
        let mut canvas = Canvas::new(8, 8);
        canvas.fill_round_rect(0, 0, 8, 8, 3, [255, 255, 255, 255]);
        let rgba = canvas.into_rgba();
        let alpha = |x: usize, y: usize| rgba[(y * 8 + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(4, 4), 255);
        assert_eq!(alpha(0, 4), 255);
    }

    #[test]
    fn filled_repeats_the_color() {
        let rgba = Canvas::filled(16, 16, [255, 80, 80, 255]).into_rgba();
        assert_eq!(rgba.len(), 16 * 16 * 4);
        assert!(rgba.chunks(4).all(|px| px == [255, 80, 80, 255]));
    }

    #[test]
    fn canvas_builds_an_icon() {
        assert!(Canvas::new(32, 32).into_icon().is_ok());
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(from_image_file(std::path::Path::new("/nonexistent/icon.png")).is_err());
    }
}
