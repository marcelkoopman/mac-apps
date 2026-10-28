//! Diagnostic logging, without content. Every line is a [`LogEvent`]: fixed names, numbers
//! (counts, sizes, durations, indexes) and error kinds. There is no way to log free text: no
//! variant holds a `String` or `&str` (a test guards the enum), and this module is the only
//! caller of `mac_ui::log` (another test guards that). So clipboard and history text, file
//! contents, file names and paths, decoder input and output, regex patterns and table data
//! can never reach the log.
//!
//! Lines go to Apple's unified log (subsystem: the bundle id, see [`SUBSYSTEM`]) and to a
//! ring buffer in memory for *Export Diagnostics…* (`OSLogStore` is not in the bindings this
//! app uses, so the export reads the buffer: the same events, last 24 hours, this run only).

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

pub use mac_ui::log::{Category, Level};

/// Console filter: `subsystem:nl.marcelkoopman.copycraft`.
pub const SUBSYSTEM: &str = "nl.marcelkoopman.copycraft";

/// What went wrong, as a kind and never as the error's text (that can hold a path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Work {
    ImageJob,
    TableJob,
    ShowAll,
    Diff,
    Preview,
    TextStep,
    Lane,
    Complexity,
    Agent,
}

/// Why a copy was not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Skip {
    Paused,
    IgnoreNext,
    IgnoredApp,
    /// Kept out by what is in it ([`crate::sensitivity::Admission::Refuse`]). Which kind of
    /// data that was is not logged.
    Refused,
}

/// Everything copycraft logs. Only fixed names, numbers and kinds: see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogEvent {
    Started,
    StartFailed(ErrorKind),
    BackgroundStartSkipped(ErrorKind),
    PrewarmSkipped(ErrorKind),
    LoginItemFailed(ErrorKind),
    MissingLocaleKey,
    ScreenshotsAllowed(bool),
    /// A screenshot went to the clipboard. Nothing about it: no size, no window, no app.
    ScreenshotCaptured,
    /// A screenshot failed, and the kind of why.
    ScreenshotFailed(crate::screenshot::Failure),
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
    RestoreHistoryFailed(ErrorKind),
    /// The snippets file could not be written. Never a name, a text or a path.
    SnippetsSaveFailed(ErrorKind),
    /// Snippets were read at start or after a wipe: only whether any came back and whether any
    /// were dropped (a secret, a full pin list).
    SnippetsLoaded {
        any: bool,
        dropped: bool,
    },
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
    /// An Apple Intelligence action failed (not a cancel): only the kind of error, never text.
    AiFailed(mac_ai::Error),
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
    /// Debug builds only: what decides the chips of a copied picture card, logged when it
    /// changes. Only yes or no, never content: history kept, the card on history instead of the
    /// clipboard, the clipboard holds a picture, history holds this copy, the picture read for
    /// the card is in, it was asked for, its scan is in, and history refused this copy.
    ImageGate {
        kept_history: bool,
        suppressed: bool,
        image: bool,
        in_history: bool,
        read: bool,
        asked: bool,
        scanned: bool,
        refused: bool,
    },
    /// Debug builds only: how an Image ▾ step and the picture it was taken on went into
    /// history. Only yes or no: the picture was in history, it was the clipboard copy read for
    /// the card, it went in now, its text was known, the text admits it, Pause, held (Pause or
    /// an ignored front app), and the copy of the step went in.
    ImageStepKept {
        source_in_history: bool,
        clipboard_copy: bool,
        source_kept: bool,
        source_checked: bool,
        admitted: bool,
        paused: bool,
        held: bool,
        kept: bool,
    },
    /// Debug builds only: a copied picture went into history at once, before its text was
    /// checked (or after, when the scan of the card was in). Only yes or no.
    ImageAdmitted {
        checked: bool,
    },
    /// Debug builds only: how a screenshot went on the clipboard. Only yes or no: its text was
    /// in within the wait, it was marked Concealed, and its text admits it into history.
    ScreenshotChecked {
        text_in: bool,
        concealed: bool,
        admitted: bool,
    },
    /// Debug builds only: the text of a copied picture history took before its check is in;
    /// whether that took it out again. Only yes or no.
    ImageChecked {
        retracted: bool,
    },
    /// Debug builds only: how a Text ▾ step and the copy it was taken on went into history,
    /// as [`LogEvent::ImageStepKept`]. Only yes or no.
    TextStepKept {
        source_in_history: bool,
        clipboard_copy: bool,
        source_kept: bool,
        admitted: bool,
        paused: bool,
        held: bool,
        kept: bool,
    },
    PreviewNoPicture,
    PreviewStarted,
    PreviewOpenFailed,
    PreviewFailed(ErrorKind),
    SaveFinished {
        ms: u64,
        spinner: bool,
    },
    SaveFailed(ErrorKind),
    /// Text ▾ › Commands from AGENTS.md found this many commands.
    AgentCommands {
        count: usize,
    },
    /// Text ▾ › Heading Outline listed this many headings.
    AgentOutline {
        headings: usize,
    },
    /// Text ▾ › Check AGENTS.md found this many warnings (of any kind: whether one is a
    /// possible secret is not logged).
    AgentChecked {
        warnings: usize,
    },
    /// A copy this large (bytes) was not kept in history: over the per-item limit.
    HistoryItemTooLarge {
        image: bool,
        bytes: usize,
    },
    /// Text ▾ › Complexity refused a text this large (bytes). Not its language.
    ComplexityTooLarge {
        bytes: usize,
    },
    /// The complexity scanner panicked on a text this large (bytes); the panic was caught.
    /// Not its language.
    ComplexityFailed {
        bytes: usize,
    },
    /// Text ▾ › Complexity scored this many functions.
    ComplexityScored {
        functions: usize,
        ms: u64,
    },
    /// Export Diagnostics… wrote this many log lines.
    DiagnosticsExported {
        lines: usize,
    },
    OpenPanelFailed(ErrorKind),
    SymbolCopyFailed(ErrorKind),
    VisitRefused,
    VisitFailed,
}

impl LogEvent {
    pub fn category(self) -> Category {
        use LogEvent as E;
        match self {
            E::ScreenshotCaptured
            | E::ScreenshotFailed(_)
            | E::CopySkipped(_)
            | E::ClipboardUnreadable
            | E::PasteAccess { .. }
            | E::CopyFailed(_)
            | E::ClearClipboardFailed(_)
            | E::SensitiveCleared
            | E::ClearSensitiveFailed(_)
            | E::FormatLinkFailed(_)
            | E::HistoryForgottenIdle { .. }
            | E::HistoryForgottenLock
            | E::RestoreHistoryFailed(_)
            | E::SnippetsSaveFailed(_)
            | E::SnippetsLoaded { .. }
            | E::SensitivityNotStarted(_)
            | E::SensitivityChecked { .. }
            | E::HistoryItemTooLarge { .. } => Category::Clipboard,
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
            | E::ImageGate { .. }
            | E::ImageStepKept { .. }
            | E::ImageAdmitted { .. }
            | E::ImageChecked { .. }
            | E::ScreenshotChecked { .. }
            | E::TextStepKept { .. }
            | E::PreviewNoPicture
            | E::PreviewStarted
            | E::PreviewOpenFailed
            | E::PreviewFailed(_)
            | E::SaveFinished { .. }
            | E::DiagnosticsExported { .. }
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
            | E::ComplexityScored { .. }
            | E::ComplexityTooLarge { .. }
            | E::ComplexityFailed { .. }
            | E::AgentCommands { .. }
            | E::AgentOutline { .. }
            | E::AgentChecked { .. }
            | E::AiFailed(_)
            | E::WorkNotStarted(..) => Category::Worker,
        }
    }

    /// Levels are chosen on purpose: routine events are debug or info, only real failures are
    /// error, and a bug in copycraft itself is fault.
    pub fn level(self) -> Level {
        use LogEvent as E;
        match self {
            // Broken invariants: the strings table or start-up itself.
            E::MissingLocaleKey | E::StartFailed(_) => Level::Fault,
            // Something the user asked for did not happen.
            E::LoginItemFailed(_)
            | E::HotkeyRegisterFailed(_)
            | E::SensitivityNotStarted(_)
            | E::CopyFailed(_)
            | E::ClearClipboardFailed(_)
            | E::ClearSensitiveFailed(_)
            | E::FormatLinkFailed(_)
            | E::RestoreHistoryFailed(_)
            | E::SnippetsSaveFailed(_)
            | E::TextStepFailed(_)
            | E::AiFailed(_)
            | E::WorkNotStarted(..)
            | E::PreviewOpenFailed
            | E::PreviewFailed(_)
            | E::SaveFailed(_)
            | E::OpenPanelFailed(_)
            | E::SymbolCopyFailed(_)
            | E::ScreenshotFailed(_)
            | E::VisitFailed => Level::Error,
            E::ComplexityFailed { .. } => Level::Fault,
            // Cosmetic or expected: worth a note, not an error.
            E::HotkeyUnregisterFailed(_)
            | E::IconBlinkFailed(_)
            | E::TooltipFailed(_)
            | E::BackgroundStartSkipped(_)
            | E::PrewarmSkipped(_)
            | E::DecodePictureSkipped(_)
            | E::DroppedImageScanFailed(_)
            | E::ClipboardUnreadable
            | E::ImageStepRefused
            | E::SvgExternalRefs
            | E::VisitRefused
            | E::TableNotShown
            | E::PreviewNoPicture
            | E::Started
            | E::ScreenshotsAllowed(_)
            | E::ScreenshotCaptured
            | E::HotkeyRegistered
            | E::PasteAccess { .. }
            | E::SensitiveCleared
            | E::HistoryForgottenIdle { .. }
            | E::HistoryForgottenLock
            | E::TableOpened { .. }
            | E::PreviewStarted
            | E::SaveFinished { .. }
            | E::DiagnosticsExported { .. }
            | E::HistoryItemTooLarge { .. } => Level::Info,
            // Routine, per copy or per job.
            E::CopySkipped(_)
            | E::SnippetsLoaded { .. }
            | E::SensitivityChecked { .. }
            | E::TableJobDone { .. }
            | E::ImageJobDone { .. }
            | E::DiffBuilt { .. }
            | E::ShowAllBuilt { .. }
            | E::ComplexityScored { .. }
            | E::ComplexityTooLarge { .. }
            | E::AgentCommands { .. }
            | E::AgentOutline { .. }
            | E::AgentChecked { .. }
            | E::SpinnerShown { .. }
            | E::SpinnerHidden
            | E::ImageGate { .. }
            | E::ImageStepKept { .. }
            | E::ImageAdmitted { .. }
            | E::ImageChecked { .. }
            | E::ScreenshotChecked { .. }
            | E::TextStepKept { .. } => Level::Debug,
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
            E::ScreenshotCaptured => "screenshot: captured".into(),
            E::ScreenshotFailed(why) => format!("screenshot: failed ({})", why.as_str()),
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
            E::RestoreHistoryFailed(k) => format!("restore history failed: {}", k.describe()),
            E::SnippetsSaveFailed(k) => format!("snippets save failed: {}", k.describe()),
            E::SnippetsLoaded { any, dropped } => {
                format!("snippets loaded: any={any} dropped={dropped}")
            }
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
            E::AiFailed(e) => format!("apple intelligence failed: {}", e.kind()),
            E::DiffBuilt { ms } => format!("diff built in {ms} ms"),
            E::ShowAllBuilt { ms } => format!("show all built in {ms} ms"),
            E::WorkNotStarted(work, k) => format!("{work:?} failed to start: {}", k.describe()),
            E::SpinnerShown { ms } => format!("spinner shown after {ms} ms"),
            E::SpinnerHidden => "spinner hidden".into(),
            E::ImageGate {
                kept_history,
                suppressed,
                image,
                in_history,
                read,
                asked,
                scanned,
                refused,
            } => format!(
                "image gate: history {kept_history}, suppressed {suppressed}, image {image}, \
                 in history {in_history}, read {read}, asked {asked}, scanned {scanned}, \
                 refused {refused}"
            ),
            E::ImageStepKept {
                source_in_history,
                clipboard_copy,
                source_kept,
                source_checked,
                admitted,
                paused,
                held,
                kept,
            } => format!(
                "image step kept: source in history {source_in_history}, clipboard copy \
                 {clipboard_copy}, source kept {source_kept}, source checked {source_checked}, \
                 admitted {admitted}, paused {paused}, held {held}, kept {kept}"
            ),
            E::ImageAdmitted { checked } => {
                format!("image admitted at once: checked {checked}")
            }
            E::ImageChecked { retracted } => format!("image checked: retracted {retracted}"),
            E::ScreenshotChecked {
                text_in,
                concealed,
                admitted,
            } => format!(
                "screenshot checked: text in {text_in}, concealed {concealed}, admitted {admitted}"
            ),
            E::TextStepKept {
                source_in_history,
                clipboard_copy,
                source_kept,
                admitted,
                paused,
                held,
                kept,
            } => format!(
                "text step kept: source in history {source_in_history}, clipboard copy \
                 {clipboard_copy}, source kept {source_kept}, admitted {admitted}, paused \
                 {paused}, held {held}, kept {kept}"
            ),
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
            E::AgentCommands { count } => format!("agent commands: {count}"),
            E::AgentOutline { headings } => format!("agent outline: {headings} headings"),
            E::AgentChecked { warnings } => format!("agent check: {warnings} warnings"),
            E::HistoryItemTooLarge { image, bytes } => {
                let kind = if image { "image" } else { "text" };
                format!("history: {kind} too large to keep ({bytes} bytes)")
            }
            E::ComplexityTooLarge { bytes } => format!("complexity: too large ({bytes} bytes)"),
            E::ComplexityFailed { bytes } => format!("complexity: scanner failed ({bytes} bytes)"),
            E::ComplexityScored { functions, ms } => {
                format!("complexity: {functions} functions in {ms} ms")
            }
            E::DiagnosticsExported { lines } => format!("diagnostics exported: {lines} lines"),
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
    let mut line = event.line();
    if matches!(event.level(), Level::Error | Level::Fault) {
        let admitted = LIMITER
            .lock()
            .map_or(Some(0), |mut limiter| limiter.admit(event, Instant::now()));
        let Some(suppressed) = admitted else {
            return;
        };
        if suppressed > 0 {
            line.push_str(&format!(" ({suppressed} identical suppressed)"));
        }
    }
    mac_ui::log::write(event.category(), event.level(), &line);
    remember(SystemTime::now(), event);
}

/// The same error (event and kind) at most once per this long.
pub const REPEAT_WINDOW: Duration = Duration::from_secs(60);

/// Rate limit for repeated identical errors: the first goes through, repeats within
/// [`REPEAT_WINDOW`] are counted, and the next one after the window carries that count.
#[derive(Default)]
pub struct Limiter {
    seen: HashMap<LogEvent, (Instant, u32)>,
}

impl Limiter {
    /// `Some(suppressed since the last one)` when `event` may be written at `now`, else `None`.
    pub fn admit(&mut self, event: LogEvent, now: Instant) -> Option<u32> {
        match self.seen.get_mut(&event) {
            Some((last, suppressed)) if now.duration_since(*last) < REPEAT_WINDOW => {
                *suppressed += 1;
                None
            }
            Some((last, suppressed)) => {
                *last = now;
                Some(std::mem::take(suppressed))
            }
            None => {
                if self.seen.len() >= 256 {
                    self.seen.clear();
                }
                self.seen.insert(event, (now, 0));
                Some(0)
            }
        }
    }
}

static LIMITER: LazyLock<Mutex<Limiter>> = LazyLock::new(|| Mutex::new(Limiter::default()));

pub(crate) fn remember(at: SystemTime, event: LogEvent) {
    if let Ok(mut ring) = RING.lock() {
        ring.push_back((at, event));
        while ring.len() > KEEP {
            ring.pop_front();
        }
    }
}

/// How far back the export goes.
pub const EXPORT_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// The export's log part: one line per event of the last [`EXPORT_WINDOW`] before `now`,
/// oldest first: seconds since the epoch, level, category, line.
pub fn recent_lines(now: SystemTime) -> Vec<String> {
    let since = now
        .checked_sub(EXPORT_WINDOW)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let Ok(ring) = RING.lock() else {
        return Vec::new();
    };
    ring.iter()
        .filter(|(at, _)| *at >= since)
        .map(|(at, event)| {
            let secs = at
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            format!(
                "{secs}  {:<5}  {:<9}  {}",
                event.level().name(),
                event.category().name(),
                event.line()
            )
        })
        .collect()
}

/// The whole diagnostics file: app and macOS version, the settings (no content), then the log.
pub fn export_text(macos: &str, now: SystemTime) -> String {
    let settings = crate::settings::load();
    let mut out = String::new();
    out.push_str("CopyCraft diagnostics\n");
    out.push_str("No clipboard content, history, file names or table data are included.\n\n");
    out.push_str(&format!("app version: {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("macOS: {macos}\n"));
    out.push_str(&format!("subsystem: {SUBSYSTEM}\n\n"));
    out.push_str("settings:\n");
    for (name, value) in [
        ("clipboard_history", settings.clipboard_history.to_string()),
        ("privacy_filter", settings.privacy_filter.to_string()),
        ("blur", settings.blur.to_string()),
        ("clear_sensitive", settings.clear_sensitive.to_string()),
        ("history_minutes", settings.history_minutes.to_string()),
        ("date_month_first", settings.date_month_first.to_string()),
        ("custom_hotkey", settings.hotkey.is_some().to_string()),
        ("language", format!("{:?}", settings.language)),
        ("page_long_text", settings.page_long_text.to_string()),
        (
            "announce_confirmations",
            settings.announce_confirmations.to_string(),
        ),
        ("confirm_clear", settings.confirm_clear.to_string()),
    ] {
        out.push_str(&format!("  {name}: {value}\n"));
    }
    out.push_str("\nlog (last 24 h of this run; time in seconds since 1970, UTC):\n");
    for line in recent_lines(now) {
        out.push_str(&line);
        out.push('\n');
    }
    out
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
    fn every_image_view_leaves_drops_to_the_card() {
        // An NSImageView of its own would take a drag over a shown picture and refuse it: a
        // drop then worked once. `mac_ui::widgets::image_view` unregisters its drag types.
        for (path, text) in sources() {
            let code = without_strings(&text);
            for made in [
                "NSImageView::alloc(",
                "NSImageView::new(",
                "imageViewWithImage",
            ] {
                assert!(
                    !code.contains(made),
                    "{path}: use mac_ui::widgets::image_view"
                );
            }
        }
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
        // A screenshot: that it was taken, or the kind of failure. Nothing about the screen.
        assert_eq!(LogEvent::ScreenshotCaptured.line(), "screenshot: captured");
        let checked = LogEvent::ScreenshotChecked {
            text_in: true,
            concealed: false,
            admitted: true,
        };
        assert_eq!(
            checked.line(),
            "screenshot checked: text in true, concealed false, admitted true"
        );
        assert_eq!(checked.level(), Level::Debug);
        assert_eq!(
            LogEvent::ScreenshotFailed(crate::screenshot::Failure::NotAllowed).line(),
            "screenshot: failed (not allowed)"
        );
        assert_eq!(
            LogEvent::ScreenshotFailed(crate::screenshot::Failure::NoPicture).level(),
            Level::Error
        );
    }

    #[test]
    fn the_export_has_versions_settings_and_recent_events_only() {
        let now = SystemTime::now();
        remember(
            now - Duration::from_secs(25 * 60 * 60),
            LogEvent::HotkeyRegistered,
        );
        remember(
            now,
            LogEvent::TableOpened {
                columns: 2,
                rows: 5,
            },
        );
        let text = export_text("Version 26.0", now);
        assert!(text.contains("No clipboard content"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("macOS: Version 26.0"));
        assert!(text.contains("privacy_filter:"));
        assert!(text.contains("table opened: 2 cols, 5 rows"));
        let old = now - Duration::from_secs(25 * 60 * 60);
        let old_secs = old
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            !text.contains(&format!("{old_secs}  ")),
            "older than 24 h is left out"
        );
    }

    #[test]
    fn repeated_identical_errors_go_out_once_a_minute_with_the_suppressed_count() {
        let mut limiter = Limiter::default();
        let t0 = Instant::now();
        let e = LogEvent::SaveFailed(ErrorKind::from_text("os error 13"));
        assert_eq!(limiter.admit(e, t0), Some(0));
        assert_eq!(limiter.admit(e, t0 + Duration::from_secs(1)), None);
        assert_eq!(limiter.admit(e, t0 + Duration::from_secs(59)), None);
        assert_eq!(limiter.admit(e, t0 + Duration::from_secs(60)), Some(2));
        assert_eq!(limiter.admit(e, t0 + Duration::from_secs(61)), None);
        assert_eq!(limiter.admit(e, t0 + Duration::from_secs(200)), Some(1));
    }

    #[test]
    fn a_different_event_or_kind_is_not_held_back() {
        let mut limiter = Limiter::default();
        let t0 = Instant::now();
        let a = LogEvent::SaveFailed(ErrorKind::from_text("os error 13"));
        let b = LogEvent::SaveFailed(ErrorKind::from_text("os error 28"));
        assert_eq!(limiter.admit(a, t0), Some(0));
        assert_eq!(limiter.admit(b, t0), Some(0));
        assert_eq!(limiter.admit(LogEvent::VisitFailed, t0), Some(0));
        assert_eq!(limiter.admit(a, t0), None);
    }

    #[test]
    fn the_limiter_stays_bounded() {
        let mut limiter = Limiter::default();
        let t0 = Instant::now();
        for ms in 0..1000u64 {
            limiter.admit(LogEvent::SaveFinished { ms, spinner: false }, t0);
        }
        assert!(limiter.seen.len() <= 256);
    }

    #[test]
    fn only_failures_are_errors_and_routine_events_are_quiet() {
        assert_eq!(LogEvent::Started.level(), Level::Info);
        assert_eq!(LogEvent::CopySkipped(Skip::Paused).level(), Level::Debug);
        assert_eq!(LogEvent::SpinnerHidden.level(), Level::Debug);
        assert_eq!(
            LogEvent::TooltipFailed(ErrorKind::from_text("")).level(),
            Level::Info
        );
        assert_eq!(
            LogEvent::SaveFailed(ErrorKind::from_text("")).level(),
            Level::Error
        );
        assert_eq!(LogEvent::MissingLocaleKey.level(), Level::Fault);
    }
}
