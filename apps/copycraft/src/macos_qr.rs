//! Core Image's `CIQRCodeGenerator` for [`crate::qr_code`]: the text's QR code at one pixel per
//! module, read back as pixels, then drawn large and written as PNG. On the device, offline;
//! the text and the picture are never logged. Typed through objc2-core-image, as the blur is
//! ([`mac_ui::blur`]).

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2_foundation::{NSData, NSObjectNSKeyValueCoding, NSString};
use objc2_core_graphics::CGImage;
use objc2_core_image::{CIContext, CIFilter};

use crate::macos_image_edit::{Encoding, encode, rgba_of};

/// The QR code of `text` (error correction level M) as PNG bytes, or `None` when Core Image
/// cannot make it (too long, or no generator).
pub fn png(text: &str) -> Option<Vec<u8>> {
    let image = generated(text)?;
    let pixels = rgba_of(&image)?;
    let dark: Vec<bool> = pixels
        .pixels
        .chunks(4)
        .map(|p| p[3] >= 128 && u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 384)
        .collect();
    let (modules, side) = crate::qr_code::modules(&dark, pixels.width, pixels.height)?;
    encode(&crate::qr_code::picture(&modules, side)?, Encoding::Png)
}

/// The generator's picture: one pixel per module.
fn generated(text: &str) -> Option<Retained<CGImage>> {
    let name = NSString::from_str("CIQRCodeGenerator");
    let message = NSData::with_bytes(text.as_bytes());
    let level = NSString::from_str("M");
    let message_key = NSString::from_str("inputMessage");
    let level_key = NSString::from_str("inputCorrectionLevel");
    // SAFETY: `filterWithName:` returns the generator or nil. Key-value coding sets its two inputs
    // with the types Core Image documents (an NSData message, an NSString level); its output is
    // nil when the message is too long. `contextWithOptions:` takes no options. `extent` is the
    // image's CGRect, and `createCGImage:fromRect:` returns an owned CGImage or nil.
    unsafe {
        let filter = CIFilter::filterWithName(&name)?;
        filter.setValue_forKey(Some(&message), &message_key);
        filter.setValue_forKey(Some(&level), &level_key);
        let image = filter.outputImage()?;
        let context = CIContext::contextWithOptions(None);
        let extent = image.extent();
        if !(extent.size.width >= 1.0 && extent.size.height >= 1.0 && extent.size.width <= 4096.0) {
            return None;
        }
        context.createCGImage_fromRect(&image, extent)
    }
}
