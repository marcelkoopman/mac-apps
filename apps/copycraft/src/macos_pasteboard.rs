#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine;
use mac_ui::objc2::AnyThread;
use mac_ui::objc2::rc::{Retained, autoreleasepool};
use mac_ui::objc2_app_kit::{
    NSImage, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF,
};
use mac_ui::objc2_foundation::{NSArray, NSData, NSString, NSURL};

use zeroize::{Zeroize, Zeroizing};

use crate::clipboard::{ClipboardImage, ClipboardView};
use crate::commands::{ImageFacts, ImageScan};
use crate::image_ops;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "webp", "heic",
];

struct CachedView {
    change_count: isize,
    view: ClipboardView,
    image_path: Option<PathBuf>,
}

static CACHE: Mutex<Option<CachedView>> = Mutex::new(None);

#[derive(Clone)]
struct CardSnap {
    change_count: isize,
    facts: ImageFacts,
    /// Present when ImageIO produced a thumbnail. The card can still show the
    /// picture from AppKit when this is missing.
    thumbnail: Option<ClipboardImage>,
}

static CARD: Mutex<Option<CardSnap>> = Mutex::new(None);

pub(crate) struct DecodedPreview {
    pub image: ClipboardImage,
    pub source_png: Option<Vec<u8>>,
}

pub(crate) fn current_view() -> ClipboardView {
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    let cached = CACHE.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .and_then(|cached| (cached.change_count == change_count).then(|| cached.view.clone()))
    });
    // A file promise can land on the pasteboard before its picture does, without
    // a new change count. The first look is "no text" and would stick otherwise.
    if let Some(view) = cached
        && remember_cached_view(&view, pasteboard_has_image(&pasteboard))
    {
        return view;
    }
    let (view, image_path) = read_view(&pasteboard);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CachedView {
            change_count,
            view: view.clone(),
            image_path,
        });
    }
    view
}

fn cached_image_path(change_count: isize) -> Option<PathBuf> {
    let guard = CACHE.lock().ok()?;
    let cached = guard.as_ref()?;
    if cached.change_count == change_count {
        cached.image_path.clone()
    } else {
        None
    }
}

pub(crate) fn change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

/// Info text and any text or barcode hidden in the current image.
pub(crate) fn scan_card_image() -> Option<ImageScan> {
    let decoded = decode_preview()?;
    let image = decoded.image;
    let source_png = decoded.source_png;
    let full_res = image.width == image.full_width && image.height == image.full_height;
    let encoded;
    let scan: Option<&[u8]> = if let Some(bytes) = source_png.as_deref() {
        Some(bytes)
    } else {
        encoded = image.png_bytes().ok();
        encoded.as_deref()
    };
    let jpeg_len = if full_res {
        crate::image_ops::jpeg_bytes(&image, crate::image_ops::JPEG_QUALITY)
            .ok()
            .map(|bytes| bytes.len())
    } else {
        None
    };
    let png_len = source_png
        .as_ref()
        .map(Vec::len)
        .or_else(|| scan.map(<[u8]>::len));
    let info = crate::image_ops::info_with_sizes(&image, png_len, jpeg_len);
    let data_url = image_data_url();
    let (ocr, qr) = scan
        .map(crate::macos_vision::scan_png)
        .unwrap_or((None, None));
    Some(ImageScan {
        info,
        data_url,
        ocr,
        qr,
    })
}

pub(crate) fn current_image_bytes() -> Option<Vec<u8>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    let cached_path = cached_image_path(change_count);
    if let Some(bytes) = autoreleasepool(|_| {
        image_source(&pasteboard, cached_path.as_deref()).and_then(|source| match source {
            ImageSource::Bytes { bytes, .. } => Some(bytes),
            ImageSource::File(path) => std::fs::read(path).ok(),
        })
    }) {
        return Some(bytes);
    }
    // A picture AppKit can draw still has to land in history, or `<` cannot
    // step back to it once the next copy replaces the pasteboard.
    let image = crate::macos_preview_image::nsimage_from_pasteboard(&pasteboard)?;
    let data = image.TIFFRepresentation()?;
    let bytes = data.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

pub(crate) fn decode_preview() -> Option<DecodedPreview> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    // Copy the encoded bytes out of the pasteboard before decoding so a large
    // image is not decoded while AppKit is still holding the original buffer.
    let cached_path = cached_image_path(change_count);
    let source = autoreleasepool(|_| image_source(&pasteboard, cached_path.as_deref()))?;
    let decoded = match source {
        ImageSource::Bytes { bytes, is_png } => decode_bytes(bytes, is_png)?,
        ImageSource::File(path) => match read_image_file(&path) {
            Some(decoded) => decoded,
            None => {
                let bytes = autoreleasepool(|_| {
                    pasteboard_data(&pasteboard, unsafe { NSPasteboardTypeTIFF })
                })?;
                decode_bytes(bytes, false)?
            }
        },
    };
    Some(decoded)
}

fn read_image_file(path: &std::path::Path) -> Option<DecodedPreview> {
    let is_png = match infer::get_from_path(path) {
        Ok(Some(kind)) => kind.extension() == "png",
        Ok(None) | Err(_) => path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png")),
    };
    if is_png {
        let bytes = std::fs::read(path).ok()?;
        return decode_bytes(bytes, true);
    }
    if let Some(image) = crate::macos_image_io::preview_from_path(path) {
        return Some(DecodedPreview {
            image,
            source_png: None,
        });
    }
    let bytes = std::fs::read(path).ok()?;
    decode_bytes(bytes, false)
}

pub(crate) fn write_history_image(bytes: &[u8]) -> Result<(), String> {
    if infer::image::is_png(bytes) {
        return write_png(bytes);
    }
    let extension = detect_bytes(bytes)
        .map(|kind| kind.extension)
        .unwrap_or("tiff");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("copycraft-history-{nanos}.{extension}"));
    std::fs::write(&path, bytes).map_err(|err| err.to_string())?;
    set_file_url(&path)
}

pub(crate) fn write_png(bytes: &[u8]) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let data = NSData::with_bytes(bytes);
    let ok = unsafe {
        pasteboard.clearContents();
        pasteboard.setData_forType(Some(&data), NSPasteboardTypePNG)
    };
    invalidate_caches();
    if ok {
        Ok(())
    } else {
        Err("could not write image".into())
    }
}

pub(crate) fn image_facts() -> Option<ImageFacts> {
    card_snap().map(|snap| snap.facts)
}

pub(crate) fn image_thumbnail() -> Option<ClipboardImage> {
    card_snap().and_then(|snap| snap.thumbnail)
}

/// Picture for the card well. Prefers AppKit's pasteboard image over the thumbnail,
/// which is blank when that decoder returns nothing.
pub(crate) fn clipboard_picture() -> Option<Retained<NSImage>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    if let Some(image) = crate::macos_preview_image::nsimage_from_pasteboard(&pasteboard) {
        return Some(image);
    }
    let thumb = image_thumbnail()?;
    crate::macos_preview_image::nsimage_from_clipboard(&thumb)
}

pub(crate) fn image_data_url() -> Option<String> {
    let exported = export_image()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&exported.bytes);
    Some(format!("data:{};base64,{encoded}", exported.mime))
}

fn card_snap() -> Option<CardSnap> {
    let change_count = change_count();
    if let Some(cached) = cached_card(change_count) {
        return Some(cached);
    }
    let built = build_card(change_count)?;
    if let Ok(mut guard) = CARD.lock() {
        *guard = Some(built.clone());
    }
    Some(built)
}

fn cached_card(change_count: isize) -> Option<CardSnap> {
    let guard = CARD.lock().ok()?;
    let cached = guard.as_ref()?;
    (cached.change_count == change_count).then(|| cached.clone())
}

fn build_card(change_count: isize) -> Option<CardSnap> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let cached_path = cached_image_path(change_count);
    let source = autoreleasepool(|_| image_source(&pasteboard, cached_path.as_deref()));
    if let Some(source) = source
        && let Some(snap) = snap_from_source(change_count, source)
    {
        return Some(snap);
    }
    // Classified as an image, but ImageIO never produced a thumbnail. Size still
    // comes from the picture AppKit can draw, so the meta line is not blank.
    let image = crate::macos_preview_image::nsimage_from_pasteboard(&pasteboard)?;
    snap_from_nsimage(change_count, &image, "Image", 0)
}

fn snap_from_source(change_count: isize, source: ImageSource) -> Option<CardSnap> {
    match source {
        ImageSource::Bytes { bytes, is_png } => {
            let label = detect_image_bytes(&bytes, is_png).label;
            let byte_len = bytes.len();
            if let Some(thumbnail) = crate::macos_image_io::preview_from_bytes(&bytes)
                .or_else(|| image_ops::preview_from_encoded(&bytes))
            {
                return Some(snap_with_thumbnail(
                    change_count,
                    label,
                    byte_len,
                    thumbnail,
                ));
            }
            let image = mac_ui::image::from_bytes(&bytes)?;
            snap_from_nsimage(change_count, &image, label, byte_len)
        }
        ImageSource::File(path) => {
            let byte_len = std::fs::metadata(&path)
                .ok()
                .and_then(|meta| usize::try_from(meta.len()).ok())
                .unwrap_or(0);
            let label = detect_path(&path).label;
            if let Some(thumbnail) = crate::macos_image_io::preview_from_path(&path).or_else(|| {
                let bytes = std::fs::read(&path).ok()?;
                image_ops::preview_from_encoded(&bytes)
            }) {
                return Some(snap_with_thumbnail(
                    change_count,
                    label,
                    byte_len,
                    thumbnail,
                ));
            }
            let text = path.to_str()?;
            let image =
                NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(text))?;
            snap_from_nsimage(change_count, &image, label, byte_len)
        }
    }
}

fn snap_with_thumbnail(
    change_count: isize,
    label: &str,
    byte_len: usize,
    thumbnail: ClipboardImage,
) -> CardSnap {
    CardSnap {
        change_count,
        facts: ImageFacts {
            format: label.to_string(),
            width: thumbnail.full_width,
            height: thumbnail.full_height,
            byte_len,
        },
        thumbnail: Some(thumbnail),
    }
}

fn snap_from_nsimage(
    change_count: isize,
    image: &NSImage,
    label: &str,
    byte_len: usize,
) -> Option<CardSnap> {
    let (width, height) = mac_ui::image::pixel_size(image);
    if width == 0 || height == 0 {
        return None;
    }
    Some(CardSnap {
        change_count,
        facts: ImageFacts {
            format: label.to_string(),
            width,
            height,
            byte_len,
        },
        thumbnail: None,
    })
}

struct ImageExport {
    bytes: Vec<u8>,
    mime: &'static str,
}

fn export_image() -> Option<ImageExport> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    let cached_path = cached_image_path(change_count);
    let source = autoreleasepool(|_| image_source(&pasteboard, cached_path.as_deref()))?;
    match source {
        ImageSource::Bytes { bytes, is_png } => {
            let detected = detect_image_bytes(&bytes, is_png);
            Some(ImageExport {
                bytes,
                mime: detected.mime,
            })
        }
        ImageSource::File(path) => {
            let bytes = std::fs::read(&path).ok()?;
            let detected = detect_bytes(&bytes).unwrap_or_else(|| extension_kind(&path));
            Some(ImageExport {
                bytes,
                mime: detected.mime,
            })
        }
    }
}

fn set_file_url(path: &std::path::Path) -> Result<(), String> {
    let text = path.to_str().ok_or_else(|| "path".to_string())?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let absolute = url
        .absoluteString()
        .ok_or_else(|| "url".to_string())?
        .to_string();
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let ok = pasteboard.setString_forType(&NSString::from_str(&absolute), unsafe {
        NSPasteboardTypeFileURL
    });
    invalidate_caches();
    if ok {
        Ok(())
    } else {
        Err("could not copy file".into())
    }
}

pub(crate) fn zeroize_caches() {
    let mut cache = CACHE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(cached) = cache.as_mut()
        && let ClipboardView::Text(text) = &mut cached.view
    {
        text.zeroize();
    }
    *cache = None;
    drop(cache);

    let mut card = CARD.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(snap) = card.as_mut()
        && let Some(image) = snap.thumbnail.as_mut()
    {
        image.rgba.zeroize();
    }
    *card = None;
}

fn invalidate_caches() {
    zeroize_caches();
}

struct ContentKind {
    label: &'static str,
    mime: &'static str,
    extension: &'static str,
}

fn detect_bytes(bytes: &[u8]) -> Option<ContentKind> {
    infer::get(bytes)
        .filter(|kind| kind.matcher_type() == infer::MatcherType::Image)
        .map(content_kind)
}

fn detect_image_bytes(bytes: &[u8], hint_png: bool) -> ContentKind {
    if let Some(kind) = detect_bytes(bytes) {
        return kind;
    }
    if hint_png {
        ContentKind {
            label: "PNG",
            mime: "image/png",
            extension: "png",
        }
    } else {
        ContentKind {
            label: "TIFF",
            mime: "image/tiff",
            extension: "tiff",
        }
    }
}

fn detect_path(path: &std::path::Path) -> ContentKind {
    infer::get_from_path(path)
        .ok()
        .flatten()
        .filter(|kind| kind.matcher_type() == infer::MatcherType::Image)
        .map(content_kind)
        .unwrap_or_else(|| extension_kind(path))
}

fn content_kind(kind: infer::Type) -> ContentKind {
    let ext = kind.extension();
    let mime = kind.mime_type();
    let (label, mime, extension) = match ext {
        "jpg" => ("JPEG", mime, "jpg"),
        "tif" => ("TIFF", mime, "tiff"),
        // infer reports HEIC photos as image/heif. The pasteboard type is HEIC.
        "heif" => ("HEIC", "image/heic", "heic"),
        "jp2" => ("JPEG 2000", mime, "jp2"),
        "png" => ("PNG", mime, "png"),
        "gif" => ("GIF", mime, "gif"),
        "webp" => ("WEBP", mime, "webp"),
        "bmp" => ("BMP", mime, "bmp"),
        "avif" => ("AVIF", mime, "avif"),
        "ico" => ("ICO", mime, "ico"),
        "psd" => ("PSD", mime, "psd"),
        "jxl" => ("JXL", mime, "jxl"),
        "cr2" => ("CR2", mime, "cr2"),
        _ => ("Image", mime, ext),
    };
    ContentKind {
        label,
        mime,
        extension,
    }
}

fn extension_kind(path: &std::path::Path) -> ContentKind {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => ContentKind {
            label: "PNG",
            mime: "image/png",
            extension: "png",
        },
        "jpg" | "jpeg" => ContentKind {
            label: "JPEG",
            mime: "image/jpeg",
            extension: "jpg",
        },
        "gif" => ContentKind {
            label: "GIF",
            mime: "image/gif",
            extension: "gif",
        },
        "webp" => ContentKind {
            label: "WEBP",
            mime: "image/webp",
            extension: "webp",
        },
        "heic" | "heif" => ContentKind {
            label: "HEIC",
            mime: "image/heic",
            extension: "heic",
        },
        "bmp" => ContentKind {
            label: "BMP",
            mime: "image/bmp",
            extension: "bmp",
        },
        "tif" | "tiff" => ContentKind {
            label: "TIFF",
            mime: "image/tiff",
            extension: "tiff",
        },
        _ => ContentKind {
            label: "Image",
            mime: "image/png",
            extension: "png",
        },
    }
}

fn read_view(pasteboard: &NSPasteboard) -> (ClipboardView, Option<PathBuf>) {
    autoreleasepool(|_| {
        let text = pasteboard_string(pasteboard, unsafe { NSPasteboardTypeString });
        // Copying a PNG file also puts its name on the pasteboard. That name is
        // the file, so it still counts as an image. Other text stays text.
        let copied_file = image_file(pasteboard);
        match text {
            Some(text)
                if !text.trim().is_empty()
                    && !names_copied_image(text.trim(), copied_file.as_deref()) =>
            {
                (ClipboardView::Text(Zeroizing::new(text)), None)
            }
            other => {
                if copied_file.is_some()
                    || has_image_data(pasteboard)
                    || NSImage::canInitWithPasteboard(pasteboard)
                {
                    (ClipboardView::Image, copied_file)
                } else if other.is_some() {
                    (ClipboardView::Empty, None)
                } else {
                    (ClipboardView::NoText, None)
                }
            }
        }
    })
}

fn names_copied_image(text: &str, path: Option<&std::path::Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let full = path.to_string_lossy();
    if text == full {
        return true;
    }
    if path.file_name().and_then(|name| name.to_str()) == Some(text) {
        return true;
    }
    text.strip_prefix("file://") == Some(full.as_ref())
}

/// A cached empty reading stays only while the pasteboard still has no picture.
fn remember_cached_view(view: &ClipboardView, image_available: bool) -> bool {
    match view {
        ClipboardView::Empty | ClipboardView::NoText => !image_available,
        ClipboardView::Text(_) | ClipboardView::Image => true,
    }
}

fn pasteboard_has_image(pasteboard: &NSPasteboard) -> bool {
    image_file(pasteboard).is_some()
        || has_image_data(pasteboard)
        || NSImage::canInitWithPasteboard(pasteboard)
}

fn has_image_data(pasteboard: &NSPasteboard) -> bool {
    let jpeg = NSString::from_str("public.jpeg");
    let heic = NSString::from_str("public.heic");
    let types = unsafe {
        NSArray::from_slice(&[NSPasteboardTypePNG, NSPasteboardTypeTIFF, &*jpeg, &*heic])
    };
    pasteboard.availableTypeFromArray(&types).is_some()
}

enum ImageSource {
    Bytes { bytes: Vec<u8>, is_png: bool },
    File(PathBuf),
}

const EXTRA_IMAGE_TYPES: &[&str] = &[
    "public.heic",
    "public.heif",
    "public.gif",
    "public.webp",
    "org.webmproject.webp",
    "com.microsoft.bmp",
    "public.avif",
];

fn image_source(
    pasteboard: &NSPasteboard,
    cached_path: Option<&std::path::Path>,
) -> Option<ImageSource> {
    // A declared PNG that is not image bytes used to win over the file and over
    // TIFF, and the card then had neither a thumbnail nor a size line.
    if let Some(bytes) = take_encoded(pasteboard, unsafe { NSPasteboardTypePNG }) {
        return Some(ImageSource::Bytes {
            bytes,
            is_png: true,
        });
    }
    let jpeg = NSString::from_str("public.jpeg");
    if let Some(bytes) = take_encoded(pasteboard, &jpeg) {
        return Some(ImageSource::Bytes {
            bytes,
            is_png: false,
        });
    }
    if let Some(path) = cached_path {
        return Some(ImageSource::File(path.to_path_buf()));
    }
    if let Some(path) = image_file(pasteboard) {
        return Some(ImageSource::File(path));
    }
    if let Some(bytes) = take_encoded(pasteboard, unsafe { NSPasteboardTypeTIFF }) {
        return Some(ImageSource::Bytes {
            bytes,
            is_png: false,
        });
    }
    for name in EXTRA_IMAGE_TYPES {
        let kind = NSString::from_str(name);
        if let Some(bytes) = take_encoded(pasteboard, &kind) {
            return Some(ImageSource::Bytes {
                bytes,
                is_png: false,
            });
        }
    }
    None
}

fn take_encoded(pasteboard: &NSPasteboard, kind: &NSString) -> Option<Vec<u8>> {
    let bytes = pasteboard_data(pasteboard, kind)?;
    looks_like_encoded_image(&bytes).then_some(bytes)
}

fn looks_like_encoded_image(bytes: &[u8]) -> bool {
    if infer::get(bytes).is_some_and(|kind| kind.matcher_type() == infer::MatcherType::Image) {
        return true;
    }
    bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"GIF8")
        || bytes.starts_with(b"II*\x00")
        || bytes.starts_with(b"MM\x00*")
        || (bytes.len() > 12 && &bytes[4..8] == b"ftyp")
}

fn decode_bytes(bytes: Vec<u8>, is_png: bool) -> Option<DecodedPreview> {
    let image = crate::macos_image_io::preview_from_bytes(&bytes)
        .or_else(|| image_ops::preview_from_encoded(&bytes))?;
    let source_png = is_png.then_some(bytes);
    Some(DecodedPreview { image, source_png })
}

fn pasteboard_string(pasteboard: &NSPasteboard, kind: &NSString) -> Option<String> {
    let text = pasteboard.stringForType(kind)?;
    Some(text.to_string())
}

fn pasteboard_data(pasteboard: &NSPasteboard, kind: &NSString) -> Option<Vec<u8>> {
    let data = pasteboard.dataForType(kind)?;
    let bytes = data.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

fn image_file(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    if type_available(pasteboard, unsafe { NSPasteboardTypeFileURL })
        && let Some(path) = path_from_file_url(pasteboard).filter(|path| is_image_file(path))
    {
        return Some(path);
    }
    let filenames = NSString::from_str("NSFilenamesPboardType");
    if type_available(pasteboard, &filenames) {
        return filenames_image(pasteboard);
    }
    None
}

fn type_available(pasteboard: &NSPasteboard, kind: &NSString) -> bool {
    let types = NSArray::from_slice(&[kind]);
    pasteboard.availableTypeFromArray(&types).is_some()
}

fn path_from_file_url(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let url = pasteboard_string(pasteboard, unsafe { NSPasteboardTypeFileURL })?;
    let url = NSString::from_str(&url);
    let path = NSURL::URLWithString(&url)?.path()?.to_string();
    Some(PathBuf::from(path))
}

fn filenames_image(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let kind = NSString::from_str("NSFilenamesPboardType");
    let list = pasteboard.propertyListForType(&kind)?;
    let array = list.downcast::<NSArray>().ok()?;
    if array.count() != 1 {
        return None;
    }
    let path = PathBuf::from(
        array
            .objectAtIndex(0)
            .downcast::<NSString>()
            .ok()?
            .to_string(),
    );
    is_image_file(&path).then_some(path)
}

fn is_image_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    match infer::get_from_path(path) {
        Ok(Some(kind)) => kind.matcher_type() == infer::MatcherType::Image,
        Ok(None) | Err(_) => has_image_extension(path),
    }
}

fn has_image_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTS
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        detect_image_bytes, extension_kind, is_image_file, names_copied_image, remember_cached_view,
    };
    use crate::clipboard::ClipboardView;
    use zeroize::Zeroizing;

    struct RemoveOnDrop(PathBuf);

    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn write_temp(name: &str, bytes: &[u8]) -> RemoveOnDrop {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, bytes).unwrap();
        RemoveOnDrop(path)
    }

    #[test]
    fn no_text_cache_drops_when_an_image_arrives() {
        assert!(!remember_cached_view(&ClipboardView::NoText, true));
        assert!(!remember_cached_view(&ClipboardView::Empty, true));
        assert!(remember_cached_view(&ClipboardView::NoText, false));
        assert!(remember_cached_view(&ClipboardView::Image, false));
        assert!(remember_cached_view(
            &ClipboardView::Text(Zeroizing::new("notes".to_string())),
            true
        ));
    }

    #[test]
    fn copied_png_filename_is_the_image() {
        let path = Path::new("/tmp/ed38402972604e86f4303a04442655a5171d7860.png");
        assert!(names_copied_image(
            "ed38402972604e86f4303a04442655a5171d7860.png",
            Some(path)
        ));
        assert!(names_copied_image(
            "/tmp/ed38402972604e86f4303a04442655a5171d7860.png",
            Some(path)
        ));
        assert!(names_copied_image(
            "file:///tmp/ed38402972604e86f4303a04442655a5171d7860.png",
            Some(path)
        ));
    }

    #[test]
    fn infer_names_png_jpeg_webp_and_heic() {
        let png = detect_image_bytes(b"\x89PNG\r\n", false);
        assert_eq!(png.label, "PNG");
        assert_eq!(png.mime, "image/png");
        let jpeg = detect_image_bytes(&[0xFF, 0xD8, 0xFF, 0], false);
        assert_eq!(jpeg.label, "JPEG");
        assert_eq!(jpeg.mime, "image/jpeg");
        assert_eq!(jpeg.extension, "jpg");
        let mut webp = [0_u8; 12];
        webp[8..12].copy_from_slice(b"WEBP");
        assert_eq!(detect_image_bytes(&webp, false).label, "WEBP");
        let heic = b"\x00\x00\x00\x10ftypheic\x00\x00\x00\x00";
        let heic = detect_image_bytes(heic, false);
        assert_eq!(heic.label, "HEIC");
        assert_eq!(heic.mime, "image/heic");
        assert_eq!(heic.extension, "heic");
        assert_eq!(extension_kind(Path::new("/tmp/shot.heic")).label, "HEIC");
        assert_eq!(detect_image_bytes(b"not-an-image", true).label, "PNG");
    }

    #[test]
    fn infer_reads_the_file_not_only_its_name() {
        let png = write_temp("copycraft-infer-png.bin", b"\x89PNG\r\n\x1a\n");
        assert!(is_image_file(&png.0));
        let pdf = write_temp("copycraft-infer-pdf.png", b"%PDF-1.4\n");
        assert!(!is_image_file(&pdf.0));
    }

    #[test]
    fn document_text_stays_text_when_an_image_file_is_also_present() {
        let path = Path::new("/tmp/shot.png");
        assert!(!names_copied_image("hello", Some(path)));
        assert!(!names_copied_image("shot.png\nmore", Some(path)));
        assert!(!names_copied_image("shot.png", None));
    }
}
