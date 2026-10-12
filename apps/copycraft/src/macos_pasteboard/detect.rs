use super::*;

pub(super) struct ContentKind {
    pub(super) label: &'static str,
    pub(super) mime: &'static str,
    pub(super) extension: &'static str,
}

pub(super) fn detect_bytes(bytes: &[u8]) -> Option<ContentKind> {
    infer::get(bytes)
        .filter(|kind| kind.matcher_type() == infer::MatcherType::Image)
        .map(content_kind)
}

pub(super) fn detect_image_bytes(bytes: &[u8], hint_png: bool) -> ContentKind {
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

pub(super) fn detect_path(path: &std::path::Path) -> ContentKind {
    infer::get_from_path(path)
        .ok()
        .flatten()
        .filter(|kind| kind.matcher_type() == infer::MatcherType::Image)
        .map(content_kind)
        .unwrap_or_else(|| extension_kind(path))
}

pub(super) fn content_kind(kind: infer::Type) -> ContentKind {
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

pub(super) fn extension_kind(path: &std::path::Path) -> ContentKind {
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
