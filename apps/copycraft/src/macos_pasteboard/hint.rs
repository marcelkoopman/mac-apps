//! The kind of a copy not read yet ([`paste_access::PasteHint`]), from
//! `detectPatternsForPatterns:completionHandler:` (macOS 15.4+). Detection reads no content and
//! shows no privacy alert (docs/pasteboard-privacy/FINDINGS.md §2): only which patterns match
//! is known, never the text. Asked once per change count, only for a copy waiting for the card
//! under Default or Ask ([`paste_access::detects_hint`]). The kind is never logged.

use super::*;

use mac_ui::block2::RcBlock;
use mac_ui::objc2_app_kit::NSPasteboardDetectionPattern;
use mac_ui::objc2_foundation::{NSError, NSSet};

use crate::paste_access::{DetectedPattern, PasteHint};

/// The hint asked for one change count: `None` inside until macOS answers (or when nothing
/// matched).
struct Asked {
    change_count: isize,
    hint: Option<PasteHint>,
}

static HINT: Mutex<Option<Asked>> = Mutex::new(None);

/// AppKit's pattern constants, paired with ours.
fn patterns() -> [(&'static NSPasteboardDetectionPattern, DetectedPattern); 7] {
    use mac_ui::objc2_app_kit::{
        NSPasteboardDetectionPatternCalendarEvent, NSPasteboardDetectionPatternEmailAddress,
        NSPasteboardDetectionPatternLink, NSPasteboardDetectionPatternNumber,
        NSPasteboardDetectionPatternPhoneNumber, NSPasteboardDetectionPatternPostalAddress,
        NSPasteboardDetectionPatternProbableWebURL,
    };
    // SAFETY: reads of AppKit's immutable `NSString` constants (extern statics); the method
    // that uses them is only called after `respondsToSelector:` (macOS 15.4+, where they exist).
    let constants = unsafe {
        [
            NSPasteboardDetectionPatternProbableWebURL,
            NSPasteboardDetectionPatternLink,
            NSPasteboardDetectionPatternNumber,
            NSPasteboardDetectionPatternPhoneNumber,
            NSPasteboardDetectionPatternEmailAddress,
            NSPasteboardDetectionPatternPostalAddress,
            NSPasteboardDetectionPatternCalendarEvent,
        ]
    };
    std::array::from_fn(|at| (constants[at], DetectedPattern::ASKED[at]))
}

/// Ask macOS which patterns copy `change_count` matches, once per change count. The answer
/// comes on another queue; the icon's tooltip is redone then ([`crate::launcher::UserEvent::PasteHint`]).
pub(super) fn ask(pasteboard: &NSPasteboard, change_count: isize) {
    if !pasteboard.respondsToSelector(sel!(detectPatternsForPatterns:completionHandler:)) {
        return;
    }
    {
        let Ok(mut asked) = HINT.lock() else {
            return;
        };
        if asked
            .as_ref()
            .is_some_and(|asked| asked.change_count == change_count)
        {
            return;
        }
        *asked = Some(Asked {
            change_count,
            hint: None,
        });
    }
    let pairs = patterns();
    let wanted: Vec<&NSPasteboardDetectionPattern> = pairs.iter().map(|(ns, _)| *ns).collect();
    let wanted = NSSet::from_slice(&wanted);
    let handler = RcBlock::new(
        move |found: *mut NSSet<NSPasteboardDetectionPattern>, _error: *mut NSError| {
            // SAFETY: AppKit passes a valid set, or nil on an error (then there is no hint),
            // that lives for the call; it is only read here.
            let Some(found) = (unsafe { found.as_ref() }) else {
                return;
            };
            let matched: Vec<DetectedPattern> = pairs
                .iter()
                .filter(|(ns, _)| found.containsObject(ns))
                .map(|(_, pattern)| *pattern)
                .collect();
            let Some(hint) = paste_access::hint_of(&matched) else {
                return;
            };
            let current = HINT
                .lock()
                .ok()
                .is_some_and(|mut asked| match asked.as_mut() {
                    Some(asked) if asked.change_count == change_count => {
                        asked.hint = Some(hint);
                        true
                    }
                    _ => false,
                });
            if current {
                crate::launcher::emit(crate::launcher::UserEvent::PasteHint);
            }
        },
    );
    pasteboard.detectPatternsForPatterns_completionHandler(&wanted, &handler);
}

/// The hint for the copy on the pasteboard now, if macOS has answered for it. No content read.
pub(crate) fn pending_hint() -> Option<PasteHint> {
    let change_count = NSPasteboard::generalPasteboard().changeCount();
    HINT.lock()
        .ok()?
        .as_ref()
        .filter(|asked| asked.change_count == change_count)
        .and_then(|asked| asked.hint)
}
