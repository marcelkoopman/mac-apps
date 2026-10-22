//! ⌘V in the card: the one read of the general pasteboard the user asks for. It is made also
//! while "Keep clipboard history" is off (paste-first), on the main thread while the key event is
//! handled, and goes to the card like a drop (`App::show_pasted`). Nothing is cached here.

use super::*;
use crate::history_mode::Pasted;

/// What is on the clipboard now: its text, else its picture. A copy another app marks private
/// is not read. Main thread only.
pub(crate) fn read_for_paste() -> Pasted {
    use crate::history_mode::{ReadTrigger, read_allowed};
    // Always allowed (the user pressed ⌘V); the gate keeps the rule in one place.
    if !read_allowed(
        crate::settings::clipboard_history(),
        ReadTrigger::ExplicitPaste,
    ) {
        return Pasted::Nothing;
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    if read_marks(&pasteboard).hides() {
        return Pasted::Hidden;
    }
    let (view, image_path) = read_view(&pasteboard);
    match view {
        ClipboardView::Text(text) => Pasted::Text(text),
        ClipboardView::Image => pasted_image(&pasteboard, image_path.as_deref())
            .map_or(Pasted::Nothing, |bytes| {
                Pasted::Image(Zeroizing::new(bytes))
            }),
        _ => Pasted::Nothing,
    }
}

/// The picture's encoded bytes: the pasteboard's own data, a copied image file, or the TIFF
/// AppKit can make of it. What is not used is zeroized.
fn pasted_image(
    pasteboard: &NSPasteboard,
    image_path: Option<&std::path::Path>,
) -> Option<Vec<u8>> {
    let mut grab = grab_image(pasteboard, image_path)?;
    let bytes = match &mut grab.source {
        ImageSource::Bytes { bytes, .. } => Some(std::mem::take(bytes)),
        ImageSource::File(path) => std::fs::read(&*path)
            .ok()
            .or_else(|| grab.tiff.as_mut().map(std::mem::take)),
    };
    grab.zeroize();
    bytes.filter(|bytes| !bytes.is_empty())
}
