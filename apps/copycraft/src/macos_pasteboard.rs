#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use mac_ui::objc2::rc::{Retained, autoreleasepool};
use mac_ui::objc2::runtime::NSObjectProtocol;
use mac_ui::objc2::{AnyThread, sel};
use mac_ui::objc2_app_kit::{
    NSImage, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF,
};
use mac_ui::objc2_foundation::{NSArray, NSData, NSString, NSURL};

use zeroize::{Zeroize, Zeroizing};

use crate::clipboard::{ClipboardImage, ClipboardView};
use crate::commands::{ImageFacts, ImageScan};
use crate::image_ops;
use crate::paste_access::{self, AccessBehavior, Kept, ReadKey, ReadPlan, ReadStrategy};

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "webp", "heic",
];

struct CachedView {
    change_count: isize,
    /// The change count, types and read strategy this reading was made for, and whether it read
    /// the contents: it is made again only as [`paste_access::plan`] says.
    kept: Kept,
    view: ClipboardView,
    marks: PasteMarks,
    image_path: Option<PathBuf>,
    /// A picture read in the same pass as the view when copies are read on showing
    /// ([`ReadStrategy::WhenShown`]): history, the card, the well and the scan share it, so a
    /// copy is read once (one alert under Ask).
    image: Option<ImageGrab>,
}

static CACHE: Mutex<Option<CachedView>> = Mutex::new(None);

/// The card is shown (or being opened): with Default or Ask a new copy is read now.
static CARD_SHOWN: AtomicBool = AtomicBool::new(false);

/// The access setting last seen, to log a change while Copycraft runs.
static LAST_BEHAVIOR: Mutex<Option<Option<AccessBehavior>>> = Mutex::new(None);

/// A picture as the pasteboard held it, read once.
#[derive(Clone)]
struct ImageGrab {
    source: ImageSource,
    /// TIFF for a copied file Copycraft cannot open (sandbox).
    tiff: Option<Vec<u8>>,
}

impl ImageGrab {
    fn zeroize(&mut self) {
        if let ImageSource::Bytes { bytes, .. } = &mut self.source {
            bytes.zeroize();
        }
        if let Some(tiff) = self.tiff.as_mut() {
            tiff.zeroize();
        }
    }
}

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
    // The change count, the types and the access setting read no content (no privacy alert).
    // The setting is looked at on every look, so a change in Settings is picked up.
    let types = type_names(&pasteboard);
    let strategy = paste_access::strategy(current_behavior(&pasteboard));
    let key = ReadKey::new(change_count, types.iter().map(String::as_str), strategy);
    let shown = CARD_SHOWN.load(Ordering::Relaxed);
    // The same copy is read once: also a reading that came back empty, failed or was denied,
    // so a read is not tried (and an alert not shown) again on every tick. A promised file that
    // puts its picture on the pasteboard later adds its types, which reads again.
    let (step, cached) = {
        let guard = CACHE.lock().ok();
        let kept = guard.as_ref().and_then(|guard| guard.as_ref());
        let step = paste_access::plan(kept.map(|cached| &cached.kept), &key, shown);
        let view = kept
            .filter(|_| step == ReadPlan::Reuse)
            .map(|cached| cached.view.clone());
        (step, view)
    };
    if let Some(view) = cached {
        return view;
    }
    let marks = marks_of(types.iter().map(String::as_str));
    let mut image = None;
    let (view, image_path, content_read) = if marks.hides() {
        (ClipboardView::Hidden, None, true)
    } else {
        match step {
            // Always Deny: no content reads at all. (`Reuse` always has a reading, returned
            // above.)
            ReadPlan::Deny | ReadPlan::Reuse => (ClipboardView::Denied, None, false),
            // Default or Ask with the card closed: only the change count and types.
            ReadPlan::Wait => (ClipboardView::Pending, None, false),
            ReadPlan::Read => {
                let (view, image_path) = read_view(&pasteboard);
                let text_declared = types.iter().any(|kind| kind == "public.utf8-plain-text");
                if paste_access::text_read_failed(text_declared, view.text().is_some())
                    && matches!(view, ClipboardView::NoText)
                {
                    eprintln!(
                        "copycraft: clipboard text could not be read (denied?); \
                         not read again until the next copy"
                    );
                }
                if strategy == ReadStrategy::WhenShown && view.is_image() {
                    image = grab_image(&pasteboard, image_path.as_deref());
                }
                (view, image_path, true)
            }
        }
    };
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(old) = guard.as_mut() {
            zeroize_cached(old);
        }
        *guard = Some(CachedView {
            change_count,
            kept: Kept { key, content_read },
            view: view.clone(),
            marks,
            image_path,
            image,
        });
    }
    view
}

/// Tell the pasteboard reader whether the card is shown (or about to be): with Default or Ask
/// a copy is read only then.
pub(crate) fn set_card_shown(shown: bool) {
    CARD_SHOWN.store(shown, Ordering::Relaxed);
}

/// The read strategy for the access setting now (cheap: no content read).
pub(crate) fn read_strategy() -> ReadStrategy {
    paste_access::strategy(current_behavior(&NSPasteboard::generalPasteboard()))
}

/// The access setting, logged when it differs from the one seen before.
fn current_behavior(pasteboard: &NSPasteboard) -> Option<AccessBehavior> {
    let behavior = access_behavior(pasteboard);
    if let Ok(mut last) = LAST_BEHAVIOR.lock() {
        if let Some(seen) = *last
            && seen != behavior
        {
            eprintln!("{}", paste_access::change_log_line(behavior));
        }
        *last = Some(behavior);
    }
    behavior
}

/// The picture on the pasteboard, read in the view's pass: only types the pasteboard declares
/// are read. Main thread only.
fn grab_image(
    pasteboard: &NSPasteboard,
    image_path: Option<&std::path::Path>,
) -> Option<ImageGrab> {
    autoreleasepool(|_| {
        let source = image_source(pasteboard, image_path).or_else(|| {
            // A picture only AppKit can draw: its TIFF, from the same pass.
            let image = crate::macos_preview_image::nsimage_from_pasteboard(pasteboard)?;
            let bytes = image.TIFFRepresentation()?.to_vec();
            (!bytes.is_empty()).then_some(ImageSource::Bytes {
                bytes,
                is_png: false,
            })
        })?;
        let tiff = match &source {
            ImageSource::File(path) if std::fs::File::open(path).is_err() => {
                pasteboard_data(pasteboard, unsafe { NSPasteboardTypeTIFF })
            }
            _ => None,
        };
        Some(ImageGrab { source, tiff })
    })
}

/// How the pictures of the current copy are had: `Some` with the picture read in the view's
/// pass (Default or Ask; `None` inside when there was none), `None` to read the pasteboard as
/// before (Always Allow, macOS before 15.4).
fn shared_image() -> Option<Option<ImageGrab>> {
    let change_count = change_count();
    let guard = CACHE.lock().ok()?;
    let cached = guard
        .as_ref()
        .filter(|cached| cached.change_count == change_count);
    match cached {
        Some(cached) if cached.kept.key.strategy == ReadStrategy::Now => None,
        Some(cached) => Some(cached.image.clone()),
        // No reading of this copy yet: no content read now, unless copies are read at once.
        None => (read_strategy() != ReadStrategy::Now).then_some(None),
    }
}

/// The general pasteboard's access setting, or `None` before macOS 15.4 (no pasteboard
/// privacy): checked with `respondsToSelector:`, as `mac_ui::activation` checks `activate`.
fn access_behavior(pasteboard: &NSPasteboard) -> Option<AccessBehavior> {
    pasteboard
        .respondsToSelector(sel!(accessBehavior))
        .then(|| AccessBehavior::from_raw(pasteboard.accessBehavior().0))
}

/// Log the general pasteboard's access setting (once, at startup; stderr only). A change is
/// logged when it is seen ([`current_behavior`]).
pub(crate) fn log_access_behavior() {
    let behavior = access_behavior(&NSPasteboard::generalPasteboard());
    if let Ok(mut last) = LAST_BEHAVIOR.lock() {
        *last = Some(behavior);
    }
    eprintln!("{}", paste_access::startup_log_line(behavior));
}

/// The pasteboard's type names (no content).
fn type_names(pasteboard: &NSPasteboard) -> Vec<String> {
    pasteboard
        .types()
        .map(|types| types.iter().map(|kind| kind.to_string()).collect())
        .unwrap_or_default()
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

/// The current picture as the pasteboard holds it, fetched on the main thread for a scan that
/// decodes on a background thread ([`scan_card_input`]). Pasteboard reads stay on the main
/// thread: with pasteboard privacy a read can show a modal alert, which must not come from a
/// background thread.
pub(crate) struct CardImageInput {
    source: ImageSource,
    /// TIFF from the pasteboard for a copied file Copycraft cannot open (sandbox), to decode
    /// instead.
    tiff: Option<Vec<u8>>,
}

/// Fetch the current picture for a scan. Main thread only.
pub(crate) fn card_image_input() -> Option<CardImageInput> {
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
            autoreleasepool(|_| pasteboard_data(&pasteboard, unsafe { NSPasteboardTypeTIFF }))
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

/// Format, pixel size and byte length of an encoded picture that is not the clipboard's, or
/// `None` when it can't be read.
pub(crate) fn image_bytes_facts(bytes: &[u8]) -> Option<ImageFacts> {
    let source = ImageSource::Bytes {
        bytes: bytes.to_vec(),
        is_png: infer::image::is_png(bytes),
    };
    snap_from_source(0, source).map(|snap| snap.facts)
}

fn scan_decoded(decoded: DecodedPreview, data_url: Option<String>) -> Option<ImageScan> {
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

/// The picture fetched by [`card_image_input`], decoded. Reads no pasteboard (the save
/// thread decodes Save's clipboard PNG with it).
pub(crate) fn decode_card_input(input: CardImageInput) -> Option<DecodedPreview> {
    decode_source(input.source, input.tiff)
}

/// Decode a fetched picture (no pasteboard reads). For a file that cannot be read, the TIFF
/// fetched with it.
fn decode_source(source: ImageSource, tiff: Option<Vec<u8>>) -> Option<DecodedPreview> {
    match source {
        ImageSource::Bytes { bytes, is_png } => decode_bytes(bytes, is_png),
        ImageSource::File(path) => {
            read_image_file(&path).or_else(|| tiff.and_then(|bytes| decode_bytes(bytes, false)))
        }
    }
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

/// Pasteboard type on everything copycraft writes itself (a history step, Copy, Format). The
/// poller sees it and does not take the write for a new copy; copycraft records what it wants
/// in history when it writes. Its data is empty.
pub(crate) const SELF_TYPE: &str = "nl.marcelkoopman.copycraft.self";

/// Markers from nspasteboard.org. A password manager puts Concealed on a password, an app puts
/// Transient on a copy that is gone again soon and AutoGenerated on one the user did not make.
/// Copies with any of them are not recorded in history and their text is not read at all.
const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";
const AUTO_GENERATED_TYPE: &str = "org.nspasteboard.AutoGeneratedType";

/// What the marker types on the pasteboard say about the current copy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PasteMarks {
    /// Copycraft wrote it ([`SELF_TYPE`]).
    pub own: bool,
    /// Concealed, Transient or AutoGenerated.
    pub private: bool,
}

impl PasteMarks {
    /// Another app's private copy: the card shows "Hidden content", nothing else.
    pub(crate) fn hides(self) -> bool {
        self.private && !self.own
    }
}

fn marks_of<'a>(types: impl IntoIterator<Item = &'a str>) -> PasteMarks {
    let mut marks = PasteMarks::default();
    for kind in types {
        match kind {
            SELF_TYPE => marks.own = true,
            CONCEALED_TYPE | TRANSIENT_TYPE | AUTO_GENERATED_TYPE => marks.private = true,
            _ => {}
        }
    }
    marks
}

fn read_marks(pasteboard: &NSPasteboard) -> PasteMarks {
    let names = type_names(pasteboard);
    marks_of(names.iter().map(String::as_str))
}

/// Marks of the current pasteboard contents, read once per change count with the view.
pub(crate) fn current_marks() -> PasteMarks {
    let change_count = change_count();
    let cached = CACHE.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .and_then(|cached| (cached.change_count == change_count).then_some(cached.marks))
    });
    cached.unwrap_or_else(|| read_marks(&NSPasteboard::generalPasteboard()))
}

/// Put `text` on the pasteboard as plain text, marked as copycraft's own write, and as
/// concealed when it is labelled sensitive, so other clipboard managers leave it out.
pub(crate) fn write_text(text: &str, concealed: bool) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let ok = autoreleasepool(|_| {
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
            && mark_own(&pasteboard, concealed)
    });
    invalidate_caches();
    if ok {
        Ok(())
    } else {
        Err("could not write text".into())
    }
}

/// Put a history picture back on the pasteboard as data: its own type (JPEG stays JPEG) and,
/// for apps that only paste PNG, a PNG copy; TIFF when that conversion fails. Never a file:
/// a picture in a temporary file outlived the history entry and the app.
pub(crate) fn write_history_image(bytes: &[u8]) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let ok = autoreleasepool(|_| {
        let data = NSData::with_bytes(bytes);
        let original = image_uti(bytes);
        pasteboard.clearContents();
        let mut ok = pasteboard.setData_forType(Some(&data), &NSString::from_str(original));
        if original != "public.png" && original != "public.tiff" {
            match png_fallback(&data) {
                Some(png) => {
                    ok &= pasteboard.setData_forType(Some(&png), unsafe { NSPasteboardTypePNG });
                }
                None => {
                    if let Some(tiff) = tiff_fallback(&data) {
                        ok &= pasteboard
                            .setData_forType(Some(&tiff), unsafe { NSPasteboardTypeTIFF });
                    }
                }
            }
        }
        ok && mark_own(&pasteboard, false)
    });
    invalidate_caches();
    if ok {
        Ok(())
    } else {
        Err("could not write image".into())
    }
}

/// Add the Concealed type to copycraft's own write on the pasteboard, once its labels are in.
pub(crate) fn add_concealed() {
    let pasteboard = NSPasteboard::generalPasteboard();
    if read_marks(&pasteboard).own {
        pasteboard.setData_forType(Some(&NSData::new()), &NSString::from_str(CONCEALED_TYPE));
        invalidate_caches();
    }
}

fn mark_own(pasteboard: &NSPasteboard, concealed: bool) -> bool {
    let empty = NSData::new();
    let own = pasteboard.setData_forType(Some(&empty), &NSString::from_str(SELF_TYPE));
    own && (!concealed
        || pasteboard.setData_forType(Some(&empty), &NSString::from_str(CONCEALED_TYPE)))
}

/// Uniform type of encoded image bytes, for the pasteboard. Unknown bytes go out as TIFF,
/// the type AppKit falls back to.
fn image_uti(bytes: &[u8]) -> &'static str {
    let extension = detect_bytes(bytes).map(|kind| kind.extension);
    uti_for_extension(extension.unwrap_or("tiff"))
}

fn uti_for_extension(extension: &str) -> &'static str {
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

fn png_fallback(data: &NSData) -> Option<Retained<NSData>> {
    use mac_ui::objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use mac_ui::objc2_foundation::NSDictionary;
    let rep = NSBitmapImageRep::imageRepWithData(data)?;
    // SAFETY: an empty dictionary is a valid property list for any file type.
    unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
}

fn tiff_fallback(data: &NSData) -> Option<Retained<NSData>> {
    NSImage::initWithData(NSImage::alloc(), data)?.TIFFRepresentation()
}

/// Remove pictures an older copycraft left in the temporary folder for history steps.
pub(crate) fn remove_stale_history_files() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        if is_stale_history_file(&entry.file_name().to_string_lossy()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn is_stale_history_file(name: &str) -> bool {
    name.starts_with("copycraft-history-")
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

impl ImageExport {
    fn data_url(&self) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        format!("data:{};base64,{encoded}", self.mime)
    }
}

/// The encoded bytes and type of a fetched picture (a file is read from disk, not the
/// pasteboard).
fn export_from_source(source: &ImageSource) -> Option<ImageExport> {
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

fn zeroize_cached(cached: &mut CachedView) {
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

fn invalidate_caches() {
    zeroize_caches();
}

/// The format of encoded picture bytes: its label ("JPEG") and file extension ("jpg").
pub(crate) fn image_kind(bytes: &[u8]) -> (&'static str, &'static str) {
    let kind = detect_image_bytes(bytes, infer::image::is_png(bytes));
    (kind.label, kind.extension)
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
        // the file, so it still counts as an image. Other text stays text. Only text that
        // could be a name or path is checked against the file URL types.
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
fn could_name_a_file(text: &str) -> bool {
    let text = text.trim();
    text.len() <= 4096 && !text.contains('\n')
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

fn has_image_data(pasteboard: &NSPasteboard) -> bool {
    let jpeg = NSString::from_str("public.jpeg");
    let heic = NSString::from_str("public.heic");
    let types = unsafe {
        NSArray::from_slice(&[NSPasteboardTypePNG, NSPasteboardTypeTIFF, &*jpeg, &*heic])
    };
    pasteboard.availableTypeFromArray(&types).is_some()
}

#[derive(Clone)]
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

/// A type the pasteboard does not declare is not read: a read is what can show the privacy
/// alert, the check is not.
fn pasteboard_string(pasteboard: &NSPasteboard, kind: &NSString) -> Option<String> {
    if !type_available(pasteboard, kind) {
        return None;
    }
    let text = pasteboard.stringForType(kind)?;
    Some(text.to_string())
}

fn pasteboard_data(pasteboard: &NSPasteboard, kind: &NSString) -> Option<Vec<u8>> {
    if !type_available(pasteboard, kind) {
        return None;
    }
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
        PasteMarks, detect_image_bytes, extension_kind, image_uti, is_image_file,
        is_stale_history_file, marks_of, names_copied_image,
    };

    #[test]
    fn private_markers_hide_other_apps_copies_only() {
        let password = marks_of(["public.utf8-plain-text", "org.nspasteboard.ConcealedType"]);
        assert!(password.hides());
        assert!(marks_of(["org.nspasteboard.TransientType"]).hides());
        assert!(marks_of(["org.nspasteboard.AutoGeneratedType"]).hides());
        let own = marks_of([
            "nl.marcelkoopman.copycraft.self",
            "org.nspasteboard.ConcealedType",
        ]);
        assert_eq!(
            own,
            PasteMarks {
                own: true,
                private: true
            }
        );
        assert!(!own.hides());
        assert!(!marks_of(["public.utf8-plain-text"]).hides());
    }

    #[test]
    fn history_pictures_keep_their_own_type() {
        assert_eq!(image_uti(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), "public.png");
        assert_eq!(
            image_uti(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F']),
            "public.jpeg"
        );
        assert_eq!(image_uti(b"GIF89a\x01\0\x01\0"), "com.compuserve.gif");
        assert_eq!(image_uti(b"not an image"), "public.tiff");
    }

    #[test]
    fn only_old_history_files_are_stale() {
        assert!(is_stale_history_file("copycraft-history-1712345678.jpeg"));
        assert!(!is_stale_history_file("copycraft-drop-1.png"));
        assert!(!is_stale_history_file("other-copycraft-history-1.png"));
    }

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
