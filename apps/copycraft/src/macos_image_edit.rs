//! The ImageIO side of Image ▾ ([`crate::image_edit`]): pictures decoded upright (their EXIF
//! orientation applied) to straight RGBA, and versions written as PNG without metadata. Built on objc2-image-io / objc2-core-graphics (approved for this).

use std::ffi::c_void;

use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType,
    CGPoint, CGRect, CGSize, kCFAllocatorNull,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    kCGColorSpaceSRGB,
};
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImagePropertyOrientation, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceThumbnailMaxPixelSize,
};
use zeroize::Zeroizing;

use crate::image_edit::{Codec, Rgba};

/// kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big: R, G, B, A in memory.
const RGBA_PREMULTIPLIED: u32 = (4 << 12) | 1;
/// Larger pictures are not edited (a 4-byte-per-pixel copy of each would be several GB).
pub const MAX_PIXELS: usize = 100_000_000;

/// The file types a version can be written as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Encoding {
    Png,
}

impl Encoding {
    pub fn uti(self) -> &'static str {
        match self {
            Self::Png => "public.png",
        }
    }
}

/// ImageIO for [`crate::image_edit::Job`]s.
pub struct ImageIoCodec;

impl Codec for ImageIoCodec {
    fn decode(&self, bytes: &[u8]) -> Option<Rgba> {
        decode(bytes)
    }

    fn encode_png(&self, image: &Rgba) -> Option<Vec<u8>> {
        encode(image, Encoding::Png)
    }
}

/// `bytes` decoded upright at full size, as straight RGBA in sRGB.
pub fn decode(bytes: &[u8]) -> Option<Rgba> {
    let len = isize::try_from(bytes.len()).ok().filter(|len| *len > 0)?;
    // Read in place, so no copy of the picture is left in CoreFoundation's memory.
    // SAFETY: `bytes` outlives `data` and everything made from it, which stay in this function.
    let data = unsafe { CFData::with_bytes_no_copy(None, bytes.as_ptr(), len, kCFAllocatorNull) }?;
    let source = unsafe { CGImageSource::with_data(&data, None) }?;
    let (width, height) = upright_size(&source)?;
    if width.checked_mul(height)? > MAX_PIXELS {
        return None;
    }
    let longest = CFNumber::new_i64(i64::try_from(width.max(height)).ok()?);
    let yes = CFBoolean::new(true);
    let keys: [&CFString; 3] = unsafe {
        [
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceThumbnailMaxPixelSize,
        ]
    };
    let values: [&CFType; 3] = [yes, yes, &longest];
    let options = CFDictionary::from_slices(&keys, &values);
    let image = unsafe { source.thumbnail_at_index(0, Some(options.as_opaque())) }?;
    let (width, height) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    if width == 0 || height == 0 || width.checked_mul(height)? > MAX_PIXELS {
        return None;
    }
    let mut pixels = Zeroizing::new(vec![0u8; width.checked_mul(height)?.checked_mul(4)?]);
    {
        let context = bitmap(&mut pixels, width, height, RGBA_PREMULTIPLIED)?;
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(&context), rect, Some(&image));
    }
    unpremultiply(&mut pixels);
    let pixels = std::mem::take(&mut *pixels);
    Rgba::new(width, height, pixels)
}

/// The width and height of the first picture in `source`, its EXIF orientation applied.
fn upright_size(source: &CGImageSource) -> Option<(usize, usize)> {
    let properties = unsafe { source.properties_at_index(0, None) }?;
    // SAFETY: image properties have CFString keys.
    let properties: &CFDictionary<CFString, CFType> = unsafe { properties.cast_unchecked() };
    let number = |key: &CFString| -> Option<i64> {
        properties.get(key)?.downcast_ref::<CFNumber>()?.as_i64()
    };
    let width = usize::try_from(number(unsafe { kCGImagePropertyPixelWidth })?).ok()?;
    let height = usize::try_from(number(unsafe { kCGImagePropertyPixelHeight })?).ok()?;
    let orientation = number(unsafe { kCGImagePropertyOrientation }).unwrap_or(1);
    // 5 to 8 are a quarter turn (with or without a mirror image).
    Some(if (5..=8).contains(&orientation) {
        (height, width)
    } else {
        (width, height)
    })
    .filter(|(width, height)| *width > 0 && *height > 0)
}

/// `image` as `encoding`, without metadata.
pub fn encode(image: &Rgba, encoding: Encoding) -> Option<Vec<u8>> {
    let mut pixels = Zeroizing::new(image.pixels.to_vec());
    premultiply(&mut pixels);
    let info = RGBA_PREMULTIPLIED;
    let picture = {
        let context = bitmap(&mut pixels, image.width, image.height, info)?;
        CGBitmapContextCreateImage(Some(&context))?
    };
    let out = CFMutableData::new(None, 0)?;
    let uti = CFString::from_static_str(encoding.uti());
    let destination = unsafe { CGImageDestination::with_data(&out, &uti, 1, None) }?;
    let finished = unsafe {
        destination.add_image(&picture, None);
        destination.finalize()
    };
    drop(destination);
    drop(picture);
    let bytes = finished.then(|| out.to_vec());
    wipe(&out);
    bytes.filter(|bytes| !bytes.is_empty())
}

/// Overwrite the encoded picture in CoreFoundation's memory before it is released.
fn wipe(data: &CFMutableData) {
    let len = usize::try_from(data.length()).unwrap_or(0);
    let ptr = CFMutableData::mutable_byte_ptr(Some(data));
    if !ptr.is_null() && len > 0 {
        // SAFETY: `ptr` points at `len` bytes owned by `data`, which nothing else uses now.
        unsafe { std::ptr::write_bytes(ptr, 0, len) };
    }
}

/// A bitmap context drawing into `pixels` (sRGB, 8 bits per channel, 4 bytes per pixel).
fn bitmap(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    info: u32,
) -> Option<CFRetained<CGContext>> {
    if pixels.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: `pixels` holds `height` rows of `width * 4` bytes and outlives the context,
    // which callers drop before they read `pixels`.
    unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast::<c_void>(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            info,
        )
    }
}

fn premultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel[..3].fill(0);
        } else if alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
#[path = "macos_image_edit_tests.rs"]
mod tests;
