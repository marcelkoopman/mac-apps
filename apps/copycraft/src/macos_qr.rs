//! Core Image's `CIQRCodeGenerator` for [`crate::qr_code`]: the text's QR code at one pixel per
//! module, read back as pixels, then drawn large and written as PNG. On the device, offline;
//! the text and the picture are never logged. Core Image has no binding among CopyCraft's
//! crates (objc2-core-image would be a new one), so it is reached by name, as the blur is
//! ([`mac_ui::blur`]).

use mac_ui::objc2::msg_send;
use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::{AnyClass, AnyObject};
use mac_ui::objc2_foundation::{NSData, NSRect, NSString};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGImage;

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
fn generated(text: &str) -> Option<CFRetained<CGImage>> {
    let filter_class = AnyClass::get(c"CIFilter")?;
    let context_class = AnyClass::get(c"CIContext")?;
    let name = NSString::from_str("CIQRCodeGenerator");
    let message = NSData::with_bytes(text.as_bytes());
    let level = NSString::from_str("M");
    let message_key = NSString::from_str("inputMessage");
    let level_key = NSString::from_str("inputCorrectionLevel");
    let output_key = NSString::from_str("outputImage");
    // SAFETY: `+[CIFilter filterWithName:]` takes an NSString and returns an autoreleased
    // CIFilter or nil. Key-value coding sets the generator's two inputs (an NSData message and
    // an NSString level, as Core Image documents them) and reads its CIImage output, nil when
    // the message is too long. `+[CIContext contextWithOptions:]` takes a nil dictionary.
    // `-extent` returns a CGRect, and `-createCGImage:fromRect:` a +1 CGImage (released by
    // `CFRetained`) or NULL.
    unsafe {
        let filter: *mut AnyObject = msg_send![filter_class, filterWithName: &*name];
        let filter = Retained::retain_autoreleased(filter)?;
        let _: () = msg_send![&*filter, setValue: &*message, forKey: &*message_key];
        let _: () = msg_send![&*filter, setValue: &*level, forKey: &*level_key];
        let image: *mut AnyObject = msg_send![&*filter, valueForKey: &*output_key];
        let image = Retained::retain_autoreleased(image)?;
        let none: *const AnyObject = std::ptr::null();
        let context: *mut AnyObject = msg_send![context_class, contextWithOptions: none];
        let context = Retained::retain_autoreleased(context)?;
        let extent: NSRect = msg_send![&*image, extent];
        if !(extent.size.width >= 1.0 && extent.size.height >= 1.0 && extent.size.width <= 4096.0) {
            return None;
        }
        let picture: *mut CGImage = msg_send![&*context, createCGImage: &*image, fromRect: extent];
        std::ptr::NonNull::new(picture).map(|raw| CFRetained::from_raw(raw))
    }
}
