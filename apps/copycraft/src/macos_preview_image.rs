#![cfg(target_os = "macos")]

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::{AnyThread, ClassType};
use mac_ui::objc2_app_kit::{NSImage, NSPasteboard};
use mac_ui::objc2_foundation::NSArray;

use crate::clipboard::ClipboardImage;

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
            && mac_ui::image::pixel_size(&image) != (0, 0)
        {
            image.setTemplate(false);
            return Some(image);
        }
    }
    let image = NSImage::initWithPasteboard(NSImage::alloc(), pasteboard)?;
    image.setTemplate(false);
    (mac_ui::image::pixel_size(&image) != (0, 0)).then_some(image)
}

pub(crate) fn nsimage_from_clipboard(image: &ClipboardImage) -> Option<Retained<NSImage>> {
    // A raw bitmap with straight alpha draws black once the card is layer-backed.
    // PNG goes through AppKit's own decoder, which composites correctly.
    image
        .png_bytes()
        .ok()
        .and_then(|bytes| mac_ui::image::from_bytes(&bytes))
        .or_else(|| mac_ui::image::from_rgba(image.width, image.height, &image.rgba))
}

#[cfg(test)]
mod tests {
    use image::ExtendedColorType;
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use mac_ui::objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use mac_ui::objc2_foundation::{NSData, NSString};

    use super::nsimage_from_pasteboard;

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
    fn pasteboard_png_is_an_nsimage() {
        let pasteboard =
            NSPasteboard::pasteboardWithName(&NSString::from_str("copycraft.image-preview.test"));
        pasteboard.clearContents();
        let data = NSData::with_bytes(&png());
        assert!(unsafe { pasteboard.setData_forType(Some(&data), NSPasteboardTypePNG) });
        let image = nsimage_from_pasteboard(&pasteboard).expect("pasteboard image");
        assert_eq!(mac_ui::image::pixel_size(&image), (2, 2));
        pasteboard.clearContents();
    }
}
