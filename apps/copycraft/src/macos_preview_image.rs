#![cfg(target_os = "macos")]

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{AnyThread, ClassType};
use mac_ui::objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSCalibratedRGBColorSpace, NSImage, NSPasteboard,
};
use mac_ui::objc2_foundation::{NSArray, NSData, NSSize};

use crate::clipboard::ClipboardImage;

pub(crate) fn nsimage_from_bytes(bytes: &[u8]) -> Option<Retained<NSImage>> {
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    image.setTemplate(false);
    Some(image)
}

/// AppKit's image from whatever the pasteboard actually holds.
///
/// The card's own thumbnail decoder never sees some copies (a file promise, HEIC,
/// or PNG bytes ImageIO will not thumbnail). The well stays the empty fill, and
/// Original has no picture to return to. This reads the pasteboard the way Preview does.
pub(crate) fn nsimage_from_pasteboard(pasteboard: &NSPasteboard) -> Option<Retained<NSImage>> {
    let classes = NSArray::from_slice(&[NSImage::class()]);
    let read = unsafe { pasteboard.readObjectsForClasses_options(&classes, None) };
    if let Some(objects) = read
        && objects.count() > 0
    {
        let object = objects.objectAtIndex(0);
        if let Ok(image) = object.downcast::<NSImage>()
            && pixel_size(&image) != (0, 0)
        {
            image.setTemplate(false);
            return Some(image);
        }
    }
    let image = NSImage::initWithPasteboard(NSImage::alloc(), pasteboard)?;
    image.setTemplate(false);
    (pixel_size(&image) != (0, 0)).then_some(image)
}

pub(crate) fn pixel_size(image: &NSImage) -> (usize, usize) {
    let reps = image.representations();
    for index in 0..reps.count() {
        let rep = reps.objectAtIndex(index);
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

pub(crate) fn nsimage_from_clipboard(image: &ClipboardImage) -> Option<Retained<NSImage>> {
    // A raw bitmap with straight alpha draws black once the card is layer-backed.
    // PNG goes through AppKit's own decoder, which composites correctly.
    image
        .png_bytes()
        .ok()
        .and_then(|bytes| nsimage_from_bytes(&bytes))
        .or_else(|| nsimage_from_rgba(image))
}

fn nsimage_from_rgba(image: &ClipboardImage) -> Option<Retained<NSImage>> {
    let width = image.width as isize;
    let height = image.height as isize;
    let row = width.checked_mul(4)?;
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
    unsafe {
        let dest = rep.bitmapData();
        if dest.is_null() {
            return None;
        }
        dest.copy_from(image.rgba.as_ptr(), image.rgba.len());
    }
    let nsimage = NSImage::initWithSize(
        NSImage::alloc(),
        NSSize::new(image.width as f64, image.height as f64),
    );
    nsimage.addRepresentation(&rep);
    Some(nsimage)
}

#[cfg(test)]
mod tests {
    use image::ExtendedColorType;
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use mac_ui::objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use mac_ui::objc2_foundation::{NSData, NSString};

    use super::{nsimage_from_bytes, nsimage_from_pasteboard, pixel_size};

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
        let image = nsimage_from_bytes(&png()).expect("png");
        assert_eq!(pixel_size(&image), (2, 2));
    }

    #[test]
    fn pasteboard_png_is_an_nsimage() {
        let pasteboard =
            NSPasteboard::pasteboardWithName(&NSString::from_str("copycraft.image-preview.test"));
        pasteboard.clearContents();
        let data = NSData::with_bytes(&png());
        assert!(unsafe { pasteboard.setData_forType(Some(&data), NSPasteboardTypePNG) });
        let image = nsimage_from_pasteboard(&pasteboard).expect("pasteboard image");
        assert_eq!(pixel_size(&image), (2, 2));
        pasteboard.clearContents();
    }
}
