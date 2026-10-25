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

/// Whether a screenshot goes into history. It holds no recognised text yet (that is checked
/// after it went in, and a token or key takes it out again, as for an Image ▾ step's copy), so
/// only Pause holds it back. Pure.
pub fn keep(paused: bool) -> StepKeep {
    let verdict = crate::sensitivity::picture_verdict(None);
    crate::menubar::step_keep(paused, verdict.admitted)
}

/// Whether a screenshot goes on the clipboard concealed: not before its text is known.
pub fn concealed() -> bool {
    crate::sensitivity::picture_verdict(None).concealed
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
    use super::{Next, after_question, concealed, keep, next};
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
        assert_eq!(keep(false), StepKeep::Keep);
        assert_eq!(keep(true), StepKeep::Paused);
        // Its text is not known yet: plain on the clipboard until the check is in.
        assert!(!concealed());
    }
}
