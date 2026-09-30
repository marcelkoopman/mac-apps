#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::path::Path;

use objc2::rc::Retained;
use objc2_foundation::{NSData, NSString, NSURL};

use crate::clipboard::ClipboardImage;
use crate::image_ops::PREVIEW_MAX_EDGE;

type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;

const RGBA_PREMUL: u32 = (4 << 12) | 1;
const CF_NUMBER_SINT64: i32 = 4;

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    static kCGImageSourceThumbnailMaxPixelSize: CFStringRef;
    static kCGImageSourceCreateThumbnailFromImageAlways: CFStringRef;
    static kCGImageSourceCreateThumbnailWithTransform: CFStringRef;
    static kCGImageSourceShouldCache: CFStringRef;
    static kCGImageSourceShouldCacheImmediately: CFStringRef;
    static kCGImagePropertyPixelWidth: CFStringRef;
    static kCGImagePropertyPixelHeight: CFStringRef;
    static kCGImagePropertyOrientation: CFStringRef;

    fn CGImageSourceCreateWithData(data: *const c_void, options: CFDictionaryRef) -> *mut c_void;
    fn CGImageSourceCreateWithURL(url: *const c_void, options: CFDictionaryRef) -> *mut c_void;
    fn CGImageSourceCopyPropertiesAtIndex(
        isrc: *mut c_void,
        index: usize,
        options: CFDictionaryRef,
    ) -> CFDictionaryRef;
    fn CGImageSourceCreateThumbnailAtIndex(
        isrc: *mut c_void,
        index: usize,
        options: CFDictionaryRef,
    ) -> *mut c_void;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(c: *mut c_void, rect: CGRect, image: *mut c_void);
    fn CGImageGetWidth(image: *mut c_void) -> usize;
    fn CGImageGetHeight(image: *mut c_void) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFBooleanFalse: *const c_void;
    fn CFRelease(cf: *const c_void);
    fn CFNumberCreate(
        allocator: *const c_void,
        the_type: i32,
        value: *const c_void,
    ) -> *const c_void;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: isize,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: *const c_void, the_type: i32, value: *mut c_void) -> u8;
    static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
}

#[repr(C)]
struct CFDictionaryKeyCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
}

#[repr(C)]
struct CFDictionaryValueCallBacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
}

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

struct Owned(*const c_void);

impl Owned {
    fn new(ptr: *const c_void) -> Option<Self> {
        // `then_some` builds its argument before the check. `Owned(null)`
        // would then drop through `CFRelease(NULL)`, which traps.
        if ptr.is_null() { None } else { Some(Self(ptr)) }
    }

    fn get(&self) -> *const c_void {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

pub(crate) fn preview_from_bytes(bytes: &[u8]) -> Option<ClipboardImage> {
    let data = NSData::with_bytes(bytes);
    let options = source_options();
    let options_ptr = options.as_ref().map(Owned::get).unwrap_or(std::ptr::null());
    let source =
        unsafe { CGImageSourceCreateWithData(Retained::as_ptr(&data).cast(), options_ptr) };
    preview_from_source(source)
}

pub(crate) fn preview_from_path(path: &Path) -> Option<ClipboardImage> {
    let text = path.to_str()?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let options = source_options();
    let options_ptr = options.as_ref().map(Owned::get).unwrap_or(std::ptr::null());
    let source = unsafe { CGImageSourceCreateWithURL(Retained::as_ptr(&url).cast(), options_ptr) };
    preview_from_source(source)
}

/// Keep ImageIO from decoding a large paste into a full bitmap just to read it.
fn source_options() -> Option<Owned> {
    let keys = unsafe {
        [
            kCGImageSourceShouldCache,
            kCGImageSourceShouldCacheImmediately,
        ]
    };
    let values = unsafe { [kCFBooleanFalse, kCFBooleanFalse] };
    let dict = unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    };
    Owned::new(dict)
}

fn preview_from_source(source: *mut c_void) -> Option<ClipboardImage> {
    let source = Owned::new(source.cast())?;
    let (full_width, full_height) = oriented_size(source.get());
    let options = thumbnail_options()?;
    let thumb =
        unsafe { CGImageSourceCreateThumbnailAtIndex(source.get().cast_mut(), 0, options.get()) };
    let thumb = Owned::new(thumb.cast())?;
    let width = unsafe { CGImageGetWidth(thumb.get().cast_mut()) };
    let height = unsafe { CGImageGetHeight(thumb.get().cast_mut()) };
    let mut rgba = rgba_from_image(thumb.get().cast_mut(), width, height)?;
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

fn thumbnail_options() -> Option<Owned> {
    let max_edge = i64::try_from(PREVIEW_MAX_EDGE).ok()?;
    let max_number = Owned::new(unsafe {
        CFNumberCreate(
            std::ptr::null(),
            CF_NUMBER_SINT64,
            (&max_edge) as *const i64 as *const c_void,
        )
    })?;
    let keys = unsafe {
        [
            kCGImageSourceThumbnailMaxPixelSize,
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
        ]
    };
    let values = [max_number.get(), unsafe { kCFBooleanTrue }, unsafe {
        kCFBooleanTrue
    }];
    let dict = unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            3,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    };
    Owned::new(dict)
}

fn oriented_size(source: *const c_void) -> (usize, usize) {
    let Some(props) = Owned::new(unsafe {
        CGImageSourceCopyPropertiesAtIndex(source.cast_mut(), 0, std::ptr::null())
    }) else {
        return (0, 0);
    };
    let mut width = dict_usize(props.get(), unsafe { kCGImagePropertyPixelWidth });
    let mut height = dict_usize(props.get(), unsafe { kCGImagePropertyPixelHeight });
    let orientation = dict_usize(props.get(), unsafe { kCGImagePropertyOrientation });
    if matches!(orientation, 5..=8) {
        std::mem::swap(&mut width, &mut height);
    }
    (width, height)
}

fn dict_usize(dict: *const c_void, key: *const c_void) -> usize {
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return 0;
    }
    let mut number = 0i64;
    let ok = unsafe {
        CFNumberGetValue(
            value,
            CF_NUMBER_SINT64,
            &mut number as *mut i64 as *mut c_void,
        )
    };
    if ok == 0 || number <= 0 {
        0
    } else {
        usize::try_from(number).unwrap_or(0)
    }
}

fn rgba_from_image(image: *mut c_void, width: usize, height: usize) -> Option<Vec<u8>> {
    let row = width.checked_mul(4)?;
    let len = row.checked_mul(height)?;
    if len == 0 {
        return None;
    }
    let mut rgba = vec![0u8; len];
    let space = Owned::new(unsafe { CGColorSpaceCreateDeviceRGB() })?;
    let context = Owned::new(unsafe {
        CGBitmapContextCreate(
            rgba.as_mut_ptr().cast(),
            width,
            height,
            8,
            row,
            space.get().cast_mut(),
            RGBA_PREMUL,
        )
    })?;
    unsafe {
        CGContextDrawImage(
            context.get().cast_mut(),
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: width as f64,
                    height: height as f64,
                },
            },
            image,
        );
    }
    drop(context);
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

    use super::{preview_from_bytes, preview_from_path};

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
}
