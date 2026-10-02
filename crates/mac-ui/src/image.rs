//! `NSImage` from encoded bytes or straight RGBA8 pixels, for an image view.
//!
//! Menu bar icons stay in [`crate::icon`]. This module is the picture an `NSImageView` draws.

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSCalibratedRGBColorSpace, NSImage, NSImageRep,
};
use objc2_foundation::{NSData, NSSize};

/// `NSImage` decoded from encoded bytes (PNG, JPEG, TIFF, anything `NSImage` reads).
///
/// `None` when AppKit cannot decode `bytes`. The image is not a template.
pub fn from_bytes(bytes: &[u8]) -> Option<Retained<NSImage>> {
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    image.setTemplate(false);
    Some(image)
}

/// `NSImage` from straight (non-premultiplied) RGBA8 pixels, top to bottom, `width * 4` bytes
/// per row.
///
/// `None` when `width` or `height` is zero, or when `rgba` is not `width * height * 4` bytes.
///
/// A raw bitmap with straight alpha draws black once the view is layer-backed. Prefer
/// [`from_bytes`] with PNG bytes for a picture shown in a layer-backed view: AppKit's decoder
/// composites that correctly.
pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Option<Retained<NSImage>> {
    if width == 0 || height == 0 {
        return None;
    }
    let width = isize::try_from(width).ok()?;
    let height = isize::try_from(height).ok()?;
    let row = width.checked_mul(4)?;
    let pixels = row.checked_mul(height)?;
    let len = usize::try_from(pixels).ok()?;
    if rgba.len() != len {
        return None;
    }
    // SAFETY: `planes` is null, so AppKit allocates the buffer. The format below is 8 bits,
    // 4 samples, non-premultiplied alpha, `row` bytes per row.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width,
            height,
            8,
            4,
            true,
            false,
            NSCalibratedRGBColorSpace,
            NSBitmapFormat::AlphaNonpremultiplied,
            row,
            32,
        )
    }?;
    // SAFETY: `bitmapData` is the buffer just allocated, `row * height` bytes, and `rgba` is
    // that long. A null pointer means AppKit did not allocate it.
    unsafe {
        let dest = rep.bitmapData();
        if dest.is_null() {
            return None;
        }
        dest.copy_from(rgba.as_ptr(), rgba.len());
    }
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(width as f64, height as f64));
    image.addRepresentation(&rep);
    Some(image)
}

/// Pixel size of `image`. Zero when it has no representation and no positive point size.
pub fn pixel_size(image: &NSImage) -> (usize, usize) {
    let reps = image.representations();
    for index in 0..reps.count() {
        let rep: Retained<NSImageRep> = reps.objectAtIndex(index);
        let width = rep.pixelsWide();
        let height = rep.pixelsHigh();
        if width > 0 && height > 0 {
            return (width as usize, height as usize);
        }
    }
    (
        positive_pixels(image.size().width),
        positive_pixels(image.size().height),
    )
}

fn positive_pixels(value: f64) -> usize {
    if value.is_finite() && value > 0.0 {
        value.round() as usize
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use ::image::ExtendedColorType;
    use ::image::ImageEncoder;
    use ::image::codecs::png::PngEncoder;

    use super::{from_bytes, from_rgba, pixel_size};

    fn png() -> Vec<u8> {
        let mut buf = Vec::new();
        PngEncoder::new(&mut buf)
            .write_image(
                &[
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
                2,
                2,
                ExtendedColorType::Rgba8,
            )
            .expect("png");
        buf
    }

    #[test]
    fn bytes_keep_the_pixel_size() {
        let image = from_bytes(&png()).expect("png");
        assert_eq!(pixel_size(&image), (2, 2));
        assert!(!image.isTemplate());
    }

    #[test]
    fn empty_bytes_are_not_an_image() {
        assert!(from_bytes(&[]).is_none());
    }

    #[test]
    fn rgba_keeps_the_pixel_size() {
        let rgba = [255, 0, 0, 255, 0, 255, 0, 128];
        let image = from_rgba(2, 1, &rgba).expect("rgba");
        assert_eq!(pixel_size(&image), (2, 1));
    }

    #[test]
    fn rgba_rejects_a_bad_buffer() {
        assert!(from_rgba(0, 1, &[0, 0, 0, 255]).is_none());
        assert!(from_rgba(1, 1, &[0, 0, 0]).is_none());
        assert!(from_rgba(2, 1, &[255, 0, 0, 255]).is_none());
    }
}
