//! Card data types: launch data, views, shown tables/images and command ids.

use super::*;

/// Formatted code stays whole so the card can color real tokens. Past this, the tail is cut.
pub(crate) const CODE_CAP: usize = 80_000;
/// Lines (table rows) a card shows before "Show all". Enough to read the data, quick to lay out.
pub const PREVIEW_ROWS: usize = 200;
/// Characters a card shows before "Show all", for text with very long lines.
pub const PREVIEW_CHARS: usize = 20_000;
/// Second line of an empty card: a file or text can be dropped on it instead.
pub(crate) fn drop_hint() -> &'static str {
    crate::locale::t("drop_hint")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubjectKind {
    Empty,
    NoText,
    Image,
    Text,
    /// Another app marked the copy private; the card has no text for it.
    Hidden,
    /// Clipboard access is set to Always Deny (pasteboard privacy): nothing is read.
    Denied,
    /// Clipboard access is Default or Ask: a new copy, read once the card is shown.
    Pending,
    /// As [`SubjectKind::Pending`], the first time: the card explains the alert macOS shows
    /// next (once; [`crate::paste_access::ask_note`]).
    PasteAsk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hist {
    pub index: usize,
    pub title: String,
    pub mark: String,
    /// Stable id of the history entry ([`crate::clipboard::ClipboardHistory::id_at`]).
    pub id: u64,
    /// The entry is text, so it can be added to a diff.
    pub text: bool,
    /// Capped text the search matches. Empty for a picture. Not drawn on the menu bar.
    pub search: Zeroizing<String>,
    /// The entry is pinned: its rows show the pin image, not a mark in [`Self::title`].
    pub pinned: bool,
    /// A pinned text's snippet name ([`crate::snippet`]).
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFacts {
    pub format: String,
    pub width: usize,
    pub height: usize,
    pub byte_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchData {
    pub subject_kind: SubjectKind,
    pub subject_text: Option<String>,
    pub image: Option<ImageFacts>,
    pub history: Vec<Hist>,
    pub can_clear_history: bool,
    /// Pins are held, so *Clear pins* is offered.
    pub can_clear_pins: bool,
    /// Recording is paused for this session.
    pub paused: bool,
    /// The next foreign copy will be skipped.
    pub ignore_next: bool,
    /// A frontmost app other than CopyCraft was seen, so it can be ignored.
    pub can_ignore_app: bool,
    /// ScreenCaptureKit is there, so *Take Screenshot…* can run.
    pub can_screenshot: bool,
    /// One line for the card meta, such as a full pin list.
    pub notice: Option<String>,
    /// `<` older and `>` newer, when an earlier copy exists. Absent hides both.
    pub history_nav: Option<HistoryNav>,
    /// The small cards of the history strip; empty when the strip is not shown.
    pub strip: Vec<Thumb>,
    /// The history position on screen (0 is the newest): the card the strip marks.
    pub strip_cursor: usize,
    /// A copy was just removed from history: ⌘Z puts it back ([`CommandId::RestoreHistory`]).
    pub can_restore_history: bool,
    /// The hint under the strip (⌘-click a card to compare), until compare was used once.
    pub strip_hint: StripHint,
    pub theme: Theme,
    /// Settings with a check mark in the `⋯` menu.
    pub settings: crate::settings::Settings,
    /// Which result the card is showing. Original is the clipboard itself.
    pub view: CardView,
    /// Info, a smaller JPEG, recognized text, and QR payloads for an image.
    pub image_scan: Option<ImageScan>,
    /// The card shows a PDF (its text, or the first page of a scan): the footer counts pages.
    pub pdf: Option<crate::pdf::PdfFacts>,
    /// The page of that PDF the Beeld view shows (0-based).
    pub pdf_page: usize,
    /// The zoom of that page (percent; [`crate::pdf::ZOOM_FIT`]: fit in the well), and, above fit,
    /// the page drawn at it. `picture` stays the page at fit.
    pub pdf_zoom: u32,
    pub pdf_zoom_picture: Option<crate::clipboard::SecretBytes>,
    /// The picture is the first page of a document (a .docx, drawn by Quick Look) and its text
    /// is the *Tekst* view. No pages to turn, no PDF footer.
    pub document_preview: bool,
    /// Set when the card is showing a chosen file instead of the clipboard.
    pub source_name: Option<String>,
    /// Well text when that file cannot be shown.
    pub source_note: Option<String>,
    /// "Show all" was chosen: the card renders the whole text, not the preview.
    pub full: bool,
    /// The picture of an image card that is not the clipboard's (a dropped image). Absent: the
    /// card draws the clipboard's picture.
    pub picture: Option<crate::clipboard::SecretBytes>,
    /// The table version the Dataframe view shows, when the text is a table the card has
    /// worked on ([`crate::table`]). Absent: the copied table as it is.
    pub table: Option<TableShown>,
    /// The versions of a picture entry ([`crate::image_edit`]), for every picture that can
    /// have them. Absent: no Image ▾.
    pub image_edit: Option<ImageShown>,
    /// The versions Text ▾ made of the text on the card ([`crate::text_ops`]). Absent until
    /// a step attaches them; the chip still shows on text and code, including a chosen file.
    pub text_edit: Option<TextShown>,
    /// History entry on screen, when the card follows history. Absent for a chosen file.
    pub shown_id: Option<u64>,
    /// "A" or "B" when that entry is chosen for the diff. Not set on the diff card.
    pub shown_mark: Option<&'static str>,
    /// Chosen history ids, oldest first: index 0 is A, index 1 is B.
    pub diff_picks: Vec<u64>,
    /// The diff of those two, when both are chosen.
    pub diff: Option<DiffShown>,
}

/// The diff the card is showing. Not stored with a history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffShown {
    /// Ordered pair and phase ([`crate::diff::token`]). Swap and a finished result change it.
    pub token: u64,
    /// Either side already has a sensitivity label.
    pub sensitive: bool,
    pub labels: crate::sensitivity::Labeling,
    pub phase: crate::diff::Phase,
}

/// Image ▾ on the picture card. A step's result is a new copy (a history entry of its own),
/// like a Text ▾ step's; there are no versions to page through on the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageShown {
    /// The picture's allocation: the copy on the card.
    pub entry: u64,
    /// What the step that made this copy did ("Grayscale"), while it is the newest step's.
    pub label: Option<String>,
    /// ⌘Z shows the copy the step was taken on, ⇧⌘Z the step's copy again (as after a Text ▾
    /// step; only shown, the clipboard is left alone).
    pub can_undo: bool,
    pub can_redo: bool,
    /// A step is being worked out.
    pub working: bool,
    /// Why the last step did nothing, failed or was not kept, for the meta line.
    pub note: Option<String>,
}

/// Text ▾ on the text card. A step's result is a new copy (a history entry of its own), like
/// an Image ▾ step's; there are no versions to page through on the card.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextShown {
    /// ⌘Z shows the copy the step was taken on, ⇧⌘Z the step's copy again (only shown, the
    /// clipboard is left alone).
    pub can_undo: bool,
    pub can_redo: bool,
    /// Why the last step did nothing, for the meta line.
    pub note: Option<String>,
    /// Code in a language with functions: Text ▾ offers Complexity.
    pub code: bool,
    /// Agent instructions: Text ▾ offers Commands, Outline and Check.
    pub agent: bool,
    /// A URL in the text holds a tracking parameter: Text ▾ offers Clean URL….
    pub tracking: bool,
    /// A JSON card (also JSON with a mistake): Text ▾ offers Format JSON….
    pub json: bool,
    /// JSON, YAML, XML or SQL: Text ▾ › Code offers its steps.
    pub code_lang: Option<crate::text_ops::code_style::CodeLang>,
    /// Short enough and no secret: Text ▾ offers QR Code ([`crate::qr_code::offered`]).
    pub qr: bool,
}

/// A table version for the card: its frame, which version it is, and what the versions are.
#[derive(Clone)]
pub struct TableShown {
    /// The version's frame. `None` while it is being worked out.
    pub frame: Option<polars::prelude::DataFrame>,
    /// Which frame that is ([`crate::table::TableVersions::frame_id`]).
    pub frame_id: u64,
    /// The version shown: 0 is the original.
    pub version: usize,
    /// "Original", then the label of each step.
    pub labels: Vec<String>,
    /// A step or another version is being worked out.
    pub working: bool,
    /// Why the last step failed, for the meta line.
    pub error: Option<String>,
    /// How the copied text is read (the date order).
    pub options: dataframe::ReadOptions,
    /// What reading it found, once it was read.
    pub notes: Option<dataframe::ReadNotes>,
    /// The entry shows the column overview (`true`) or the grid; `None` until it is decided
    /// ([`dataframe::shows_overview`]).
    pub overview: Option<bool>,
}

impl TableShown {
    pub fn can_undo(&self) -> bool {
        self.version > 0
    }

    pub fn can_redo(&self) -> bool {
        self.version + 1 < self.labels.len()
    }
}

impl PartialEq for TableShown {
    /// Same version, versions, state and frame (the same frame, not equal values).
    fn eq(&self, other: &Self) -> bool {
        self.frame.is_some() == other.frame.is_some()
            && self.frame_id == other.frame_id
            && self.version == other.version
            && self.labels == other.labels
            && self.working == other.working
            && self.error == other.error
            && self.options == other.options
            && self.notes == other.notes
            && self.overview == other.overview
    }
}

impl Eq for TableShown {}

impl std::fmt::Debug for TableShown {
    /// Shape and version only: the frame holds copied data.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableShown")
            .field("shape", &self.frame.as_ref().map(|df| df.shape()))
            .field("version", &self.version)
            .field("versions", &self.labels.len())
            .field("working", &self.working)
            .finish()
    }
}

/// What the card shows in place of the separate preview window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CardView {
    #[default]
    Original,
    Format,
    Convert,
    Decode,
    /// In-card table grid (Open table opens the window; this view remains for versions).
    Dataframe,
    Schema,
    Sample,
    Info,
    Ocr,
    Qr,
    /// Line diff of two history copies. Not a view of one entry, and not stored.
    Diff,
}

impl CardView {
    pub fn from_command(id: &CommandId) -> Option<Self> {
        Some(match id {
            CommandId::Original => Self::Original,
            CommandId::Format => Self::Format,
            CommandId::Convert => Self::Convert,
            CommandId::Decode => Self::Decode,
            CommandId::Dataframe => Self::Dataframe,
            CommandId::Schema => Self::Schema,
            CommandId::Sample => Self::Sample,
            CommandId::Info => Self::Info,
            CommandId::Ocr => Self::Ocr,
            CommandId::Qr => Self::Qr,
            CommandId::Diff => Self::Diff,
            _ => return None,
        })
    }
}

/// Background result for an image card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageScan {
    pub info: String,
    pub data_url: Option<String>,
    pub ocr: Option<String>,
    pub qr: Option<String>,
}

impl ImageScan {
    /// Overwrite the recognized text, the data URL and the info line.
    pub fn wipe(&mut self) {
        self.info.zeroize();
        for text in [&mut self.data_url, &mut self.ocr, &mut self.qr]
            .into_iter()
            .flatten()
        {
            text.zeroize();
        }
    }
}

/// Bytes the save icon writes.
pub struct SaveFile {
    pub filename: String,
    pub extension: &'static str,
    pub bytes: Vec<u8>,
}

impl Drop for SaveFile {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Which way history can move, where the card is, and how many copies are kept. Index 0 is
/// the newest copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryNav {
    pub can_older: bool,
    pub can_newer: bool,
    /// 1-based place of the copy on screen, counted from the newest (1) like the History menu.
    /// `‹` (previous) counts down toward 1, `›` (next) counts up toward the oldest.
    pub position: usize,
    /// Copies in history, including the one on screen.
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandId {
    Original,
    Visit,
    /// Link card: the link without tracking parameters and `user:password@` on the clipboard.
    CopyCleanLink,
    /// Link card: only the domain on the clipboard.
    CopyDomain,
    /// Text card with one URL in it: open that URL.
    OpenLink,
    Format,
    Convert,
    Decode,
    /// Former chip that switched to [`CardView::Dataframe`]; Open table opens the window.
    #[allow(dead_code)]
    Dataframe,
    Schema,
    Sample,
    Info,
    Ocr,
    Qr,
    /// Show the diff of the two chosen history copies.
    Diff,
    /// Add, compare, or remove history entry `id` ([`crate::diff::menu_for`]).
    DiffToggle(u64),
    /// The same action for the copy on screen. It stays on the `⋯` menu itself.
    DiffShown,
    /// Turn A and B around.
    DiffSwap,
    Copy,
    Save,
    /// Open the picture in Preview (Voorvertoning). The card closes: it floats above other
    /// windows and would cover Preview.
    OpenPreview,
    History(usize),
    /// Step to the previous copy. The history order stays put.
    HistoryOlder,
    /// Step back toward the newest copy.
    HistoryNewer,
    /// Empty the OS pasteboard. Copies still held in memory stay until [`Clear`](CommandId::Clear).
    ClearClipboard,
    ClearHistory,
    /// Save copycraft's own recent log, versions and settings (never content) as a .txt file.
    ExportDiagnostics,
    /// Check Apple Intelligence again and say what is wrong, or that it works.
    CheckAi,
    /// Wipe clipboard copies held in memory, then empty the pasteboard.
    Clear,
    /// Open a file on the card.
    ChooseFile,
    /// Leave the chosen file and show the clipboard again.
    UseClipboard,
    /// Render the whole text on the card instead of its preview.
    ShowAll,
    Appearance(Theme),
    /// `⋯` › Theme ▸: the card theme, as in Settings › Display.
    CardTheme(crate::card_theme::CardTheme),
    /// Show history copy `n` on the card (the strip's cards). Only shows it; nothing is copied.
    HistoryView(usize),
    /// The ✕ on strip card `n`, or ⌫ on the card shown: take that copy out of history (the
    /// clipboard keeps what it holds). [`CommandId::RestoreHistory`] puts it back.
    RemoveHistory(usize),
    /// ⌘Z after [`CommandId::RemoveHistory`]: put the copy back where it was.
    RestoreHistory,
    /// ⌘-click on strip card `n`: compare that copy with the one on screen in
    /// the diff view (the older of the two is A, before).
    StripCompare(usize),
    /// Right-click on strip card `n` › Pin / Unpin: pin that copy, or unpin it when it is
    /// pinned (any card, not only the one on screen).
    StripPin(usize),
    /// Esc or a normal click on the strip while two cards are compared: drop the comparison.
    EndCompare,
    /// ⌘Y: show or hide the history strip.
    ToggleStrip,
    /// Turn the PDF's pages in the Beeld view by this many (−1: back).
    PdfPage(isize),
    /// Zoom the PDF page in the Beeld view (⌘+ ⌘− ⌘0, pinch).
    PdfZoom(crate::pdf::ZoomStep),
    /// How wide the card is.
    WindowSize(crate::settings::WindowSize),
    /// Turn the minute timer on copycraft's sensitive copies on or off.
    ClearSensitive,
    /// Forget history this many minutes after the last copy; 0: no time limit.
    KeepHistory(u32),
    /// A step on the table: a new version after the one shown ([`crate::table`]).
    TableStep(crate::table::TableOp),
    /// ⌘Z: the table version before the one shown, or the copy before a Text ▾ or Image ▾
    /// step's.
    StepBack,
    /// ⇧⌘Z: the table version after the one shown, or the copy after a step's.
    StepForward,
    /// Show table version `n` (0 is the original), from the version capsule's menu.
    TableVersion(usize),
    /// Right-click on a cell of the table window's grid › Copy Cell: that cell's value, by
    /// row and column of the version shown.
    TableCopyCell(usize, usize),
    /// › Copy Row: that row's values, tab-separated.
    TableCopyRow(usize),
    /// › Copy Column: that column's values, one per line.
    TableCopyColumn(usize),
    /// Former card chip; steps live in the table window's Table ▾.
    #[allow(dead_code)]
    TableMenu,
    /// The "Image ▾" chip: the card pops a menu of picture steps ([`image_menu_items`]).
    ImageMenu,
    /// A step on the picture: a new version after the one shown ([`crate::image_edit`]).
    /// Undo, redo and the version capsule use the table's commands.
    ImageStep(crate::image_edit::ImageOp),
    /// Resize › Custom…: asks for a width or a height.
    ImageResizeCustom,
    /// The "Text ▾" chip on a text or code card: the card pops [`text_menu_items`].
    TextMenu,
    /// Text ▾ › Complexity: cognitive complexity per function, in a menu.
    TextComplexity,
    /// Text ▾ › Commands from AGENTS.md on agent instructions: their shell commands as a new copy.
    AgentCommands,
    /// Text ▾ › Heading Outline: the headings, each going to its line.
    AgentOutline,
    /// Text ▾ › Check AGENTS.md: warnings about the instructions, each going to its line.
    AgentCheck,
    /// A row of the Complexity, Outline or Check menu: scroll the card to this (1-based) line.
    GotoLine(usize),
    /// Show or hide the line numbers of plain text cards (the well's numbers button).
    ToggleLineNumbers,
    /// Ask for N, then keep the first N lines as a new copy.
    TextHead,
    /// Ask for N, then keep the last N lines as a new copy.
    TextTail,
    /// The lines in reverse order, as a new copy.
    TextReverse,
    /// Ask for a separator and a column, then keep that column as a new copy.
    TextColumn,
    /// Text ▾ › Filter (regex)…: keep the lines a pattern matches, like grep.
    TextFilter,
    /// Text ▾ › Clean URL… (and the chip on a link card): a preview of the URLs without their
    /// tracking parameters, then Apply to Clipboard as a new copy.
    TextCleanUrl,
    /// Text ▾ › QR Code (and the chip on a link card): the text as a QR code, as a new picture
    /// copy ([`crate::qr_code`]).
    TextQr,
    /// Text ▾ › Format JSON…: Format or Compress and Sort keys with a preview, then Apply to
    /// clipboard as a new copy.
    TextJson,
    /// Text ▾ › Case style › a style: the dialog with that style picked, a preview, then
    /// Apply to Clipboard as a new copy.
    TextCase(crate::text_ops::case_style::CaseStyle),
    /// ⋯ › New: a UUID, the Unix time or ISO 8601 now, as a new card and on the clipboard.
    Generate(crate::generate::Generator),
    /// ⋯ › New › Color from Screen…: the system loupe; the colour as a new card and copy.
    PickColor,
    /// Text ▾ › Convert › Encoding: Base64, URL or hex, either way.
    TextEncode(crate::text_ops::encoding::Codec),
    /// Text ▾ › Convert › Encoding: SHA-256, SHA-1 or MD5 as lowercase hex.
    TextHash(crate::text_ops::hash::HashKind),
    /// Text ▾ › Convert › Code: YAML, XML or SQL formatted or minified, JSON ⇄ YAML, as a new
    /// copy.
    TextCode(crate::text_ops::code_style::CodeOp),
    /// The "AI" chip left of Text ▾ on text and code cards: the card pops [`ai_menu_items`].
    AiMenu,
    /// An Apple Intelligence action (from the AI chip's menu or search): runs on this click only, never on its own. The
    /// result is a preview ([`CommandId::AiApply`] puts it on the clipboard).
    Ai(mac_ai::Action),
    /// A code action on one function of a long or multi-function code card (the index into
    /// [`crate::format::fn_spans::function_spans`]; `AI_FN_WHOLE` is the whole text, `AI_FN_MORE`
    /// the dimmed "and N more" line): only that function goes to the model.
    AiFn(mac_ai::Action, u8),
    AiApply,
    /// The preview's Cancel, Esc, or the card closing: stops the run and drops the result.
    AiCancel,
    /// The preview's Hide button: the card goes to the background, the run and preview stay
    /// (the menu bar icon brings the card back).
    AiMinimize,
    /// A disabled line in the Apple Intelligence menu saying why the actions are off.
    AiUnavailable,
    /// A disabled last line in the Apple Intelligence menu: the actions run on this Mac.
    AiOnDevice,
    /// Read dates that fit both orders as `mm/dd/yyyy` (`true`) or `dd/mm/yyyy`.
    TableDateOrder(bool),
    /// Open the column picker on the card ([`crate::column_picker`]); Apply takes one step.
    TableChooseColumns,
    /// Open the table on the card in its own resizable window ([`table_window_view`]).
    TableOpenWindow,
    /// Open the Settings window (hotkey, date order, blur, Open at Login).
    Settings,
    /// Open the About window (name, version, offline promise).
    About,
    /// Idee voorstellen…: a small form that opens a draft mail ([`crate::mailto`]).
    SuggestFeature,
    /// Probleem melden…: the same form for a problem report.
    ReportProblem,
    /// Open the welcome window again.
    Welcome,
    /// Stop recording foreign copies until it is switched off. Not stored.
    Pause,
    /// Skip the next foreign copy, then record again.
    IgnoreNext,
    /// Add the last other frontmost app to the ignore list.
    IgnoreFrontApp,
    /// Take a screenshot (an area, a window or the whole screen) to the clipboard and history
    /// ([`crate::screenshot`]).
    TakeScreenshot,
    /// Pin the entry on screen.
    Pin,
    /// Unpin the entry on screen.
    Unpin,
    /// ⋯ › Save as Snippet… / Rename Snippet…: name the pinned text on screen ([`crate::snippet`]).
    SnippetName,
    /// Fill In Snippet…: the fill-in panel for the snippet with this history id, then a new copy.
    SnippetFill(u64),
    /// Delete the entry on screen.
    DeleteHistory,
    /// Drop every pin. Unpinned entries stay.
    ClearPins,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    pub title: String,
    pub detail: String,
    pub(crate) keywords: String,
    /// `false` dims the menu item (a non-text copy cannot be added to a diff).
    pub enabled: bool,
    /// A pinned history row: its menu item and chip show the pin ([`crate::commands::PIN_SYMBOL`]),
    /// and VoiceOver reads [`crate::commands::pinned_label`]. The title has no mark.
    pub pinned: bool,
}

impl Command {
    pub fn wipe(&mut self) {
        self.title.zeroize();
        self.detail.zeroize();
        self.keywords.zeroize();
    }

    pub(crate) fn matches(&self, needle: &str) -> bool {
        field_has(&self.title, needle)
            || field_has(&self.detail, needle)
            || field_has(&self.keywords, needle)
    }
}

/// Case-insensitive substring. The lowercase copy is zeroized; it can hold clipboard text.
pub(crate) fn field_has(haystack: &str, needle: &str) -> bool {
    let mut lower = haystack.to_lowercase();
    let hit = lower.contains(needle);
    lower.zeroize();
    hit
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkCard {
    pub title: String,
    pub meta: String,
    pub excerpt: String,
    pub placeholder: String,
    pub shows_image: bool,
    /// Syntax colors for formatted code. The excerpt is that formatted body.
    pub highlight: Option<FormatKind>,
    /// The well text can be selected, so part of a data URL can be copied on its own.
    pub selectable: bool,
    /// The link (a page or a YouTube video) on a link card. Shown as plain text: nothing is
    /// fetched for it.
    pub link_page: Option<String>,
    /// Set when the excerpt is a preview of a longer text, for example
    /// "Showing 200 of 23,220 rows". "Show all" renders the rest.
    pub preview_note: Option<String>,
    /// The line (1-based, in the excerpt) where copied JSON stops parsing; the well marks it.
    pub error_line: Option<usize>,
    /// The colour on a Colour card ([`crate::color`]); the well draws it as a swatch above the
    /// excerpt, which holds the same colour in every notation.
    pub swatch: Option<crate::color::Swatch>,
}

impl WorkCard {
    /// Overwrite the copied text the card holds (the excerpt, the link, and the title and meta,
    /// which can quote it).
    pub fn wipe(&mut self) {
        self.title.zeroize();
        self.meta.zeroize();
        self.excerpt.zeroize();
        self.placeholder.zeroize();
        if let Some(link) = self.link_page.as_mut() {
            link.zeroize();
        }
        if let Some(note) = self.preview_note.as_mut() {
            note.zeroize();
        }
    }
}

impl Drop for LaunchData {
    fn drop(&mut self) {
        if let Some(text) = self.subject_text.as_mut() {
            text.zeroize();
        }
        for thumb in &mut self.strip {
            thumb.wipe();
        }
        for item in &mut self.history {
            item.title.zeroize();
            item.mark.zeroize();
            item.search.zeroize();
        }
        if let Some(notice) = self.notice.as_mut() {
            notice.zeroize();
        }
        if let Some(name) = self.source_name.as_mut() {
            name.zeroize();
        }
        if let Some(note) = self.source_note.as_mut() {
            note.zeroize();
        }
    }
}

impl Drop for ImageScan {
    fn drop(&mut self) {
        self.info.zeroize();
        for text in [&mut self.data_url, &mut self.ocr, &mut self.qr] {
            if let Some(value) = text.as_mut() {
                value.zeroize();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipFrame {
    pub x: f64,
    pub row: usize,
    pub width: f64,
}
