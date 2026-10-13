//! Diagnostic logging, without content. Every line is a [`LogEvent`]: fixed names, numbers
//! (counts, sizes, durations, indexes) and error kinds. There is no way to log free text: no
//! variant holds a `String` or `&str` (a test guards the enum), and this module is the only
//! caller of `mac_ui::log` (another test guards that). So clipboard and history text, file
//! contents, file names and paths, decoder input and output, regex patterns and table data
//! can never reach the log.
//!
//! Lines go to Apple's unified log (subsystem: the bundle id, see [`SUBSYSTEM`]) and to a
//! ring buffer in memory for *Export diagnostics…* (`OSLogStore` is not in the bindings this
//! app uses, so the export reads the buffer: the same events, last 24 hours, this run only).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::SystemTime;

pub use mac_ui::log::{Category, Level};

/// Console filter: `subsystem:com.github.marcelkoopman.mac-apps.copycraft`.
pub const SUBSYSTEM: &str = "com.github.marcelkoopman.mac-apps.copycraft";

/// What went wrong, as a kind and never as the error's text (that can hold a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    InvalidData,
    InvalidInput,
    TimedOut,
    Interrupted,
    Unsupported,
    OutOfMemory,
    /// An OS error code (`errno`, an `NSError` code) and nothing else.
    Os(i32),
    Other,
}

impl ErrorKind {
    pub fn io(error: &std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        if let Some(code) = error.raw_os_error() {
            return Self::Os(code);
        }
        match error.kind() {
            K::NotFound => Self::NotFound,
            K::PermissionDenied => Self::PermissionDenied,
            K::AlreadyExists => Self::AlreadyExists,
            K::InvalidData => Self::InvalidData,
            K::InvalidInput => Self::InvalidInput,
            K::TimedOut => Self::TimedOut,
            K::Interrupted => Self::Interrupted,
            K::Unsupported => Self::Unsupported,
            K::OutOfMemory => Self::OutOfMemory,
            _ => Self::Other,
        }
    }

    /// The kind of an `anyhow` error: the first I/O error in its chain, else [`Self::Other`].
    pub fn of(error: &anyhow::Error) -> Self {
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<std::io::Error>())
            .map_or_else(|| Self::from_text(&format!("{error:#}")), Self::io)
    }

    /// The kind of an error that only exists as text: just an `os error N` / `code N` number
    /// when the text has one. The text itself is dropped.
    pub fn from_text(text: &str) -> Self {
        for marker in ["os error ", "error ", "code ", "Code="] {
            if let Some(at) = text.find(marker) {
                let digits: String = text[at + marker.len()..]
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
                    .collect();
                if let Ok(code) = digits.parse() {
                    return Self::Os(code);
                }
            }
        }
        Self::Other
    }

    fn describe(self) -> String {
        match self {
            Self::Os(code) => format!("os error {code}"),
            other => format!("{other:?}"),
        }
    }
}

/// Background jobs that can fail to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    ImageJob,
    TableJob,
    ShowAll,
    Diff,
    Preview,
    TextStep,
    Lane,
}

/// Why a copy was not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Paused,
    IgnoreNext,
    IgnoredApp,
}

/// Everything copycraft logs. Only fixed names, numbers and kinds: see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEvent {
    Started,
    StartFailed(ErrorKind),
    BackgroundStartSkipped(ErrorKind),
    PrewarmSkipped(ErrorKind),
    LoginItemFailed(ErrorKind),
    MissingLocaleKey,
    ScreenshotsAllowed(bool),
    HotkeyRegistered,
    HotkeyRegisterFailed(ErrorKind),
    HotkeyUnregisterFailed(ErrorKind),
    IconBlinkFailed(ErrorKind),
    TooltipFailed(ErrorKind),
    SensitivityNotStarted(ErrorKind),
    SensitivityChecked {
        kb: u64,
        ms: u64,
    },
    CopySkipped(Skip),
    ClipboardUnreadable,
    PasteAccess {
        behavior: Option<crate::paste_access::AccessBehavior>,
        changed: bool,
    },
    CopyFailed(ErrorKind),
    ClearClipboardFailed(ErrorKind),
    SensitiveCleared,
    ClearSensitiveFailed(ErrorKind),
    FormatLinkFailed(ErrorKind),
    HistoryForgottenIdle {
        minutes: u32,
    },
    HistoryForgottenLock,
    HistoryStep {
        index: u64,
        micros: u64,
    },
    RestoreHistoryFailed(ErrorKind),
    TableOpened {
        columns: u64,
        rows: u64,
    },
    TableNotShown,
    TableJobDone {
        ms: u64,
    },
    ImageJobDone {
        ms: u64,
    },
    ImageStepRefused,
    DecodePictureSkipped(ErrorKind),
    DroppedImageScanFailed(ErrorKind),
    SvgExternalRefs,
    TextStepFailed(ErrorKind),
    DiffBuilt {
        ms: u64,
    },
    ShowAllBuilt {
        ms: u64,
    },
    WorkNotStarted(Work, ErrorKind),
    SpinnerShown {
        ms: u64,
    },
    SpinnerHidden,
    PreviewNoPicture,
    PreviewStarted,
    PreviewOpenFailed,
    PreviewFailed(ErrorKind),
    SaveFinished {
        ms: u64,
        spinner: bool,
    },
    SaveFailed(ErrorKind),
    OpenPanelFailed(ErrorKind),
    SymbolCopyFailed(ErrorKind),
    VisitRefused,
    VisitFailed,
}

impl LogEvent {
    pub fn category(self) -> Category {
        use LogEvent as E;
        match self {
            E::CopySkipped(_)
            | E::ClipboardUnreadable
            | E::PasteAccess { .. }
            | E::CopyFailed(_)
            | E::ClearClipboardFailed(_)
            | E::SensitiveCleared
            | E::ClearSensitiveFailed(_)
            | E::FormatLinkFailed(_)
            | E::HistoryForgottenIdle { .. }
            | E::HistoryForgottenLock
            | E::HistoryStep { .. }
            | E::RestoreHistoryFailed(_)
            | E::SensitivityNotStarted(_)
            | E::SensitivityChecked { .. } => Category::Clipboard,
            E::TableOpened { .. } | E::TableNotShown | E::TableJobDone { .. } => Category::Table,
            E::DecodePictureSkipped(_) | E::SvgExternalRefs => Category::Decode,
            E::MissingLocaleKey
            | E::ScreenshotsAllowed(_)
            | E::HotkeyRegistered
            | E::HotkeyRegisterFailed(_)
            | E::HotkeyUnregisterFailed(_) => Category::Settings,
            E::LoginItemFailed(_) => Category::Login,
            E::Started
            | E::StartFailed(_)
            | E::IconBlinkFailed(_)
            | E::TooltipFailed(_)
            | E::SpinnerShown { .. }
            | E::SpinnerHidden
            | E::PreviewNoPicture
            | E::PreviewStarted
            | E::PreviewOpenFailed
            | E::PreviewFailed(_)
            | E::SaveFinished { .. }
            | E::SaveFailed(_)
            | E::OpenPanelFailed(_)
            | E::SymbolCopyFailed(_)
            | E::VisitRefused
            | E::VisitFailed => Category::Window,
            E::BackgroundStartSkipped(_)
            | E::PrewarmSkipped(_)
            | E::ImageJobDone { .. }
            | E::ImageStepRefused
            | E::DroppedImageScanFailed(_)
            | E::TextStepFailed(_)
            | E::DiffBuilt { .. }
            | E::ShowAllBuilt { .. }
            | E::WorkNotStarted(..) => Category::Worker,
        }
    }

    pub fn level(self) -> Level {
        use LogEvent as E;
        match self {
            E::StartFailed(_)
            | E::LoginItemFailed(_)
            | E::HotkeyRegisterFailed(_)
            | E::HotkeyUnregisterFailed(_)
            | E::IconBlinkFailed(_)
            | E::TooltipFailed(_)
            | E::SensitivityNotStarted(_)
            | E::CopyFailed(_)
            | E::ClearClipboardFailed(_)
            | E::ClearSensitiveFailed(_)
            | E::FormatLinkFailed(_)
            | E::RestoreHistoryFailed(_)
            | E::TextStepFailed(_)
            | E::WorkNotStarted(..)
            | E::PreviewOpenFailed
            | E::PreviewFailed(_)
            | E::SaveFailed(_)
            | E::OpenPanelFailed(_)
            | E::SymbolCopyFailed(_)
            | E::VisitFailed
            | E::MissingLocaleKey => Level::Error,
            E::SensitivityChecked { .. }
            | E::HistoryStep { .. }
            | E::TableJobDone { .. }
            | E::ImageJobDone { .. }
            | E::DiffBuilt { .. }
            | E::ShowAllBuilt { .. }
            | E::SpinnerShown { .. }
            | E::SpinnerHidden => Level::Debug,
            _ => Level::Info,
        }
    }

    /// The line: the event's name and its numbers or kinds, nothing else.
    pub fn line(self) -> String {
        use LogEvent as E;
        match self {
            E::Started => "started".into(),
            E::StartFailed(k) => format!("start failed: {}", k.describe()),
            E::BackgroundStartSkipped(k) => {
                format!("background start-up skipped: {}", k.describe())
            }
            E::PrewarmSkipped(k) => format!("prewarm skipped: {}", k.describe()),
            E::LoginItemFailed(k) => format!("login item failed: {}", k.describe()),
            E::MissingLocaleKey => "missing locale key".into(),
            E::ScreenshotsAllowed(on) => {
                format!("allow screenshots: {}", if on { "on" } else { "off" })
            }
            E::HotkeyRegistered => "hotkey registered".into(),
            E::HotkeyRegisterFailed(k) => format!("hotkey register failed: {}", k.describe()),
            E::HotkeyUnregisterFailed(k) => format!("hotkey unregister failed: {}", k.describe()),
            E::IconBlinkFailed(k) => format!("menu bar icon blink failed: {}", k.describe()),
            E::TooltipFailed(k) => format!("menu bar tooltip failed: {}", k.describe()),
            E::SensitivityNotStarted(k) => {
                format!("sensitivity checker not started: {}", k.describe())
            }
            E::SensitivityChecked { kb, ms } => format!("sensitivity checked: {kb} KB in {ms} ms"),
            E::CopySkipped(why) => format!("copy skipped: {why:?}"),
            E::ClipboardUnreadable => {
                "clipboard text unreadable (denied?), not read again until the next copy".into()
            }
            E::PasteAccess { behavior, changed } => format!(
                "pasteboard access behavior{}: {}",
                if changed { " now" } else { "" },
                behavior.map_or_else(
                    || "not available (before macOS 15.4)".to_string(),
                    |b| b.name()
                )
            ),
            E::CopyFailed(k) => format!("copy failed: {}", k.describe()),
            E::ClearClipboardFailed(k) => format!("clear clipboard failed: {}", k.describe()),
            E::SensitiveCleared => "sensitive copy cleared after a minute".into(),
            E::ClearSensitiveFailed(k) => format!("clear sensitive copy failed: {}", k.describe()),
            E::FormatLinkFailed(k) => format!("format link failed: {}", k.describe()),
            E::HistoryForgottenIdle { minutes } => {
                format!("history forgotten {minutes} min after the last copy")
            }
            E::HistoryForgottenLock => "history forgotten (lock, sleep or user switch)".into(),
            E::HistoryStep { index, micros } => format!("history step to {index} in {micros} µs"),
            E::RestoreHistoryFailed(k) => format!("restore history failed: {}", k.describe()),
            E::TableOpened { columns, rows } => {
                format!("table opened: {columns} cols, {rows} rows")
            }
            E::TableNotShown => "open table: the card shows no table".into(),
            E::TableJobDone { ms } => format!("table job done in {ms} ms"),
            E::ImageJobDone { ms } => format!("image job done in {ms} ms"),
            E::ImageStepRefused => "image step refused outside the picture view".into(),
            E::DecodePictureSkipped(k) => format!("decode picture skipped: {}", k.describe()),
            E::DroppedImageScanFailed(k) => format!("dropped image scan failed: {}", k.describe()),
            E::SvgExternalRefs => "svg has outside references, shown as code".into(),
            E::TextStepFailed(k) => format!("text step failed: {}", k.describe()),
            E::DiffBuilt { ms } => format!("diff built in {ms} ms"),
            E::ShowAllBuilt { ms } => format!("show all built in {ms} ms"),
            E::WorkNotStarted(work, k) => format!("{work:?} failed to start: {}", k.describe()),
            E::SpinnerShown { ms } => format!("spinner shown after {ms} ms"),
            E::SpinnerHidden => "spinner hidden".into(),
            E::PreviewNoPicture => "preview: no picture to open".into(),
            E::PreviewStarted => "preview started".into(),
            E::PreviewOpenFailed => "preview: could not open Preview".into(),
            E::PreviewFailed(k) => format!("preview failed: {}", k.describe()),
            E::SaveFinished { ms, spinner } => format!(
                "save finished in {ms} ms ({})",
                if spinner {
                    "spinner shown"
                } else {
                    "no spinner"
                }
            ),
            E::SaveFailed(k) => format!("save failed: {}", k.describe()),
            E::OpenPanelFailed(k) => format!("open panel failed: {}", k.describe()),
            E::SymbolCopyFailed(k) => format!("symbol copy failed: {}", k.describe()),
            E::VisitRefused => "visit refused: only http and https links are opened".into(),
            E::VisitFailed => "visit failed: the system could not open the link".into(),
        }
    }
}

/// Lines kept for the export.
const KEEP: usize = 4000;
pub(crate) static RING: Mutex<VecDeque<(SystemTime, LogEvent)>> = Mutex::new(VecDeque::new());

/// Log `event` (unified log and the export buffer).
pub fn log(event: LogEvent) {
    mac_ui::log::write(event.category(), event.level(), &event.line());
    remember(SystemTime::now(), event);
}

pub(crate) fn remember(at: SystemTime, event: LogEvent) {
    if let Ok(mut ring) = RING.lock() {
        ring.push_back((at, event));
        while ring.len() > KEEP {
            ring.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn copy_only<T: Copy>() {}
    // Compile-time guard: an event is `Copy`, so it cannot own a `String`, `Vec` or `PathBuf`.
    const _: () = copy_only::<LogEvent>();

    /// The source of `name` in this crate, for the guards below.
    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path.display().to_string(), text));
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut out,
        );
        out
    }

    /// `text` with the insides of `"…"` literals dropped (escapes skipped), for the guards.
    fn without_strings(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut inside = false;
        let mut escaped = false;
        for ch in text.chars() {
            if inside {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    inside = false;
                    out.push(ch);
                }
            } else {
                if ch == '"' {
                    inside = true;
                }
                out.push(ch);
            }
        }
        out
    }

    #[test]
    fn events_hold_no_text() {
        let src = include_str!("diag.rs");
        let start = src.find("pub enum LogEvent {").unwrap();
        let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
        for banned in [
            "str", "String", "Path", "OsStr", "Vec", "Box", "dyn ", "Cow", "'",
        ] {
            assert!(!body.contains(banned), "LogEvent may not hold {banned}");
        }
        for name in ["pub enum ErrorKind {", "pub enum Work {", "pub enum Skip {"] {
            let start = src.find(name).unwrap();
            let body = &src[start..start + src[start..].find("\n}\n").unwrap()];
            assert!(!body.contains("str") && !body.contains("String"), "{name}");
        }
    }

    #[test]
    fn only_this_module_writes_the_log_and_nothing_prints() {
        for (path, text) in sources() {
            let own = path.ends_with("src/diag.rs");
            if !own {
                assert!(
                    !text.contains("mac_ui::log::write"),
                    "{path} writes the log directly"
                );
            }
            // Test-only modules may print; elsewhere only outside string literals counts.
            if own || path.ends_with("testdata_manifest.rs") {
                continue;
            }
            let code = without_strings(&text);
            for banned in ["eprintln!", "println!", "dbg!", "eprint!("] {
                assert!(!code.contains(banned), "{path} uses {banned}");
            }
        }
    }

    #[test]
    fn error_kinds_keep_codes_and_drop_text() {
        let denied = std::io::Error::from_raw_os_error(13);
        assert_eq!(ErrorKind::io(&denied), ErrorKind::Os(13));
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "/Users/marcel/secret.csv");
        assert_eq!(ErrorKind::io(&missing), ErrorKind::NotFound);
        let wrapped = anyhow::Error::new(missing).context("cannot read /Users/marcel/secret.csv");
        assert_eq!(ErrorKind::of(&wrapped), ErrorKind::NotFound);
        assert_eq!(
            ErrorKind::from_text("write /tmp/x: Permission denied (os error 13)"),
            ErrorKind::Os(13)
        );
        assert_eq!(
            ErrorKind::from_text("could not write /Users/marcel/a b.txt"),
            ErrorKind::Other
        );
        let line = LogEvent::SaveFailed(ErrorKind::of(&wrapped)).line();
        assert_eq!(line, "save failed: NotFound");
        assert!(!line.contains("marcel"));
    }

    #[test]
    fn lines_are_names_and_numbers() {
        assert_eq!(
            LogEvent::TableOpened {
                columns: 3,
                rows: 192
            }
            .line(),
            "table opened: 3 cols, 192 rows"
        );
        assert_eq!(
            LogEvent::LoginItemFailed(ErrorKind::Os(-25)).line(),
            "login item failed: os error -25"
        );
        assert_eq!(
            LogEvent::TableOpened {
                columns: 1,
                rows: 1
            }
            .category(),
            Category::Table
        );
        assert_eq!(LogEvent::CopyFailed(ErrorKind::Other).level(), Level::Error);
    }
}
