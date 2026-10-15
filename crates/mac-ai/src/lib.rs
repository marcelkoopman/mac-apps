//! Local AI actions on text with Apple's Foundation Models framework (macOS 26+, Apple silicon,
//! Apple Intelligence on). The model runs on the device: no network, nothing stored.
//!
//! The Swift side is in `swift/bridge.swift` (compiled by `build.rs`); [`bridge`] is the only
//! place with `unsafe`. Without the bridge (another OS, an Intel Mac, no `swiftc` at build time)
//! the crate still compiles and [`availability`] says why the actions are unavailable.
//!
//! Privacy: this crate never logs, prints or caches text. An [`Error`] holds no text, only its
//! kind ([`Error::kind`]), which is all a caller may log.

use std::fmt;

#[cfg(mac_ai_swift)]
#[path = "bridge.rs"]
mod bridge;
#[cfg(not(mac_ai_swift))]
#[path = "bridge_stub.rs"]
mod bridge;

pub use bridge::Job;

/// The longest text sent to the model, in characters. Longer text is cut ([`truncate`]).
pub const MAX_CHARS: usize = 20_000;

/// What the model does with the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Summarize,
    Shorter,
    MoreFormal,
    Explain,
    /// Dutch to English or English to Dutch, whichever the text is not.
    Translate,
}

impl Action {
    pub const ALL: [Action; 5] = [
        Action::Summarize,
        Action::Shorter,
        Action::MoreFormal,
        Action::Explain,
        Action::Translate,
    ];

    /// The number `bridge.swift` knows the action by.
    #[cfg_attr(not(mac_ai_swift), allow(dead_code))]
    pub(crate) fn code(self) -> i32 {
        match self {
            Action::Summarize => 0,
            Action::Shorter => 1,
            Action::MoreFormal => 2,
            Action::Explain => 3,
            Action::Translate => 4,
        }
    }
}

/// Why the actions cannot run here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    /// Not macOS, or macOS before 26.
    UnsupportedSystem,
    NotAppleSilicon,
    /// This Mac does not support Apple Intelligence.
    DeviceNotEligible,
    /// Apple Intelligence is switched off in System Settings.
    AppleIntelligenceOff,
    /// Apple Intelligence is on, but the model is still downloading or preparing.
    ModelNotReady,
    /// This build has no Swift bridge (it was built without `swiftc`).
    NoBridge,
    Other,
}

/// Whether the model can be used right now.
pub type Availability = Result<(), Unavailable>;

/// Read the status code of `mac_ai_availability` in `bridge.swift`.
#[cfg_attr(not(mac_ai_swift), allow(dead_code))]
pub(crate) fn decode_availability(code: i32) -> Availability {
    match code {
        0 => Ok(()),
        1 => Err(Unavailable::DeviceNotEligible),
        2 => Err(Unavailable::AppleIntelligenceOff),
        3 => Err(Unavailable::ModelNotReady),
        5 => Err(Unavailable::UnsupportedSystem),
        _ => Err(Unavailable::Other),
    }
}

/// Whether Foundation Models can run now: macOS 26+, Apple silicon and Apple Intelligence on.
/// Cheap, but ask the system each time: the user can switch Apple Intelligence on or off.
pub fn availability() -> Availability {
    if !cfg!(target_os = "macos") {
        return Err(Unavailable::UnsupportedSystem);
    }
    if !cfg!(target_arch = "aarch64") {
        return Err(Unavailable::NotAppleSilicon);
    }
    bridge::availability()
}

/// `text` cut to [`MAX_CHARS`] characters (at a character boundary), and whether it was cut.
pub fn truncate(text: &str) -> (&str, bool) {
    match text.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    }
}

/// Why a run gave no result. Holds no text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Error {
    Unavailable,
    Cancelled,
    /// The model's safety guardrails refused the text or its answer.
    Guardrail,
    /// The text does not fit the model's context window.
    TooLong,
    /// The model does not support the language of the text.
    Language,
    Failed,
}

impl Error {
    /// A fixed name of the error, the only thing to put in a log.
    pub fn kind(self) -> &'static str {
        match self {
            Error::Unavailable => "unavailable",
            Error::Cancelled => "cancelled",
            Error::Guardrail => "guardrail",
            Error::TooLong => "too-long",
            Error::Language => "language",
            Error::Failed => "failed",
        }
    }

    /// Read the status code of `mac_ai_job_run` (not 0) in `bridge.swift`.
    #[cfg_attr(not(mac_ai_swift), allow(dead_code))]
    pub(crate) fn from_code(code: i32) -> Self {
        match code {
            1 => Error::Unavailable,
            2 => Error::Cancelled,
            3 => Error::Guardrail,
            4 => Error::TooLong,
            5 => Error::Language,
            _ => Error::Failed,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_codes_map_to_reasons() {
        assert_eq!(decode_availability(0), Ok(()));
        assert_eq!(decode_availability(1), Err(Unavailable::DeviceNotEligible));
        assert_eq!(
            decode_availability(2),
            Err(Unavailable::AppleIntelligenceOff)
        );
        assert_eq!(decode_availability(3), Err(Unavailable::ModelNotReady));
        assert_eq!(decode_availability(5), Err(Unavailable::UnsupportedSystem));
        assert_eq!(decode_availability(4), Err(Unavailable::Other));
        assert_eq!(decode_availability(-7), Err(Unavailable::Other));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn not_macos_is_unavailable() {
        assert_eq!(availability(), Err(Unavailable::UnsupportedSystem));
    }

    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    #[test]
    fn intel_mac_is_unavailable() {
        assert_eq!(availability(), Err(Unavailable::NotAppleSilicon));
    }

    #[cfg(not(mac_ai_swift))]
    #[test]
    fn without_the_bridge_nothing_runs() {
        assert!(availability().is_err());
        let job = Job::new();
        assert_eq!(job.run(Action::Summarize, "hello"), Err(Error::Unavailable));
    }

    /// On a real Mac the answer depends on the machine; it must be a clean answer either way.
    #[cfg(mac_ai_swift)]
    #[test]
    fn availability_answers_without_crashing() {
        let _ = availability();
    }

    #[test]
    fn short_text_is_not_truncated() {
        assert_eq!(truncate("hello"), ("hello", false));
        let exact = "a".repeat(MAX_CHARS);
        assert_eq!(truncate(&exact), (exact.as_str(), false));
    }

    #[test]
    fn long_text_is_cut_at_the_limit() {
        let long = "a".repeat(MAX_CHARS + 5);
        let (cut, was_cut) = truncate(&long);
        assert!(was_cut);
        assert_eq!(cut.chars().count(), MAX_CHARS);
    }

    #[test]
    fn truncation_keeps_multibyte_characters_whole() {
        let long = "é".repeat(MAX_CHARS + 1);
        let (cut, was_cut) = truncate(&long);
        assert!(was_cut);
        assert_eq!(cut.chars().count(), MAX_CHARS);
        assert_eq!(cut.len(), MAX_CHARS * 2);
    }

    #[test]
    fn errors_show_only_their_kind() {
        assert_eq!(Error::from_code(2), Error::Cancelled);
        assert_eq!(Error::from_code(99), Error::Failed);
        assert_eq!(Error::TooLong.to_string(), "too-long");
    }
}
