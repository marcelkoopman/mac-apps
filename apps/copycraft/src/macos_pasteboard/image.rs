use super::*;

/// The current picture as the pasteboard holds it, fetched on the main thread for a scan that
/// decodes on a background thread ([`scan_card_input`]). Pasteboard reads stay on the main
/// thread: with pasteboard privacy a read can show a modal alert, which must not come from a
/// background thread.
pub(crate) struct CardImageInput {
    pub(super) source: ImageSource,
    /// TIFF from the pasteboard for a copied file CopyCraft cannot open (sandbox), to decode
    /// instead.
    pub(super) tiff: Option<Vec<u8>>,
}

/// Fetch the current picture for a scan. Main thread only.
pub(crate) fn card_image_input() -> Option<CardImageInput> {
    if foreign_blocked() {
        return None;
    }
    if let Some(shared) = shared_image() {
        return shared.map(|grab| CardImageInput {
            source: grab.source,
            tiff: grab.tiff,
        });
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    let cached_path = cached_image_path(change_count);
    let source = autoreleasepool(|_| image_source(&pasteboard, cached_path.as_deref()))?;
    let tiff = match &source {
        ImageSource::File(path) if std::fs::File::open(path).is_err() => {
            autoreleasepool(|_| pasteboard_data(&pasteboard, mac_ui::pasteboard_types::tiff()))
        }
        _ => None,
    };
    Some(CardImageInput { source, tiff })
}

/// Info text and any text or barcode hidden in the picture fetched by [`card_image_input`].
/// Reads no pasteboard: runs on a background thread.
pub(crate) fn scan_card_input(input: CardImageInput) -> Option<ImageScan> {
    let data_url = export_from_source(&input.source).map(|exported| exported.data_url());
    scan_decoded(decode_source(input.source, input.tiff)?, data_url)
}

/// Info text and any text or barcode hidden in an encoded picture that is not the clipboard's
/// (a dropped image).
pub(crate) fn scan_image_bytes(bytes: &[u8]) -> Option<ImageScan> {
    let is_png = infer::image::is_png(bytes);
    let mime = detect_image_bytes(bytes, is_png).mime;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let data_url = Some(format!("data:{mime};base64,{encoded}"));
    scan_decoded(decode_bytes(bytes.to_vec(), is_png)?, data_url)
}

/// How long a picture's text recognition may take before it goes into history without it.
pub(crate) const RECOGNITION_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// [`scan_image_bytes`] on a thread of its own, at most [`RECOGNITION_LIMIT`]: `None` when it
/// took longer (its late result is wiped) or could not start.
pub(crate) fn scan_image_bytes_within(bytes: Zeroizing<Vec<u8>>) -> Option<ImageScan> {
    crate::work::within(
        "copycraft-image-scan",
        RECOGNITION_LIMIT,
        move || scan_image_bytes(&bytes),
        |scan| {
            if let Some(mut scan) = scan {
                scan.wipe();
            }
        },
    )
    .flatten()
}

/// Only the text recognised in an encoded picture, for the history gate of a copied picture
/// ([`crate::sensitivity::picture_admission`]), at most [`RECOGNITION_LIMIT`]. `None` when the
/// picture holds no text, cannot be read, recognition fails or takes too long. No data URL or
/// size info is made; a barcode's text is wiped. Never logged.
pub(crate) fn recognized_text_within(bytes: Zeroizing<Vec<u8>>) -> Option<Zeroizing<String>> {
    crate::work::within(
        "copycraft-image-text",
        RECOGNITION_LIMIT,
        move || {
            let decoded = decode_bytes(bytes.to_vec(), infer::image::is_png(&bytes))?;
            let png = match decoded.source_png {
                Some(png) => Zeroizing::new(png),
                None => Zeroizing::new(decoded.image.png_bytes().ok()?),
            };
            let (ocr, qr) = crate::macos_vision::scan_png(&png);
            if let Some(mut qr) = qr {
                qr.zeroize();
            }
            ocr.map(Zeroizing::new)
        },
        drop,
    )
    .flatten()
}

/// Format, pixel size and byte length of an encoded picture that is not the clipboard's, or
/// `None` when it can't be read.
pub(crate) fn image_bytes_facts(bytes: &[u8]) -> Option<ImageFacts> {
    let source = ImageSource::Bytes {
        bytes: bytes.to_vec(),
        is_png: infer::image::is_png(bytes),
    };
    snap_from_source(0, source).map(|snap| snap.facts)
}

pub(super) fn scan_decoded(decoded: DecodedPreview, data_url: Option<String>) -> Option<ImageScan> {
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
    if foreign_blocked() {
        return None;
    }
    if let Some(shared) = shared_image() {
        let grab = shared?;
        return match grab.source {
            ImageSource::Bytes { bytes, .. } => Some(bytes),
            ImageSource::File(path) => std::fs::read(path).ok().or(grab.tiff),
        };
    }
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

/// The current picture for history, fetched on the main thread like [`card_image_input`]
/// (with the same TIFF fallback as [`current_image_bytes`] for a picture only AppKit can
/// draw). [`image_input_bytes`] turns it into bytes on a background thread. Main thread only.
pub(crate) fn history_image_input() -> Option<CardImageInput> {
    if foreign_blocked() {
        return None;
    }
    if let Some(shared) = shared_image() {
        return shared.map(|grab| CardImageInput {
            source: grab.source,
            tiff: grab.tiff,
        });
    }
    if let Some(input) = card_image_input() {
        return Some(input);
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let image = crate::macos_preview_image::nsimage_from_pasteboard(&pasteboard)?;
    let bytes = image.TIFFRepresentation()?.to_vec();
    (!bytes.is_empty()).then_some(CardImageInput {
        source: ImageSource::Bytes {
            bytes,
            is_png: false,
        },
        tiff: None,
    })
}

/// The bytes of a fetched picture: a copied file is read here, so call it off the main
/// thread. Reads no pasteboard.
pub(crate) fn image_input_bytes(input: CardImageInput) -> Option<Vec<u8>> {
    let bytes = match input.source {
        ImageSource::Bytes { bytes, .. } => bytes,
        ImageSource::File(path) => std::fs::read(path).ok().or(input.tiff)?,
    };
    (!bytes.is_empty()).then_some(bytes)
}

/// The picture fetched by [`card_image_input`], decoded. Reads no pasteboard (the save
/// thread decodes Save's clipboard PNG with it).
pub(crate) fn decode_card_input(input: CardImageInput) -> Option<DecodedPreview> {
    decode_source(input.source, input.tiff)
}

/// Decode a fetched picture (no pasteboard reads). For a file that cannot be read, the TIFF
/// fetched with it.
pub(super) fn decode_source(source: ImageSource, tiff: Option<Vec<u8>>) -> Option<DecodedPreview> {
    match source {
        ImageSource::Bytes { bytes, is_png } => decode_bytes(bytes, is_png),
        ImageSource::File(path) => {
            read_image_file(&path).or_else(|| tiff.and_then(|bytes| decode_bytes(bytes, false)))
        }
    }
}

pub(super) fn read_image_file(path: &std::path::Path) -> Option<DecodedPreview> {
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

/// Uniform type of encoded image bytes, for the pasteboard. Unknown bytes go out as TIFF,
/// the type AppKit falls back to.
pub(super) fn image_uti(bytes: &[u8]) -> &'static str {
    let extension = detect_bytes(bytes).map(|kind| kind.extension);
    uti_for_extension(extension.unwrap_or("tiff"))
}

pub(super) fn uti_for_extension(extension: &str) -> &'static str {
    match extension {
        "png" => "public.png",
        "jpg" | "jpeg" => "public.jpeg",
        "heic" | "heif" => "public.heic",
        "gif" => "com.compuserve.gif",
        "webp" => "org.webmproject.webp",
        "bmp" => "com.microsoft.bmp",
        "avif" => "public.avif",
        _ => "public.tiff",
    }
}

pub(super) fn png_fallback(data: &NSData) -> Option<Retained<NSData>> {
    mac_ui::image::png_from_bitmap_data(data)
}

pub(super) fn tiff_fallback(data: &NSData) -> Option<Retained<NSData>> {
    NSImage::initWithData(NSImage::alloc(), data)?.TIFFRepresentation()
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
    if let Some(shared) = shared_image() {
        let from_grab = shared.and_then(|grab| match &grab.source {
            ImageSource::Bytes { bytes, .. } => mac_ui::image::from_bytes(bytes),
            ImageSource::File(path) => path
                .to_str()
                .and_then(|path| {
                    NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))
                })
                .or_else(|| grab.tiff.as_deref().and_then(mac_ui::image::from_bytes)),
        });
        if from_grab.is_some() {
            return from_grab;
        }
    } else {
        let pasteboard = NSPasteboard::generalPasteboard();
        if let Some(image) = crate::macos_preview_image::nsimage_from_pasteboard(&pasteboard) {
            return Some(image);
        }
    }
    let thumb = image_thumbnail()?;
    crate::macos_preview_image::nsimage_from_clipboard(&thumb)
}

pub(super) fn card_snap() -> Option<CardSnap> {
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

pub(super) fn cached_card(change_count: isize) -> Option<CardSnap> {
    let guard = CARD.lock().ok()?;
    let cached = guard.as_ref()?;
    (cached.change_count == change_count).then(|| cached.clone())
}

pub(super) fn build_card(change_count: isize) -> Option<CardSnap> {
    if let Some(shared) = shared_image() {
        let grab = shared?;
        let tiff = grab.tiff;
        return snap_from_source(change_count, grab.source).or_else(|| {
            let bytes = tiff?;
            snap_from_source(
                change_count,
                ImageSource::Bytes {
                    bytes,
                    is_png: false,
                },
            )
        });
    }
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

pub(super) fn snap_from_source(change_count: isize, source: ImageSource) -> Option<CardSnap> {
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

pub(super) fn snap_with_thumbnail(
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

pub(super) fn snap_from_nsimage(
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

pub(super) struct ImageExport {
    pub(super) bytes: Vec<u8>,
    pub(super) mime: &'static str,
}

impl ImageExport {
    pub(super) fn data_url(&self) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{encoded}", self.mime)
    }
}

/// The encoded bytes and type of a fetched picture (a file is read from disk, not the
/// pasteboard).
pub(super) fn export_from_source(source: &ImageSource) -> Option<ImageExport> {
    match source {
        ImageSource::Bytes { bytes, is_png } => {
            let detected = detect_image_bytes(bytes, *is_png);
            Some(ImageExport {
                bytes: bytes.clone(),
                mime: detected.mime,
            })
        }
        ImageSource::File(path) => {
            let bytes = std::fs::read(path).ok()?;
            let detected = detect_bytes(&bytes).unwrap_or_else(|| extension_kind(path));
            Some(ImageExport {
                bytes,
                mime: detected.mime,
            })
        }
    }
}

pub(super) fn zeroize_cached(cached: &mut CachedView) {
    if let ClipboardView::Text(text) = &mut cached.view {
        text.zeroize();
    }
    if let Some(image) = cached.image.as_mut() {
        image.zeroize();
    }
}

pub(crate) fn zeroize_caches() {
    let mut cache = CACHE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(cached) = cache.as_mut() {
        zeroize_cached(cached);
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

pub(super) fn invalidate_caches() {
    zeroize_caches();
}

/// The format of encoded picture bytes: its label ("JPEG") and file extension ("jpg").
pub(crate) fn image_kind(bytes: &[u8]) -> (&'static str, &'static str) {
    let kind = detect_image_bytes(bytes, infer::image::is_png(bytes));
    (kind.label, kind.extension)
}
