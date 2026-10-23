#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use mac_ui::objc2::rc::{Retained, autoreleasepool};
use mac_ui::objc2::runtime::NSObjectProtocol;
use mac_ui::objc2::{AnyThread, sel};
use mac_ui::objc2_app_kit::{NSImage, NSPasteboard, NSWorkspace};
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

/// The access setting last seen, to log a change while CopyCraft runs.
static LAST_BEHAVIOR: Mutex<Option<Option<AccessBehavior>>> = Mutex::new(None);

/// A picture as the pasteboard held it, read once.
#[derive(Clone)]
struct ImageGrab {
    source: ImageSource,
    /// TIFF for a copied file CopyCraft cannot open (sandbox).
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
    // History off (paste-first): nothing is looked at, not even the change count. The card shows
    // what was pasted or dropped (history), as while recording is paused.
    if !background_read_allowed() {
        return ClipboardView::Held;
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    // The change count, the types and the access setting read no content (no privacy alert).
    // The setting is looked at on every look, so a change in Settings is picked up.
    let types = type_names(&pasteboard);
    let behavior = current_behavior(&pasteboard);
    let strategy = paste_access::strategy(behavior);
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
    if !marks.own && !marks.hides() && foreign_skipped(change_count) {
        let view = ClipboardView::Held;
        if let Ok(mut guard) = CACHE.lock() {
            if let Some(old) = guard.as_mut() {
                zeroize_cached(old);
            }
            *guard = Some(CachedView {
                change_count,
                kept: Kept {
                    key,
                    content_read: true,
                },
                view: view.clone(),
                marks,
                image_path: None,
                image: None,
            });
        }
        return view;
    }
    let mut image = None;
    let (view, image_path, content_read) = if marks.hides() {
        (ClipboardView::Hidden, None, true)
    } else {
        match step {
            // Always Deny: no content reads at all. (`Reuse` always has a reading, returned
            // above.)
            ReadPlan::Deny | ReadPlan::Reuse => (ClipboardView::Denied, None, false),
            // Default or Ask with the card closed: only the change count and types, and the
            // kind macOS detects (no content read, no alert).
            ReadPlan::Wait => {
                if paste_access::detects_hint(behavior, step) {
                    hint::ask(&pasteboard, change_count);
                }
                (ClipboardView::Pending, None, false)
            }
            ReadPlan::Read => {
                let (view, image_path) = read_view(&pasteboard);
                let text_declared = types.iter().any(|kind| kind == "public.utf8-plain-text");
                if paste_access::text_read_failed(text_declared, view.text().is_some())
                    && matches!(view, ClipboardView::NoText)
                {
                    crate::diag::log(crate::diag::LogEvent::ClipboardUnreadable);
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

/// Whether CopyCraft may read the pasteboard on its own now: the poll, or the card opening
/// ([`crate::history_mode::read_allowed`]). ⌘V does not ask: [`read_for_paste`] always reads.
pub(crate) fn background_read_allowed() -> bool {
    let trigger = if CARD_SHOWN.load(Ordering::Relaxed) {
        crate::history_mode::ReadTrigger::CardOpen
    } else {
        crate::history_mode::ReadTrigger::Poll
    };
    crate::history_mode::read_allowed(crate::settings::clipboard_history(), trigger)
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
            crate::diag::log(crate::diag::LogEvent::PasteAccess {
                behavior,
                changed: true,
            });
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
                pasteboard_data(pasteboard, mac_ui::pasteboard_types::tiff())
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
    // History off: no picture of the pasteboard's (and no read for one).
    if !background_read_allowed() {
        return Some(None);
    }
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
    crate::diag::log(crate::diag::LogEvent::PasteAccess {
        behavior,
        changed: false,
    });
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

/// Frontmost app's bundle id. No pasteboard read. `None` when there is none.
pub(crate) fn front_bundle() -> Option<String> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(app.bundleIdentifier()?.to_string())
}

/// A foreign change that must not be read. Own writes and concealed copies are left to
/// their own paths. Asking twice about one change does not consume ignore-next again.
fn foreign_skipped(change: isize) -> bool {
    let bundle = front_bundle();
    crate::capture::observe_front(bundle.as_deref());
    let ignored = bundle
        .as_deref()
        .is_some_and(crate::settings::bundle_ignored);
    crate::capture::consider(change, ignored).is_some()
}

/// Refuse a pasteboard picture read for a suppressed foreign change, or for a concealed copy.
fn foreign_blocked() -> bool {
    if !background_read_allowed() {
        return true;
    }
    let marks = current_marks();
    if marks.own {
        return false;
    }
    if marks.hides() {
        return true;
    }
    foreign_skipped(change_count())
}

mod image;
#[allow(unused_imports)]
pub use image::*;
mod write;
#[allow(unused_imports)]
pub use write::*;
mod detect;
mod hint;
pub(crate) use hint::pending_hint;
mod paste;
#[allow(unused_imports)]
pub use detect::*;
pub(crate) use paste::read_for_paste;
mod read;
#[allow(unused_imports)]
pub use read::*;
#[cfg(test)]
mod tests;
