//! Screenshots (phase 1), without AppKit: when the Screen Recording permission is asked, and
//! whether a screenshot goes into history. The capture itself, the selection overlay and the
//! thumbnail are `mac_ui::screenshot`'s; `menubar::shot` wires them to the app.
//!
//! A screenshot is a new copy: on the clipboard and, as CopyCraft's own copy the user asked
//! for, in history like an Image ▾ step's (Pause holds it, an ignored front app does not). It
//! stays in memory only, so Wipe, the lock screen and forgetting held state take it as any
//! other picture. Nothing about it is logged but that it was taken or why it failed.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::menubar::StepKeep;

/// Whether screenshots can be taken here: ScreenCaptureKit is there (always on macOS 26, the
/// app's floor; looked up anyway, so a stripped system greys the command out instead).
pub fn available() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac_ui::screenshot::available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// What a screenshot request does next, from the Screen Recording permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Allowed: show the selection overlay.
    Select,
    /// Never asked: explain first (one *Continue* button), then the system's question.
    Explain,
    /// Asked before and still not allowed: say so, with *Open Privacy & Security…*.
    Denied,
}

/// [`Next`] from whether macOS allows screen capture now (`granted`, asked again on every
/// request, so a change in System Settings is picked up) and whether CopyCraft asked before
/// (`asked`). Pure.
pub fn next(granted: bool, asked: bool) -> Next {
    if granted {
        Next::Select
    } else if asked {
        Next::Denied
    } else {
        Next::Explain
    }
}

/// After the system's question: allowed goes on to the selection; anything else ends here.
/// macOS shows its own alert on the first question (with a way into System Settings), so the
/// refusal is not repeated now; the next request says it ([`Next::Denied`]). Pure.
pub fn after_question(granted: bool) -> Option<Next> {
    granted.then_some(Next::Select)
}

/// How long a screenshot waits for its text before it goes on the clipboard. Within it, the
/// text decides at once, as for any picture; later, it goes on concealed and the full scan
/// ([`crate::macos_pasteboard::RECOGNITION_LIMIT`]) checks it again.
pub const RECOGNITION_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// How a screenshot goes on the clipboard and into history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotVerdict {
    /// What its text decided ([`crate::sensitivity::picture_verdict`]); with the text not in
    /// yet, that of a picture without text, so the later scan can still tighten it
    /// ([`crate::sensitivity::picture_recheck`]).
    pub verdict: crate::sensitivity::PictureVerdict,
    /// Marked Concealed on the clipboard: sensitive text, or text not known yet. Only a
    /// sensitive text also clears it after a minute.
    pub mark_concealed: bool,
}

/// [`ShotVerdict`] from the text recognised within [`RECOGNITION_WAIT`]: `None` when it was
/// not in by then, `Some(None)` for a picture without text. Pure.
pub fn verdict(recognised: Option<Option<&str>>) -> ShotVerdict {
    let verdict = crate::sensitivity::picture_verdict(recognised.flatten());
    ShotVerdict {
        verdict,
        mark_concealed: verdict.concealed || recognised.is_none(),
    }
}

/// Whether a screenshot with `verdict` goes into history: Pause holds it back, and a token or
/// key in its text keeps it out. Pure.
pub fn keep(paused: bool, verdict: ShotVerdict) -> StepKeep {
    crate::menubar::step_keep(paused, verdict.verdict.admitted)
}

/// Why a screenshot failed, for the log: a kind, never what was on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Failure {
    /// ScreenCaptureKit is not there (an older or a stripped macOS).
    Unavailable,
    /// macOS refused the capture (no Screen Recording permission).
    NotAllowed,
    /// The screen, window or area was gone, or the capture returned no picture.
    NoPicture,
    /// The picture could not be put on the clipboard.
    Clipboard,
}

impl Failure {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::NotAllowed => "not allowed",
            Self::NoPicture => "no picture",
            Self::Clipboard => "clipboard",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Next, after_question, keep, next, verdict};
    use crate::menubar::StepKeep;

    #[test]
    fn the_permission_is_explained_once_then_refusals_point_to_system_settings() {
        // Allowed: straight to the selection, asked before or not.
        assert_eq!(next(true, false), Next::Select);
        assert_eq!(next(true, true), Next::Select);
        // Never asked: the explanation, then the system's question.
        assert_eq!(next(false, false), Next::Explain);
        // Asked and still refused: no second explanation, the way to System Settings.
        assert_eq!(next(false, true), Next::Denied);
        // After the question: allowed selects, a refusal stops without a second alert.
        assert_eq!(after_question(true), Some(Next::Select));
        assert_eq!(after_question(false), None);
    }

    #[test]
    fn a_screenshot_goes_into_history_unless_recording_is_paused() {
        let plain = verdict(Some(None));
        assert_eq!(keep(false, plain), StepKeep::Keep);
        assert_eq!(keep(true, plain), StepKeep::Paused);
        assert!(!plain.mark_concealed);
    }

    #[test]
    fn a_screenshot_waits_for_its_text_before_the_clipboard() {
        assert_eq!(super::RECOGNITION_WAIT, std::time::Duration::from_secs(1));
        // Ordinary text: plain, in history.
        let text = verdict(Some(Some("Meeting notes for Tuesday")));
        assert!(!text.mark_concealed);
        assert_eq!(keep(false, text), StepKeep::Keep);
        // A token: concealed and kept out of history before it reaches either.
        let token = verdict(Some(Some("ghp_1234567890abcdefghijklmnopqrstuvwxyzAB")));
        assert!(token.mark_concealed && token.verdict.concealed);
        assert_eq!(keep(false, token), StepKeep::Secret);
        // An e-mail address: concealed (cleared later), still in history.
        let mail = verdict(Some(Some("write to jan.jansen@example.com")));
        assert!(mail.mark_concealed && mail.verdict.concealed);
        assert_eq!(keep(false, mail), StepKeep::Keep);
        // Not in within the wait: concealed until the full scan, which can still take it out.
        let late = verdict(None);
        assert!(late.mark_concealed);
        assert!(!late.verdict.concealed);
        assert_eq!(keep(false, late), StepKeep::Keep);
    }
}
