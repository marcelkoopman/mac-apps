use super::*;

pub(super) fn read_view(pasteboard: &NSPasteboard) -> (ClipboardView, Option<PathBuf>) {
    autoreleasepool(|_| {
        let text = pasteboard_string(pasteboard, mac_ui::pasteboard_types::string());
        // ⌘C of a file in Finder puts the file's name on the pasteboard next to its URL.
        // The card shows that file the way a drop does: a picture as a picture, text (CSV,
        // JSON, …) as that text. Real copied text, and a name that is not this file, stays
        // text. Only a short single line is checked against the file URL.
        if let Some(found) = copied_file_view(pasteboard, text.as_deref()) {
            return found;
        }
        let copied_file = text
            .as_deref()
            .is_none_or(could_name_a_file)
            .then(|| image_file(pasteboard))
            .flatten();
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

/// Blank, or one short line: the text Finder puts next to a copied file.
pub(super) fn could_name_a_file(text: &str) -> bool {
    let text = text.trim();
    text.len() <= 4096 && !text.contains('\n')
}

pub(super) fn names_copied_image(text: &str, path: Option<&std::path::Path>) -> bool {
    path.is_some_and(|path| names_copied_file(text, path))
}

/// `text` is the pasteboard's label for `path`: the file name, the full path, or a `file://` URL.
pub(super) fn names_copied_file(text: &str, path: &std::path::Path) -> bool {
    let full = path.to_string_lossy();
    if text == full {
        return true;
    }
    if path.file_name().and_then(|name| name.to_str()) == Some(text) {
        return true;
    }
    text.strip_prefix("file://") == Some(full.as_ref())
}

pub(super) fn has_image_data(pasteboard: &NSPasteboard) -> bool {
    let jpeg = NSString::from_str("public.jpeg");
    let heic = NSString::from_str("public.heic");
    let types = NSArray::from_slice(&[
        mac_ui::pasteboard_types::png(),
        mac_ui::pasteboard_types::tiff(),
        &*jpeg,
        &*heic,
    ]);
    pasteboard.availableTypeFromArray(&types).is_some()
}

#[derive(Clone)]
pub(super) enum ImageSource {
    Bytes { bytes: Vec<u8>, is_png: bool },
    File(PathBuf),
}

pub(super) const EXTRA_IMAGE_TYPES: &[&str] = &[
    "public.heic",
    "public.heif",
    "public.gif",
    "public.webp",
    "org.webmproject.webp",
    "com.microsoft.bmp",
    "public.avif",
];

pub(super) fn image_source(
    pasteboard: &NSPasteboard,
    cached_path: Option<&std::path::Path>,
) -> Option<ImageSource> {
    // A declared PNG that is not image bytes used to win over the file and over
    // TIFF, and the card then had neither a thumbnail nor a size line.
    if let Some(bytes) = take_encoded(pasteboard, mac_ui::pasteboard_types::png()) {
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
    if let Some(bytes) = take_encoded(pasteboard, mac_ui::pasteboard_types::tiff()) {
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

pub(super) fn take_encoded(pasteboard: &NSPasteboard, kind: &NSString) -> Option<Vec<u8>> {
    let bytes = pasteboard_data(pasteboard, kind)?;
    looks_like_encoded_image(&bytes).then_some(bytes)
}

pub(super) fn looks_like_encoded_image(bytes: &[u8]) -> bool {
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

pub(super) fn decode_bytes(bytes: Vec<u8>, is_png: bool) -> Option<DecodedPreview> {
    let image = crate::macos_image_io::preview_from_bytes(&bytes)
        .or_else(|| image_ops::preview_from_encoded(&bytes))?;
    let source_png = is_png.then_some(bytes);
    Some(DecodedPreview { image, source_png })
}

/// A type the pasteboard does not declare is not read: a read is what can show the privacy
/// alert, the check is not.
pub(super) fn pasteboard_string(pasteboard: &NSPasteboard, kind: &NSString) -> Option<String> {
    if !type_available(pasteboard, kind) {
        return None;
    }
    let text = pasteboard.stringForType(kind)?;
    Some(text.to_string())
}

pub(super) fn pasteboard_data(pasteboard: &NSPasteboard, kind: &NSString) -> Option<Vec<u8>> {
    if !type_available(pasteboard, kind) {
        return None;
    }
    let data = pasteboard.dataForType(kind)?;
    let bytes = data.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

/// A Finder copy of one file, when `text` is that file's name (or there is no text). `Some`
/// is the card for its contents: an image file stays a picture, text is the file's text.
/// `None` when this pasteboard is not that file, so the string is shown as copied.
pub(super) fn copied_file_view(
    pasteboard: &NSPasteboard,
    text: Option<&str>,
) -> Option<(ClipboardView, Option<PathBuf>)> {
    if !text.is_none_or(could_name_a_file) {
        return None;
    }
    let url = file_url(pasteboard)?;
    let path = PathBuf::from(url.path()?.to_string());
    if text.is_some_and(|text| !names_copied_file(text.trim(), &path)) {
        return None;
    }
    mac_ui::pasteboard_types::with_security_scope(&url, move || {
        if is_image_file(&path) {
            Some((ClipboardView::Image, Some(path)))
        } else {
            crate::open_file::load(&path)
                .text
                .map(|body| (ClipboardView::Text(body), None))
        }
    })
}

/// The one file on the pasteboard, image or not. A `file://` URL keeps the security scope
/// Finder put on it; an older file-name list is a plain path.
pub(super) fn file_url(pasteboard: &NSPasteboard) -> Option<Retained<NSURL>> {
    if type_available(pasteboard, mac_ui::pasteboard_types::file_url())
        && let Some(raw) = pasteboard_string(pasteboard, mac_ui::pasteboard_types::file_url())
        && let Some(url) = NSURL::URLWithString(&NSString::from_str(&raw))
        && url.path().is_some()
    {
        return Some(url);
    }
    let path = filenames_path(pasteboard)?;
    Some(NSURL::fileURLWithPath(&NSString::from_str(
        &path.to_string_lossy(),
    )))
}

pub(super) fn image_file(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    if type_available(pasteboard, mac_ui::pasteboard_types::file_url())
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

pub(super) fn type_available(pasteboard: &NSPasteboard, kind: &NSString) -> bool {
    let types = NSArray::from_slice(&[kind]);
    pasteboard.availableTypeFromArray(&types).is_some()
}

pub(super) fn path_from_file_url(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let url = pasteboard_string(pasteboard, mac_ui::pasteboard_types::file_url())?;
    let url = NSString::from_str(&url);
    let path = NSURL::URLWithString(&url)?.path()?.to_string();
    Some(PathBuf::from(path))
}

pub(super) fn filenames_path(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let kind = NSString::from_str("NSFilenamesPboardType");
    if !type_available(pasteboard, &kind) {
        return None;
    }
    let list = pasteboard.propertyListForType(&kind)?;
    let array = list.downcast::<NSArray>().ok()?;
    if array.count() != 1 {
        return None;
    }
    Some(PathBuf::from(
        array
            .objectAtIndex(0)
            .downcast::<NSString>()
            .ok()?
            .to_string(),
    ))
}

pub(super) fn filenames_image(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let path = filenames_path(pasteboard)?;
    is_image_file(&path).then_some(path)
}

pub(super) fn is_image_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    match infer::get_from_path(path) {
        Ok(Some(kind)) => kind.matcher_type() == infer::MatcherType::Image,
        Ok(None) | Err(_) => has_image_extension(path),
    }
}

pub(super) fn has_image_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTS
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
}
