#![cfg(target_os = "macos")]

//! Previews of pasted and dropped pictures: a thumbnail decoded by ImageIO, no bigger than
//! [`PREVIEW_MAX_EDGE`], without decoding the whole picture into ImageIO's cache. Built on
//! objc2-image-io / objc2-core-graphics, with the helpers of [`crate::macos_image_edit`].

use std::path::Path;

use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType, CFURL, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGColorSpace, CGContext, CGImage};
use objc2_image_io::CGImageSource;

use crate::clipboard::ClipboardImage;
use crate::image_ops::PREVIEW_MAX_EDGE;
use crate::macos_image_edit::{
    Properties, RGBA_PREMULTIPLIED, SourceOf, bitmap_in, image_source, keys, pixel_numbers,
    properties, thumbnail,
};

pub(crate) fn preview_from_bytes(bytes: &[u8]) -> Option<ClipboardImage> {
    let data = CFData::from_bytes(bytes);
    let source = image_source(SourceOf::Data(&data), Some(&source_options()))?;
    preview_from_source(&source)
}

pub(crate) fn preview_from_path(path: &Path) -> Option<ClipboardImage> {
    // Only UTF-8 paths, as before (`NSString` took the `&str`).
    let url = CFURL::from_file_path(path.to_str()?)?;
    let source = image_source(SourceOf::Url(&url), Some(&source_options()))?;
    preview_from_source(&source)
}

/// Keep ImageIO from decoding a large paste into a full bitmap just to read it.
fn source_options() -> CFRetained<Properties> {
    let no = CFBoolean::new(false);
    let keys = keys();
    let keys: [&CFString; 2] = [keys.should_cache, keys.should_cache_immediately];
    let values: [&CFType; 2] = [no, no];
    CFDictionary::from_slices(&keys, &values)
}

fn preview_from_source(source: &CGImageSource) -> Option<ClipboardImage> {
    let (full_width, full_height) = oriented_size(source);
    let thumb = thumbnail(source, i64::try_from(PREVIEW_MAX_EDGE).ok()?)?;
    let (width, height) = (CGImage::width(Some(&thumb)), CGImage::height(Some(&thumb)));
    let mut rgba = rgba_from_image(&thumb, width, height)?;
    unpremultiply(&mut rgba);
    let mut image = ClipboardImage::new(width, height, rgba)?;
    if full_width == 0 || full_height == 0 {
        image.full_width = width;
        image.full_height = height;
    } else {
        image.full_width = full_width;
        image.full_height = full_height;
    }
    Some(image)
}

/// The full picture's width and height, its EXIF orientation applied; 0 for what is missing.
fn oriented_size(source: &CGImageSource) -> (usize, usize) {
    let Some(props) = properties(source) else {
        return (0, 0);
    };
    let [width, height, orientation] = pixel_numbers(&props);
    let size = |number: Option<i64>| {
        number
            .filter(|number| *number > 0)
            .and_then(|number| usize::try_from(number).ok())
            .unwrap_or(0)
    };
    let (mut width, mut height) = (size(width), size(height));
    if matches!(size(orientation), 5..=8) {
        std::mem::swap(&mut width, &mut height);
    }
    (width, height)
}

/// Run `draw` on a premultiplied RGBA bitmap context (device RGB) over `rgba` (`width` ×
/// `height` pixels, 4 bytes each). The context is released before this returns. `None` when
/// the buffer has the wrong length or the system makes no context.
pub(crate) fn with_bitmap<R>(
    rgba: &mut [u8],
    width: usize,
    height: usize,
    draw: impl FnOnce(&CGContext) -> R,
) -> Option<R> {
    let space = CGColorSpace::new_device_rgb()?;
    let context = bitmap_in(rgba, width, height, RGBA_PREMULTIPLIED, &space)?;
    let result = draw(&context);
    drop(context);
    Some(result)
}

fn rgba_from_image(image: &CGImage, width: usize, height: usize) -> Option<Vec<u8>> {
    let len = width.checked_mul(4)?.checked_mul(height)?;
    if len == 0 {
        return None;
    }
    let mut rgba = vec![0u8; len];
    with_bitmap(&mut rgba, width, height, |context| {
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(context), rect, Some(image));
    })?;
    Some(rgba)
}

fn unpremultiply(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let alpha = u16::from(px[3]);
        if alpha == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if alpha != 255 {
            px[0] = ((u16::from(px[0]) * 255) / alpha) as u8;
            px[1] = ((u16::from(px[1]) * 255) / alpha) as u8;
            px[2] = ((u16::from(px[2]) * 255) / alpha) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use image::ExtendedColorType;
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;

    use super::{preview_from_bytes, preview_from_path, unpremultiply, with_bitmap};

    fn encode_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        PngEncoder::new(&mut buf)
            .write_image(rgba, width, height, ExtendedColorType::Rgba8)
            .expect("png");
        buf
    }

    #[test]
    fn unreadable_image_returns_none_without_releasing_null() {
        assert!(preview_from_bytes(b"not an image").is_none());
        let path = std::env::temp_dir().join("copycraft-not-an-image.txt");
        std::fs::write(&path, b"hello").expect("write");
        let preview = preview_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(preview.is_none());
        assert!(preview_from_path(std::path::Path::new("/no/such/copycraft-image.png")).is_none());
    }

    #[test]
    fn thumbnail_keeps_small_png_pixels() {
        let png = encode_rgba(
            2,
            2,
            &[255, 0, 0, 255, 255, 0, 0, 128, 0, 0, 255, 255, 0, 0, 0, 0],
        );
        let image = preview_from_bytes(&png).expect("preview");
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!((image.full_width, image.full_height), (2, 2));
        assert_eq!(
            image.rgba,
            [255, 0, 0, 255, 255, 0, 0, 128, 0, 0, 255, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn thumbnail_shrinks_the_long_edge_and_keeps_full_size() {
        let width = 1280usize;
        let height = 4usize;
        let mut rgba = vec![0u8; width * height * 4];
        for px in rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[10, 20, 30, 255]);
        }
        let png = encode_rgba(width as u32, height as u32, &rgba);
        let image = preview_from_bytes(&png).expect("preview");
        assert_eq!((image.full_width, image.full_height), (width, height));
        assert!(image.width < width);
        assert!(image.width.max(image.height) <= 640);
        let path = std::env::temp_dir().join("copycraft-thumb.png");
        std::fs::write(&path, &png).expect("write");
        let from_path = preview_from_path(&path);
        let _ = std::fs::remove_file(&path);
        let from_path = from_path.expect("path preview");
        assert_eq!(
            (from_path.full_width, from_path.full_height),
            (width, height)
        );
        assert!(from_path.width.max(from_path.height) <= 640);
    }

    #[test]
    fn with_bitmap_checks_the_buffer_and_draws_into_it() {
        let mut short = vec![0u8; 15];
        assert!(with_bitmap(&mut short, 2, 2, |_| ()).is_none());
        assert!(with_bitmap(&mut [], 0, 0, |_| ()).is_none());
        let mut rgba = vec![0u8; 16];
        assert_eq!(with_bitmap(&mut rgba, 2, 2, |_| 7), Some(7));
    }

    #[test]
    fn unpremultiply_truncates_and_clears_transparent_pixels() {
        let mut px = [100, 50, 0, 128, 9, 9, 9, 0, 1, 2, 3, 255];
        unpremultiply(&mut px);
        assert_eq!(px, [199, 99, 0, 128, 0, 0, 0, 0, 1, 2, 3, 255]);
    }
}
