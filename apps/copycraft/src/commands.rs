use zeroize::{Zeroize, Zeroizing};

use crate::appearance::Theme;
use crate::clipboard;
use crate::convert;
use crate::dataframe;
use crate::decode;
use crate::format::{self, FormatKind};
use crate::toolbar_visibility;
use mac_ui::keys::Key;

pub const MAX_VISIBLE: usize = 8;
pub const CHIP_PITCH: f64 = 34.0;
pub const CHIP_PILL_H: f64 = 28.0;

pub const CHIP_GAP: f64 = 6.0;
/// History chevrons: square hit areas at the ends of the history capsule, as tall as a chip.
pub const NAV_BUTTON: f64 = 28.0;
/// The "3 / 20" position between the chevrons. Fixed and wide enough for the longest label
/// (history holds 20), so the capsule never changes width while stepping.
pub const NAV_COUNT_W: f64 = 44.0;
/// The whole history capsule: chevron, position, chevron, with no gaps.
pub const NAV_SPAN: f64 = NAV_BUTTON + NAV_COUNT_W + NAV_BUTTON;

/// A history chevron: its SF Symbol, the glyph drawn without SF Symbols, its name (the
/// VoiceOver label and tooltip, never drawn) and which way it steps through history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chevron {
    pub symbol: &'static str,
    pub glyph: &'static str,
    pub name: &'static str,
    /// Steps to an older copy (the position number goes up).
    pub older: bool,
    /// Its arrow key ([`arrow_action`]), drawn (`←`) and spoken (`Left Arrow`).
    pub arrow: &'static str,
    pub spoken_arrow: &'static str,
}

/// `‹` on the left: the previous number, toward 1 (the newest copy).
pub const PREVIOUS_CHEVRON: Chevron = Chevron {
    symbol: "chevron.left",
    glyph: "‹",
    name: "Previous",
    older: false,
    arrow: "←",
    spoken_arrow: "Left Arrow",
};

/// `›` on the right: the next number, toward N (the oldest copy).
pub const NEXT_CHEVRON: Chevron = Chevron {
    symbol: "chevron.right",
    glyph: "›",
    name: "Next",
    older: true,
    arrow: "→",
    spoken_arrow: "Right Arrow",
};

impl Chevron {
    /// The command a click runs.
    pub fn command(&self) -> CommandId {
        if self.older {
            CommandId::HistoryOlder
        } else {
            CommandId::HistoryNewer
        }
    }

    /// Whether there is somewhere to go: `‹` is off at 1, `›` at N.
    pub fn enabled(&self, nav: &HistoryNav) -> bool {
        if self.older {
            nav.can_older
        } else {
            nav.can_newer
        }
    }

    /// The button title: empty when the symbol image shows (image only, so no name is drawn
    /// beside or under it), else just the fallback glyph.
    pub fn title(&self, has_image: bool) -> &'static str {
        if has_image { "" } else { self.glyph }
    }

    /// Tooltip: the name and its keys, `Previous (← or ⌘←)`.
    pub fn tooltip(&self) -> String {
        format!("{} ({} or ⌘{})", self.name, self.arrow, self.arrow)
    }

    /// VoiceOver hint: `Left Arrow, or Command-Left Arrow`.
    pub fn spoken_shortcut(&self) -> String {
        format!("{0}, or Command-{0}", self.spoken_arrow)
    }
}

/// Where the keyboard focus is when an arrow key reaches the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowFocus {
    /// The card itself: the search field while it is empty (also before a search), the empty
    /// find field, a button or the reveal cover.
    Card,
    /// The search or find field with text in it: plain ← → move its caret.
    TextWithContent,
}

/// What an arrow key does on the card ([`arrow_action`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowAction {
    /// Step through history like this chevron. At an end (or without history) nothing moves
    /// and the card beeps.
    History(Chevron),
    /// Move the chip selection ([`step_chip`]).
    Chip { dx: isize, dy: isize },
    /// Leave the key to the text field (its caret moves).
    Text,
}

/// The arrow-key rule:
/// - ⌘← / ⌘→ always step through history (Previous / Next), also from a field with text.
/// - ← / → move the caret in a search or find field that has text. Otherwise they step through
///   history while the history capsule is shown, and move between the chips when it is not.
/// - ↑ / ↓ move between the chips: to the row above or below, or to the previous or next chip
///   when there is no row that way (so every chip stays reachable when ← → step history).
///
/// The read-only text in the well handles its own arrows when it has the focus (a click in it
/// puts a caret or selection there), so it never gets here for plain arrows.
pub fn arrow_action(
    key: Key,
    command: bool,
    focus: ArrowFocus,
    has_history: bool,
) -> Option<ArrowAction> {
    let (chevron, dx) = match key {
        Key::Left => (PREVIOUS_CHEVRON, -1),
        Key::Right => (NEXT_CHEVRON, 1),
        Key::Up => return Some(ArrowAction::Chip { dx: 0, dy: -1 }),
        Key::Down => return Some(ArrowAction::Chip { dx: 0, dy: 1 }),
        _ => return None,
    };
    Some(if command {
        ArrowAction::History(chevron)
    } else if focus == ArrowFocus::TextWithContent {
        ArrowAction::Text
    } else if has_history {
        ArrowAction::History(chevron)
    } else {
        ArrowAction::Chip { dx, dy: 0 }
    })
}

/// Empty space kept on the right of the first chip row so the capsule fits.
pub const NAV_RESERVE: f64 = CHIP_GAP + NAV_SPAN;
/// The table's version capsule (`↶ v2/3 ↷`), the history capsule's size.
pub const VERSION_SPAN: f64 = NAV_SPAN;

/// Room the first chip row keeps on its right for the capsules: the history capsule (`nav`)
/// and, left of it, the version capsule (`versions`).
pub fn trailing_reserve(nav: bool, versions: bool) -> f64 {
    let mut reserve = 0.0;
    if nav {
        reserve += NAV_RESERVE;
    }
    if versions {
        reserve += CHIP_GAP + VERSION_SPAN;
    }
    reserve
}

/// Left edge of the version capsule in a chip row ending at `right`: just left of the history
/// capsule when there is one, else at the right end.
pub fn version_capsule_x(right: f64, nav: bool) -> f64 {
    let nav_room = if nav { NAV_SPAN + CHIP_GAP } else { 0.0 };
    right - nav_room - VERSION_SPAN
}
const EXCERPT_LINES: usize = 6;
const EXCERPT_LINE_CHARS: usize = 48;
/// Formatted code stays whole so the card can color real tokens. Past this, the tail is cut.
const CODE_CAP: usize = 80_000;
/// Lines (table rows) a card shows before "Show all". Enough to read the data, quick to lay out.
pub const PREVIEW_ROWS: usize = 200;
/// Characters a card shows before "Show all", for text with very long lines.
pub const PREVIEW_CHARS: usize = 20_000;
/// Second line of an empty card: a file or text can be dropped on it instead.
const DROP_HINT: &str = "Drop a text file or image here to open it";

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
    /// next (once; [`crate::paste_access::ASK_NOTE`]).
    PasteAsk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hist {
    pub index: usize,
    pub title: String,
    pub mark: String,
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
    /// `<` older and `>` newer, when an earlier copy exists. Absent hides both.
    pub history_nav: Option<HistoryNav>,
    pub theme: Theme,
    /// Settings with a check mark in the `⋯` menu.
    pub settings: crate::settings::Settings,
    /// Which result the card is showing. Original is the clipboard itself.
    pub view: CardView,
    /// Info, a smaller JPEG, recognized text, and QR payloads for an image.
    pub image_scan: Option<ImageScan>,
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
}

/// The versions of a picture for the card. When a step's version is shown, [`LaunchData`]'s
/// `image`, `picture` and `image_scan` are that version's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageShown {
    /// The entry: the original picture's allocation, so every version is the same item.
    pub entry: u64,
    /// "Original", then each step's label.
    pub labels: Vec<String>,
    /// The version shown: 0 is the original.
    pub version: usize,
    /// A step or another version is being worked out.
    pub working: bool,
    /// Why the last step did nothing or failed, for the meta line.
    pub note: Option<String>,
}

impl ImageShown {
    pub fn can_undo(&self) -> bool {
        self.version > 0
    }

    pub fn can_redo(&self) -> bool {
        self.version + 1 < self.labels.len()
    }
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
    Dataframe,
    Schema,
    Sample,
    Info,
    Ocr,
    Qr,
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

impl HistoryNav {
    /// Position between the chevrons: `1 / 3`.
    pub fn label(&self) -> String {
        format!("{} / {}", self.position, self.total)
    }

    /// What VoiceOver reads for the position: `Item 1 of 3`.
    pub fn spoken(&self) -> String {
        format!("Item {} of {}", self.position, self.total)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandId {
    Original,
    Visit,
    Format,
    Convert,
    Decode,
    Dataframe,
    Schema,
    Sample,
    Info,
    Ocr,
    Qr,
    Copy,
    Save,
    History(usize),
    /// Step to the previous copy. The history order stays put.
    HistoryOlder,
    /// Step back toward the newest copy.
    HistoryNewer,
    /// Empty the OS pasteboard. Copies still held in memory stay until [`Clear`](CommandId::Clear).
    ClearClipboard,
    ClearHistory,
    /// Wipe clipboard copies held in memory, then empty the pasteboard.
    Clear,
    /// Open a file on the card.
    ChooseFile,
    /// Leave the chosen file and show the clipboard again.
    UseClipboard,
    /// Render the whole text on the card instead of its preview.
    ShowAll,
    Appearance(Theme),
    /// Turn the minute timer on copycraft's sensitive copies on or off.
    ClearSensitive,
    /// Forget history this many minutes after the last copy; 0: no time limit.
    KeepHistory(u32),
    /// A step on the table: a new version after the one shown ([`crate::table`]).
    TableStep(crate::table::TableOp),
    /// Show the table version before the one shown.
    TableUndo,
    /// Show the table version after the one shown.
    TableRedo,
    /// Show table version `n` (0 is the original), from the version capsule's menu.
    TableVersion(usize),
    /// The "Table ▾" chip: the card pops a menu of table steps ([`table_menu`]).
    TableMenu,
    /// The "Image ▾" chip: the card pops a menu of picture steps ([`image_menu_items`]).
    ImageMenu,
    /// A step on the picture: a new version after the one shown ([`crate::image_edit`]).
    /// Undo, redo and the version capsule use the table's commands.
    ImageStep(crate::image_edit::ImageOp),
    /// Resize › Custom…: asks for a width or a height.
    ImageResizeCustom,
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
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    pub title: String,
    pub detail: String,
    keywords: String,
}

impl Command {
    pub fn wipe(&mut self) {
        self.title.zeroize();
        self.detail.zeroize();
        self.keywords.zeroize();
    }

    fn matches(&self, needle: &str) -> bool {
        self.title.to_lowercase().contains(needle)
            || self.detail.to_lowercase().contains(needle)
            || self.keywords.to_lowercase().contains(needle)
    }
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
        for item in &mut self.history {
            item.title.zeroize();
            item.mark.zeroize();
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

/// History row. The type mark and size stand in for the copied text.
pub fn history_label(mark: &str, byte_len: usize) -> String {
    format!("{mark}  {}", format_bytes(byte_len))
}

/// Whether a revealed card stays revealed when it is stored again: while it shows the same
/// history entry (`shown_key` and `next_key` the same [`content_key`]), in all its views, chips,
/// table versions and steps, and when a rebuilt card or its sensitivity labels arrive. Views of
/// one entry can show different labels (a conversion, a version's CSV); a step only removes
/// data, so none of that masks it again. Another entry does; focus loss, Wipe, lock and
/// retention mask it on their own.
pub fn stays_revealed(revealed: bool, shown_key: u64, next_key: u64) -> bool {
    revealed && shown_key == next_key
}

/// The well has copied content, so it stays blurred until it is clicked.
/// Image info is dimensions and a data URL, so the Info chip stays sharp.
pub fn masks_content(card: &WorkCard, view: CardView) -> bool {
    if view == CardView::Info {
        return false;
    }
    card.shows_image || card.link_page.is_some() || !card.excerpt.is_empty()
}

/// How a masked well hides the copy. A blur that cannot be installed covers
/// the well and drops the excerpt. The transparent click target is not a mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WellMask {
    pub blur: bool,
    pub shade: bool,
    pub show_body: bool,
}

pub fn well_mask(masked: bool, blur_ready: bool) -> WellMask {
    if !masked {
        WellMask {
            blur: false,
            shade: false,
            show_body: true,
        }
    } else if blur_ready {
        WellMask {
            blur: true,
            shade: false,
            show_body: true,
        }
    } else {
        WellMask {
            blur: false,
            shade: true,
            show_body: false,
        }
    }
}

/// Identity of the copied item. Chips on that item share one reveal.
pub fn content_key(data: &LaunchData) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.subject_kind.hash(&mut hasher);
    data.subject_text.hash(&mut hasher);
    data.source_name.hash(&mut hasher);
    if let Some(edit) = &data.image_edit {
        // Each version of a picture is the same entry, which stays revealed.
        edit.entry.hash(&mut hasher);
    } else {
        if let Some(image) = &data.image {
            image.format.hash(&mut hasher);
            image.width.hash(&mut hasher);
            image.height.hash(&mut hasher);
            image.byte_len.hash(&mut hasher);
        }
        // Two dropped pictures alike in name, format and size are still different items.
        if let Some(picture) = &data.picture {
            picture.allocation_id().hash(&mut hasher);
        }
    }
    // Not the table's version, view, reading or labels: those show the same entry, which stays
    // revealed ([`stays_revealed`]).
    hasher.finish()
}

/// The start of the clipboard text, kept on its own lines.
pub fn payload_excerpt(text: &str) -> String {
    let mut out = Vec::new();
    let mut extra = false;
    for (index, line) in text.lines().enumerate() {
        if index >= EXCERPT_LINES {
            extra = true;
            break;
        }
        out.push(clip_chars(line, EXCERPT_LINE_CHARS));
    }
    if extra
        && let Some(last) = out.last_mut()
        && !last.ends_with('…')
    {
        last.push('…');
    }
    out.join("\n")
}

pub fn work_card(data: &LaunchData) -> WorkCard {
    if let Some(note) = &data.source_note {
        return WorkCard {
            title: data
                .source_name
                .clone()
                .unwrap_or_else(|| "File".to_string()),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: note.clone(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        };
    }
    let mut card = compose_card(data);
    if let Some(name) = &data.source_name {
        card.meta = if card.meta.is_empty() {
            name.clone()
        } else {
            format!("{name}  ·  {}", card.meta)
        };
    }
    card
}

fn compose_card(data: &LaunchData) -> WorkCard {
    match data.subject_kind {
        SubjectKind::Image => image_card(data),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            if let Some(card) = youtube_card(text) {
                return card;
            }
            if let Some(card) = page_card(text) {
                return card;
            }
            let (excerpt, preview_note) = excerpt_for(text, data.full);
            let mut card = WorkCard {
                title: text_title(text),
                meta: text_meta(text),
                excerpt,
                placeholder: String::new(),
                shows_image: false,
                highlight: None,
                selectable: false,
                link_page: None,
                preview_note,
                error_line: None,
            };
            let view = presented_view(text, data.view);
            apply_text_view(&mut card, text, view, data.full, data.table.as_ref());
            // JSON with a mistake stays as copied; the meta line says where, the well marks it.
            if view == CardView::Format
                && let Some(problem) = crate::validate::broken_json(text)
            {
                add_meta_note(&mut card, &problem.note());
                card.error_line = Some(problem.line);
            }
            card
        }
        SubjectKind::Empty => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: format!("Nothing copied\n{DROP_HINT}"),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Hidden => WorkCard {
            title: crate::clipboard::HIDDEN_CONTENT.to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: "The app that copied this marked it private.\nCopycraft does not show it or keep it in history.".to_string(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Denied => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: format!(
                "{}\nPaste from Other Apps is set to Deny for Copycraft.\n{DROP_HINT}",
                crate::paste_access::DENIED_NOTE
            ),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Pending | SubjectKind::PasteAsk => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: if data.subject_kind == SubjectKind::PasteAsk {
                crate::paste_access::ASK_NOTE.to_string()
            } else {
                format!("{}\n{DROP_HINT}", crate::paste_access::PENDING_NOTE)
            },
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::NoText => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: format!("No text on the clipboard\n{DROP_HINT}"),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
    }
}

fn image_card(data: &LaunchData) -> WorkCard {
    let mut facts = data.image.as_ref().map(image_meta).unwrap_or_default();
    if let Some(edit) = &data.image_edit {
        let mut parts: Vec<&str> = Vec::new();
        if !facts.is_empty() {
            parts.push(&facts);
        }
        if edit.version > 0
            && let Some(label) = edit.labels.get(edit.version)
        {
            parts.push(label);
        }
        if edit.working {
            parts.push("Working…");
        } else if let Some(note) = &edit.note {
            parts.push(note);
        }
        facts = parts.join("  ·  ");
    }
    let text = match data.view {
        CardView::Info => Some(image_info_text(data)),
        CardView::Ocr | CardView::Qr => image_view_text(data.image_scan.as_ref(), data.view),
        _ => None,
    };
    if let Some(text) = text {
        let meta = if matches!(data.view, CardView::Ocr | CardView::Qr) {
            text_meta(&text)
        } else {
            facts
        };
        return WorkCard {
            title: match data.view {
                CardView::Ocr => "Text".to_string(),
                CardView::Qr => "QR".to_string(),
                _ => "Image info".to_string(),
            },
            meta,
            excerpt: shown_body(&text),
            placeholder: String::new(),
            shows_image: false,
            highlight: None,
            selectable: true,
            link_page: None,
            preview_note: None,
            error_line: None,
        };
    }
    WorkCard {
        title: "Image".to_string(),
        meta: facts,
        excerpt: String::new(),
        placeholder: String::new(),
        shows_image: true,
        highlight: None,
        selectable: false,
        link_page: None,
        preview_note: None,
        error_line: None,
    }
}

/// Text a non-picture image view copies or saves.
/// OCR and QR use that body.
pub fn image_view_text(scan: Option<&ImageScan>, view: CardView) -> Option<String> {
    let scan = scan?;
    match view {
        CardView::Info => Some(scan.info.clone()),
        CardView::Ocr => scan.ocr.clone(),
        CardView::Qr => scan.qr.clone(),
        _ => None,
    }
}

/// `full` renders the whole text; otherwise the card shows a preview (see [`excerpt_for`]).
fn apply_text_view(
    card: &mut WorkCard,
    source: &str,
    view: CardView,
    full: bool,
    table: Option<&TableShown>,
) {
    if view == CardView::Original {
        show_copied_table(card, source, full);
        return;
    }
    if view == CardView::Dataframe {
        match table {
            Some(table)
                if table.version > 0 || table.options != dataframe::ReadOptions::default() =>
            {
                show_table_version(card, table, full)
            }
            _ => show_dataframe(card, source, full, table.and_then(|table| table.overview)),
        }
        if let Some(error) = table.and_then(|table| table.error.as_deref()) {
            add_meta_note(card, error);
        }
        return;
    }
    let Some(body) = transformed_text(source, view) else {
        return;
    };
    let kind = format::detect(source);
    card.title = if view == CardView::Format && matches!(kind, FormatKind::Json | FormatKind::Xml) {
        text_title(source)
    } else if let Some(flagged) = (view == CardView::Format)
        .then(|| crate::validate::code_title(source))
        .flatten()
    {
        // Formatting still runs; the title says the brackets do not balance.
        flagged
    } else if view == CardView::Format && body == source {
        kind.source_heading().to_string()
    } else {
        text_view_title(source, view, &body)
    };
    card.meta = text_meta(&body);
    if matches!(view, CardView::Schema | CardView::Sample) {
        set_excerpt(card, &body, full);
        card.highlight = Some(if view == CardView::Sample || kind == FormatKind::Xml {
            FormatKind::Xml
        } else {
            FormatKind::Json
        });
        card.selectable = true;
    } else if view == CardView::Format && opens_formatted(source) {
        set_excerpt(card, &body, full);
        card.highlight = Some(kind);
    } else {
        set_excerpt(card, &body, full);
    }
}

/// A copied CSV or TSV opens aligned and colored as that table.
/// The polars grid stays on the Dataframe button. Copy and save stay on the source.
/// A preview aligns only the header and the first [`PREVIEW_ROWS`] rows.
fn show_copied_table(card: &mut WorkCard, source: &str, full: bool) {
    let kind = format::detect(source);
    if !matches!(kind, FormatKind::Csv | FormatKind::Tsv) {
        return;
    }
    let cut = (!full)
        .then(|| preview_cut(source, PREVIEW_ROWS + 1))
        .flatten();
    let shown = cut.map_or(source, |end| &source[..end]);
    let body = align_table(shown, kind).unwrap_or_else(|| shown.to_string());
    card.highlight = Some(kind);
    card.selectable = true;
    let start = dataframe::table_start(source);
    if cut.is_some() {
        // The header and the lines above it are not rows.
        let above = 1 + start.map_or(0, |start| start.header_line);
        let rows = source.lines().count().saturating_sub(above);
        let shown_rows = shown.lines().count().saturating_sub(above);
        card.excerpt = body;
        card.preview_note = Some(showing_note(shown_rows, rows, "rows"));
        card.meta = text_meta(source);
    } else {
        set_excerpt(card, &body, full);
        card.meta = text_meta_from(&body, source);
    }
    add_table_note(card, start);
}

/// "40 rows × 20 columns" for the column overview, which shows no rows.
fn overview_note(preview: &dataframe::DataframePreview) -> String {
    format!("{} rows × {} columns", preview.rows, preview.columns)
}

/// "Header on line N" in the meta line, after the size, when lines above a table's header
/// were skipped ([`dataframe::TableStart::note`]).
fn add_table_note(card: &mut WorkCard, start: Option<dataframe::TableStart>) {
    if let Some(note) = start.and_then(|start| start.note()) {
        add_meta_note(card, &note);
    }
}

/// `note` in the meta line after the size, before the sensitivity labels.
fn add_meta_note(card: &mut WorkCard, note: &str) {
    add_note(&mut card.meta, note);
}

/// `note` right after the size in `meta`, before the labels and earlier notes.
fn add_note(meta: &mut String, note: &str) {
    *meta = match meta.find(META_SEPARATOR) {
        Some(at) => format!("{}{META_SEPARATOR}{note}{}", &meta[..at], &meta[at..]),
        None => format!("{meta}{META_SEPARATOR}{note}"),
    };
}

const META_SEPARATOR: &str = "  ·  ";

/// The table as a polars grid: whole with `full`, else its first [`PREVIEW_ROWS`] rows with a
/// note when the table has more. The meta measures the copied table when rows are left out,
/// and says where the header is and which date order was read when either is a guess.
/// The grid no longer has the table's delimiters, so a Salaris column would lose its financial
/// mark: the meta classifies the copied table.
fn show_dataframe(card: &mut WorkCard, source: &str, full: bool, overview: Option<bool>) {
    let max_rows = if full { usize::MAX } else { PREVIEW_ROWS };
    let Some(preview) = dataframe::try_format_preview(source, max_rows, overview) else {
        return;
    };
    card.title = "Dataframe".to_string();
    card.highlight = Some(FormatKind::Dataframe);
    card.selectable = true;
    if let Some(overview) = preview.overview.as_ref() {
        card.meta = text_meta(source);
        card.excerpt = overview.clone();
        card.preview_note = None;
        add_meta_note(card, &overview_note(&preview));
    } else if preview.rows > preview.shown_rows {
        card.meta = text_meta(source);
        card.preview_note = Some(showing_note(preview.shown_rows, preview.rows, "rows"));
        card.excerpt = preview.grid;
    } else {
        card.meta = text_meta_from(&preview.grid, source);
        card.excerpt = if full {
            preview.grid
        } else {
            shown_body(&preview.grid)
        };
        card.preview_note = None;
    }
    add_table_note(card, dataframe::table_start(source.trim()));
    if let Some(note) = dataframe::dates_note(&preview.dates) {
        add_meta_note(card, &note);
    }
}

/// The step of the version shown ("Identifier removed") in the meta line, after the size;
/// nothing for the original.
fn add_version_note(card: &mut WorkCard, table: &TableShown) {
    if table.version > 0
        && let Some(label) = table.labels.get(table.version)
    {
        add_meta_note(card, label);
    }
}

/// A table version after one or more steps, from its frame. Size, lines and sensitivity
/// labels are the version's own (as CSV): a step can drop or keep a sensitive column. Which
/// version it is shows in the version capsule ([`VersionBar`]), its step in the meta line.
fn show_table_version(card: &mut WorkCard, table: &TableShown, full: bool) {
    card.title = "Dataframe".to_string();
    card.highlight = Some(FormatKind::Dataframe);
    card.preview_note = None;
    let Some(frame) = &table.frame else {
        card.excerpt.clear();
        card.placeholder = "Working on the table…".to_string();
        card.selectable = false;
        card.meta.clear();
        return;
    };
    let max_rows = if full { usize::MAX } else { PREVIEW_ROWS };
    let Some(preview) = dataframe::frame_preview(frame, max_rows, table.overview) else {
        card.excerpt.clear();
        card.placeholder = "The table is empty".to_string();
        card.selectable = false;
        card.meta.clear();
        return;
    };
    let csv = Zeroizing::new(dataframe::frame_csv(frame).unwrap_or_default());
    card.selectable = true;
    card.meta = text_meta_from(&csv, &csv);
    if let Some(overview) = preview.overview.as_ref() {
        card.excerpt = overview.clone();
        add_meta_note(card, &overview_note(&preview));
    } else if preview.rows > preview.shown_rows {
        card.preview_note = Some(showing_note(preview.shown_rows, preview.rows, "rows"));
        card.excerpt = preview.grid;
    } else if full {
        card.excerpt = preview.grid;
    } else {
        card.excerpt = shown_body(&preview.grid);
    }
    for note in table
        .notes
        .iter()
        .flat_map(dataframe::ReadNotes::meta_notes)
    {
        add_meta_note(card, &note);
    }
    // Last, so it reads first after the size.
    add_version_note(card, table);
}

/// Put `body` in the well: whole with `full`, else its preview and the note.
fn set_excerpt(card: &mut WorkCard, body: &str, full: bool) {
    let (excerpt, note) = excerpt_for(body, full);
    card.excerpt = excerpt;
    card.preview_note = note;
}

/// What the well shows of `body`: all of it with `full`; else at most [`PREVIEW_ROWS`] lines
/// and [`PREVIEW_CHARS`] characters, with a note saying how much is left out. Copy, Save and
/// Format always use the whole text, never this excerpt.
pub fn excerpt_for(body: &str, full: bool) -> (String, Option<String>) {
    if full {
        return (body.to_string(), None);
    }
    let Some(end) = preview_cut(body, PREVIEW_ROWS) else {
        return (shown_body(body), None);
    };
    let shown = &body[..end];
    let lines = body.lines().count();
    let shown_lines = shown.lines().count();
    let note = if shown_lines < lines && shown.chars().count() < PREVIEW_CHARS {
        showing_note(shown_lines, lines, "lines")
    } else {
        showing_note(shown.chars().count(), body.chars().count(), "characters")
    };
    (format!("{shown}\n…"), Some(note))
}

/// Byte length of the first `lines` lines of `text`, at most [`PREVIEW_CHARS`] characters.
/// `None` when all of `text` fits.
pub fn preview_cut(text: &str, lines: usize) -> Option<usize> {
    let by_lines = text
        .match_indices('\n')
        .nth(lines.saturating_sub(1))
        .map(|(index, _)| index)
        .filter(|&index| index + 1 < text.len() && !text[index + 1..].trim().is_empty());
    let by_chars = text
        .char_indices()
        .nth(PREVIEW_CHARS)
        .map(|(index, _)| index);
    match (by_lines, by_chars) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// "Showing 200 of 23,220 rows".
pub fn showing_note(shown: usize, total: usize, unit: &str) -> String {
    format!(
        "Showing {} of {} {unit}",
        group_thousands(shown),
        group_thousands(total)
    )
}

pub(crate) fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Column gaps are display-only. A tab is drawn as a middle dot so the columns
/// land on character cells; the copied bytes keep their real delimiter.
const TSV_MARK: char = '\u{00b7}';

/// Lines above the header (see [`dataframe::table_start`]) stay as they are, above the table.
fn align_table(source: &str, kind: FormatKind) -> Option<String> {
    let (above, table) = match dataframe::table_start(source) {
        Some(start) if start.skipped > 0 => source.split_at(start.offset),
        _ => ("", source),
    };
    let aligned = align_rows(table, kind)?;
    Some(format!("{above}{aligned}"))
}

fn align_rows(source: &str, kind: FormatKind) -> Option<String> {
    let parsed = source_separator(source, kind)?;
    let shown = if kind == FormatKind::Tsv {
        TSV_MARK
    } else {
        parsed
    };
    let mut rows = Vec::new();
    for line in source.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            rows.push(None);
        } else {
            rows.push(Some(split_cells(line, parsed)));
        }
    }
    let mut widths = Vec::new();
    for row in &rows {
        let Some(cells) = row else {
            continue;
        };
        for (i, cell) in cells.iter().enumerate() {
            if widths.len() == i {
                widths.push(0);
            }
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|row| match row {
            Some(cells) => render_row(cells, &widths, shown),
            None => String::new(),
        })
        .collect();
    Some(lines.join("\n"))
}

fn source_separator(source: &str, kind: FormatKind) -> Option<char> {
    match kind {
        FormatKind::Tsv => Some('\t'),
        FormatKind::Csv => csv_separator(source),
        _ => None,
    }
}

fn csv_separator(source: &str) -> Option<char> {
    if let Some(start) = dataframe::table_start(source) {
        return Some(start.separator as char);
    }
    let header = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let semis = header.matches(';').count();
    let commas = header.matches(',').count();
    if semis >= 1 && semis >= commas {
        Some(';')
    } else if commas >= 1 {
        Some(',')
    } else {
        None
    }
}

fn split_cells(line: &str, sep: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek().is_some_and(|(_, next)| *next == '"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if ch == sep && !in_quotes {
            cells.push(line[start..idx].trim().to_string());
            start = idx + ch.len_utf8();
        }
    }
    cells.push(line[start..].trim().to_string());
    cells
}

fn render_row(cells: &[String], widths: &[usize], sep: char) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        out.push_str(cell);
        if i + 1 == cells.len() {
            break;
        }
        let width = widths.get(i).copied().unwrap_or(0);
        let pad = width.saturating_sub(cell.chars().count());
        for _ in 0..pad {
            out.push(' ');
        }
        out.push(' ');
        out.push(sep);
        out.push(' ');
    }
    out
}

/// JSON, YAML, XML, Markdown, and code open already formatted, so Original is not a separate card state.
pub fn presented_view(source: &str, view: CardView) -> CardView {
    if view == CardView::Original && opens_formatted(source) {
        CardView::Format
    } else {
        view
    }
}

fn opens_formatted(source: &str) -> bool {
    matches!(
        format::detect(source),
        FormatKind::Json
            | FormatKind::Yaml
            | FormatKind::Xml
            | FormatKind::Html
            | FormatKind::Markdown
            | FormatKind::Rust
            | FormatKind::Java
            | FormatKind::Python
    )
}

fn image_info_text(data: &LaunchData) -> String {
    let mut parts = Vec::new();
    if let Some(info) = data.image_scan.as_ref().map(|scan| scan.info.clone()) {
        parts.push(info);
    } else if let Some(facts) = &data.image {
        parts.push(format!(
            "{}\nSize: {}×{}\n{}",
            facts.format,
            facts.width,
            facts.height,
            format_bytes(facts.byte_len)
        ));
    } else {
        parts.push("Image".to_string());
    }
    if let Some(url) = data
        .image_scan
        .as_ref()
        .and_then(|scan| scan.data_url.clone())
    {
        parts.push(url);
    }
    parts.join("\n\n")
}

fn shown_body(body: &str) -> String {
    let mut chars = body.chars();
    let shown: String = chars.by_ref().take(CODE_CAP).collect();
    if chars.next().is_some() {
        format!("{shown}\n…")
    } else {
        shown
    }
}

fn text_title(text: &str) -> String {
    crate::validate::title(text)
        .or_else(|| crate::validate::code_title(text))
        .unwrap_or_else(|| format::detect(text).source_heading().to_string())
}

fn text_view_title(source: &str, view: CardView, body: &str) -> String {
    match view {
        CardView::Format => format::detect(source).preview_heading().to_string(),
        CardView::Dataframe => "Dataframe".to_string(),
        CardView::Schema => if format::detect(source) == FormatKind::Xml {
            "XSD schema"
        } else {
            "Avro schema"
        }
        .to_string(),
        CardView::Sample => "Sample XML".to_string(),
        _ => format::detect(body).source_heading().to_string(),
    }
}

/// Full text for the current card view. Original returns the source.
pub fn transformed_text(source: &str, view: CardView) -> Option<String> {
    match view {
        CardView::Original => Some(source.to_string()),
        CardView::Format => Some(clipboard::formatted(source)),
        CardView::Convert => convert::try_convert(source).map(|body| clipboard::formatted(&body)),
        CardView::Decode => decode::try_decode(source).map(|body| clipboard::formatted(&body)),
        CardView::Dataframe => dataframe::try_format(source),
        CardView::Schema => {
            if format::detect(source) == FormatKind::Xml {
                crate::xsd_schema::try_schema(source)
            } else {
                crate::avro_schema::try_schema(source)
            }
        }
        CardView::Sample => crate::xsd_schema::try_sample(source),
        CardView::Info | CardView::Ocr | CardView::Qr => None,
    }
}

/// File name and extension [`text_save_file`] gives `view`, when they follow from `source`
/// alone (Original, Format, Dataframe). The save panel can then open before the bytes are
/// built, and the conversion runs after a path is chosen. `None` for views whose name depends
/// on the converted text.
pub fn deferred_save_name(source: &str, view: CardView) -> Option<(String, &'static str)> {
    let kind = match view {
        CardView::Original | CardView::Format => format::detect(source),
        CardView::Dataframe => FormatKind::Dataframe,
        _ => return None,
    };
    Some((kind.suggested_filename(), kind.suggested_extension()))
}

pub fn text_save_file(source: &str, view: CardView) -> Option<SaveFile> {
    if view == CardView::Dataframe {
        // The table as the view reads it (dates typed), as CSV; the save panel can pick Parquet.
        let bytes = dataframe::TableFile::Csv.bytes(&dataframe::parse_table(source)?)?;
        return Some(SaveFile {
            filename: FormatKind::Dataframe.suggested_filename(),
            extension: FormatKind::Dataframe.suggested_extension(),
            bytes,
        });
    }
    if view == CardView::Schema {
        let body = transformed_text(source, view)?;
        let (filename, extension) = if format::detect(source) == FormatKind::Xml {
            ("clipboard.xsd", "xsd")
        } else {
            ("clipboard.avsc", "avsc")
        };
        return Some(SaveFile {
            filename: filename.to_string(),
            extension,
            bytes: body.into_bytes(),
        });
    }
    let body = transformed_text(source, view)?;
    let kind = match view {
        CardView::Format => format::detect(source),
        _ => format::detect(&body),
    };
    Some(SaveFile {
        filename: kind.suggested_filename(),
        extension: kind.suggested_extension(),
        bytes: body.into_bytes(),
    })
}

/// A page link: the URL as plain text under the kind, "Link". Nothing is fetched for it.
fn page_card(text: &str) -> Option<WorkCard> {
    let page = crate::page_preview::page_url(text)?;
    Some(WorkCard {
        title: "Link".to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        // The URL can be selected and copied.
        selectable: true,
        link_page: Some(page.to_string()),
        preview_note: None,
        error_line: None,
    })
}

fn youtube_card(text: &str) -> Option<WorkCard> {
    crate::youtube::video_id(text)?;
    Some(WorkCard {
        title: "YouTube".to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        selectable: true,
        link_page: Some(text.trim().to_string()),
        preview_note: None,
        error_line: None,
    })
}

/// Actions for the thing on the clipboard. Housekeeping stays in [`overflow`].
pub fn chips(data: &LaunchData) -> Vec<Command> {
    match data.subject_kind {
        SubjectKind::Image => image_chips(data.image_scan.as_ref(), data.image_edit.is_some()),
        SubjectKind::Text => {
            let mut chips = copied_text_chips(data.subject_text.as_deref().unwrap_or(""));
            // The date question, in the Dataframe view while dates fit both orders.
            if data.view == CardView::Dataframe
                && let Some(question) = data
                    .table
                    .as_ref()
                    .and_then(|table| table.notes)
                    .and_then(|notes| date_order_command(&notes))
            {
                chips.push(question);
            }
            chips
        }
        SubjectKind::Empty
        | SubjectKind::NoText
        | SubjectKind::Hidden
        | SubjectKind::Denied
        | SubjectKind::Pending
        | SubjectKind::PasteAsk => Vec::new(),
    }
}

/// The chips of a copied text, which follow from the text alone. Working them out tries the
/// formatter, the converters and the table parser, and every card update (each step through
/// history, twice through [`search_pool`]) asks again, so they are remembered for the last
/// copies, whatever their length. Only the text's hash is kept with them; the chips are fixed
/// labels. [`forget_chips`] drops them (Wipe).
static TEXT_CHIPS: crate::memo::Memo<Vec<Command>> =
    crate::memo::Memo::with_min_len(crate::clipboard::MAX_HISTORY + 4, 0);

fn copied_text_chips(text: &str) -> Vec<Command> {
    TEXT_CHIPS.get_or_compute(text, |text| {
        if crate::youtube::video_id(text).is_some() || crate::page_preview::page_url(text).is_some()
        {
            link_chips(text)
        } else {
            text_chips(text)
        }
    })
}

/// Work out (and remember) the chips of a copied text ahead of the card, off the main thread.
pub fn warm_chips(text: &str) {
    copied_text_chips(text);
}

/// Drop the remembered chips (Wipe).
pub fn forget_chips() {
    TEXT_CHIPS.clear();
}

/// Chips, earlier copies, and appearance. Quit stays out.
pub fn search_pool(data: &LaunchData) -> Vec<Command> {
    let mut commands = chips(data);
    let offers_table = commands.iter().any(|c| c.id == CommandId::TableMenu);
    if offers_table || data.table.is_some() {
        commands.extend(table_menu(data.table.as_ref()));
        commands.extend(data.table.as_ref().map(table_undo_redo).unwrap_or_default());
    }
    if let Some(edit) = &data.image_edit {
        commands.extend(
            image_menu_items(edit)
                .into_iter()
                .map(|(command, _)| command),
        );
    }
    for item in &data.history {
        commands.push(command(
            CommandId::History(item.index),
            &item.title,
            &item.mark,
            "history",
        ));
    }
    for theme in [Theme::System, Theme::Light, Theme::Dark] {
        let name = theme_name(theme);
        let detail = if data.theme == theme {
            "Appearance · current"
        } else {
            "Appearance"
        };
        let keywords = match theme {
            Theme::System => "appearance theme system",
            Theme::Light => "appearance theme light",
            Theme::Dark => "appearance theme dark",
        };
        commands.push(command(
            CommandId::Appearance(theme),
            name,
            detail,
            keywords,
        ));
    }
    commands
}

/// Byte range of 1-based line `line` in `text` (without its line end), for the well's mark.
/// `None` past the last line.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn line_range(text: &str, line: usize) -> Option<std::ops::Range<usize>> {
    let mut start = 0;
    for (index, part) in text.split('\n').enumerate() {
        if index + 1 == line {
            return Some(start..start + part.trim_end_matches('\r').len());
        }
        start += part.len() + 1;
    }
    None
}

/// Title of the chip that shows a YAML entry as JSON.
pub const TO_JSON_TITLE: &str = "To JSON";

/// Title of the chip that opens the table's menu.
pub fn table_menu_title() -> &'static str {
    crate::locale::t("table_menu")
}
/// English title kept for tests that pin the default language.
#[cfg_attr(not(test), allow(dead_code))]
pub const TABLE_MENU_TITLE: &str = "Table ▾";

/// The steps a table can take, for the "Table ▾" menu and the search.
pub fn table_steps() -> Vec<Command> {
    crate::table::TableOp::ONE_CLICK
        .iter()
        .map(table_step_command)
        .collect()
}

fn table_step_command(op: &crate::table::TableOp) -> Command {
    command(
        CommandId::TableStep(op.clone()),
        &op.title(),
        op.group().unwrap_or("Table"),
        op.keywords(),
    )
}

/// The submenu a table menu item sits in ([`crate::table::TableOp::group`]).
pub fn menu_group(id: &CommandId) -> Option<&'static str> {
    match id {
        CommandId::TableStep(op) => op.group(),
        CommandId::ImageStep(op) => op.group(),
        CommandId::ImageResizeCustom => Some(crate::image_edit::RESIZE_GROUP),
        _ => None,
    }
}

/// The date question: when a date column fits both orders, a command to read it the other
/// way ("Dates are mm/dd/yyyy").
fn date_order_command(notes: &dataframe::ReadNotes) -> Option<Command> {
    if !notes.ambiguous_dates {
        return None;
    }
    let (title, month_first) = if notes.month_first {
        ("Dates are dd/mm/yyyy", false)
    } else {
        ("Dates are mm/dd/yyyy", true)
    };
    Some(command(
        CommandId::TableDateOrder(month_first),
        title,
        "Dates fit both orders",
        "dates order day month us european ambiguous",
    ))
}

/// The "Image ▾" chip.
pub fn image_menu_title() -> &'static str {
    crate::locale::t("image_menu")
}
/// Resize › Custom….
pub const CUSTOM_RESIZE_TITLE: &str = "Custom…";

/// The "Image ▾" menu: Resize (a submenu), the other steps, then undo and redo when there is
/// a version to go to. No check marks.
pub fn image_menu_items(edit: &ImageShown) -> Vec<(Command, bool)> {
    use crate::image_edit::ImageOp;
    let step = |op: &ImageOp| {
        let detail = op.group().unwrap_or("Image");
        command(
            CommandId::ImageStep(*op),
            &op.title(),
            detail,
            image_keywords(op),
        )
    };
    let (resizes, others): (Vec<ImageOp>, Vec<ImageOp>) =
        ImageOp::MENU.iter().partition(|op| op.group().is_some());
    let mut commands: Vec<Command> = resizes.iter().map(step).collect();
    commands.push(command(
        CommandId::ImageResizeCustom,
        CUSTOM_RESIZE_TITLE,
        crate::image_edit::RESIZE_GROUP,
        "resize custom width height size image",
    ));
    commands.extend(others.iter().map(step));
    if edit.can_undo() {
        commands.push(command(
            CommandId::TableUndo,
            "Undo image step",
            &edit.labels[edit.version],
            "undo image version back",
        ));
    }
    if edit.can_redo() {
        commands.push(command(
            CommandId::TableRedo,
            "Redo image step",
            &edit.labels[edit.version + 1],
            "redo image version forward",
        ));
    }
    commands
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

fn image_keywords(op: &crate::image_edit::ImageOp) -> &'static str {
    use crate::image_edit::ImageOp;
    match op {
        ImageOp::Resize(_) => "resize smaller scale size image",
        ImageOp::RotateLeft | ImageOp::RotateRight => "rotate turn image",
        ImageOp::FlipHorizontal | ImageOp::FlipVertical => "flip mirror image",
        ImageOp::RemoveMetadata => "remove metadata exif gps strip image",
        ImageOp::Grayscale => "grayscale black white gray image",
    }
}

/// Undo and redo of a table step, for the search when there is a version to go to (⌘Z, ⇧⌘Z
/// and the version capsule have them too; the Table ▾ menu does not).
fn table_undo_redo(table: &TableShown) -> Vec<Command> {
    let mut commands = Vec::new();
    if table.can_undo() {
        commands.push(command(
            CommandId::TableUndo,
            "Undo table step",
            &table.labels[table.version],
            "undo table version back",
        ));
    }
    if table.can_redo() {
        commands.push(command(
            CommandId::TableRedo,
            "Redo table step",
            &table.labels[table.version + 1],
            "redo table version forward",
        ));
    }
    commands
}

/// The "Table ▾" menu with check marks ([`table_menu`]; none is checked).
pub fn table_menu_items(table: Option<&TableShown>) -> Vec<(Command, bool)> {
    table_menu(table)
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

/// The "Table ▾" menu: Remove duplicate rows, Remove empty rows and columns, then, once the
/// table is read, Choose columns… (more than one column), Sort ascending ›, Sort descending ›
/// and Open in window. Undo and redo are ⌘Z and ⇧⌘Z and the version capsule.
pub fn table_menu(table: Option<&TableShown>) -> Vec<Command> {
    let mut commands = table_steps();
    let Some(frame) = table.and_then(|table| table.frame.as_ref()) else {
        return commands;
    };
    if frame.width() > 1 {
        commands.push(command(
            CommandId::TableChooseColumns,
            CHOOSE_COLUMNS_TITLE,
            "Keep some columns, as one step",
            "choose pick select keep remove columns table",
        ));
    }
    let columns: Vec<String> = frame
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    commands.extend(
        crate::table::TableOp::column_steps(&columns)
            .iter()
            .map(table_step_command),
    );
    commands.push(command(
        CommandId::TableOpenWindow,
        OPEN_WINDOW_TITLE,
        "A larger, resizable table window",
        "open window table larger resize sidebar",
    ));
    commands
}

/// The Table ▾ item that opens the column picker.
pub const CHOOSE_COLUMNS_TITLE: &str = "Choose columns…";

/// The Table ▾ item that opens the table window.
pub const OPEN_WINDOW_TITLE: &str = "Open in window";

/// Rows the table window shows at most (the meta line says when there are more).
pub const WINDOW_ROWS: usize = 1_000;

/// What the table window shows of the entry's table ([`table_window_view`]): the grid of the
/// version shown, its meta line, the columns for the sidebar, the versions for the version bar
/// and its Table ▾ menu. Zeroized when dropped (the grid and meta quote the table).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableWindowView {
    /// The version's grid, its first [`WINDOW_ROWS`] rows; empty while it is worked out.
    pub grid: String,
    /// Said in place of the grid when there is none ("Working on the table…").
    pub placeholder: String,
    /// "4 rows × 6 columns  ·  PII  ·  Duplicates removed", as on the card's meta line.
    pub meta: String,
    /// The sidebar: every column's (shown) name and type. No cell values.
    pub columns: Vec<crate::column_picker::PickerColumn>,
    /// "Original", then each step's label, and the version shown.
    pub versions: VersionBar,
    /// The sensitivity labels are still being checked: it cannot be revealed yet.
    pub checking: bool,
    /// The window's Table ▾ menu ([`table_window_menu`]), with its check marks.
    pub menu: Vec<(Command, bool)>,
}

impl TableWindowView {
    pub fn can_undo(&self) -> bool {
        self.versions.current > 0
    }

    pub fn can_redo(&self) -> bool {
        self.versions.current + 1 < self.versions.labels.len()
    }

    /// "v2/3".
    pub fn version_title(&self) -> String {
        format!(
            "v{}/{}",
            self.versions.current + 1,
            self.versions.labels.len().max(1)
        )
    }

    pub fn wipe(&mut self) {
        self.grid.zeroize();
        self.meta.zeroize();
        self.placeholder.zeroize();
        for column in &mut self.columns {
            column.name.zeroize();
            column.shown.zeroize();
        }
        self.columns.clear();
        for label in &mut self.versions.labels {
            label.zeroize();
        }
        for (command, _) in &mut self.menu {
            command.wipe();
        }
        self.menu.clear();
    }
}

impl Drop for TableWindowView {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// The table window's view of `table`, the table of the entry whose copied text is `source`.
/// Like the card's Dataframe view: the version's size and sensitivity labels (the original's
/// are the copied table's, as on the card), the read notes, the step of the version shown and
/// the last error in the meta line.
pub fn table_window_view(table: &TableShown, source: &str) -> TableWindowView {
    let mut view = TableWindowView::default();
    view.versions = VersionBar {
        labels: table.labels.clone(),
        current: table.version,
    };
    view.menu = table_window_menu(table);
    view.columns = picker_columns(table).unwrap_or_default();
    let Some(frame) = &table.frame else {
        view.placeholder = "Working on the table…".to_string();
        return view;
    };
    let Some(preview) = dataframe::frame_preview(frame, WINDOW_ROWS, Some(false)) else {
        view.placeholder = "The table is empty".to_string();
        return view;
    };
    let csv = Zeroizing::new(dataframe::frame_csv(frame).unwrap_or_default());
    let original = table.version == 0 && table.options == dataframe::ReadOptions::default();
    let classified: &str = if original { source } else { &csv };
    let mut meta = text_meta_from(&csv, classified);
    let (rows, columns) = frame.shape();
    let size_end = meta.find(META_SEPARATOR).unwrap_or(meta.len());
    meta.replace_range(
        ..size_end,
        &format!(
            "{} {} × {columns} {}",
            group_thousands(rows),
            if rows == 1 { "row" } else { "rows" },
            if columns == 1 { "column" } else { "columns" }
        ),
    );
    let mut notes: Vec<String> = Vec::new();
    // The step of the version shown reads first, as on the card.
    if table.version > 0
        && let Some(label) = table.labels.get(table.version)
    {
        notes.push(label.clone());
    }
    if preview.rows > preview.shown_rows {
        notes.push(showing_note(preview.shown_rows, preview.rows, "rows"));
    }
    notes.extend(
        table
            .notes
            .iter()
            .flat_map(dataframe::ReadNotes::meta_notes),
    );
    notes.extend(table.error.clone());
    // Each note goes right after the size: the last added reads first.
    for note in notes.iter().rev() {
        add_note(&mut meta, note);
    }
    view.checking = crate::sensitivity::meta_status(&meta).is_some();
    view.meta = meta;
    view.grid = preview.grid;
    view
}

/// The table window's Table ▾ menu: the card's, without what is the card's own (the column
/// picker, Open in window): the two one-click steps and the sorts.
pub fn table_window_menu(table: &TableShown) -> Vec<(Command, bool)> {
    table_menu_items(Some(table))
        .into_iter()
        .filter(|(command, _)| {
            !matches!(
                command.id,
                CommandId::TableChooseColumns | CommandId::TableOpenWindow
            )
        })
        .collect()
}

/// The columns of the version shown, for the column picker: real and shown names (a long
/// shared prefix shortened, as on the card) and friendly types. No cell values.
pub fn picker_columns(table: &TableShown) -> Option<Vec<crate::column_picker::PickerColumn>> {
    let frame = table.frame.as_ref()?;
    let names: Vec<String> = frame
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let shown = dataframe::display_names(&names);
    Some(
        frame
            .columns()
            .iter()
            .zip(names)
            .zip(shown)
            .map(
                |((column, name), shown)| crate::column_picker::PickerColumn {
                    name,
                    shown,
                    kind: dataframe::friendly_type(column.dtype()),
                },
            )
            .collect(),
    )
}

/// The version capsule in the chip row, left of the history capsule: shown in the Dataframe
/// view while the table has more than one version.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionBar {
    /// "Original", then each step's label.
    pub labels: Vec<String>,
    /// The version shown.
    pub current: usize,
}

impl VersionBar {
    pub fn of(data: &LaunchData) -> Option<Self> {
        if data.subject_kind == SubjectKind::Image {
            let edit = data.image_edit.as_ref()?;
            return (data.view == CardView::Original && edit.labels.len() > 1).then(|| Self {
                labels: edit.labels.clone(),
                current: edit.version.min(edit.labels.len() - 1),
            });
        }
        let table = data.table.as_ref()?;
        let text = data.subject_text.as_deref()?;
        (presented_view(text, data.view) == CardView::Dataframe && table.labels.len() > 1).then(
            || Self {
                labels: table.labels.clone(),
                current: table.version.min(table.labels.len() - 1),
            },
        )
    }

    /// "v2/3", the capsule's label.
    pub fn title(&self) -> String {
        format!("v{}/{}", self.current + 1, self.labels.len())
    }

    /// "Version 2 of 3, Duplicates removed": the capsule's tooltip and VoiceOver label.
    pub fn spoken(&self) -> String {
        format!(
            "Version {} of {}, {}",
            self.current + 1,
            self.labels.len(),
            self.labels[self.current]
        )
    }

    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current + 1 < self.labels.len()
    }

    /// The versions to pick from, the one shown first-class (`true`).
    pub fn menu(&self) -> Vec<(Command, bool)> {
        self.labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let title = format!("{}. {label}", index + 1);
                (
                    command(CommandId::TableVersion(index), &title, "Version", "version"),
                    index == self.current,
                )
            })
            .collect()
    }
}

/// What ⌘Z (`shift`: ⇧⌘Z) does on the card: undo or redo a table step when there is one, else
/// nothing (`None`, so a text field keeps the key). `chars` is the key without modifiers.
pub fn undo_key(
    chars: &str,
    command: bool,
    shift: bool,
    other_modifier: bool,
    bar: Option<&VersionBar>,
) -> Option<CommandId> {
    if !command || other_modifier || !chars.eq_ignore_ascii_case("z") {
        return None;
    }
    let bar = bar?;
    if shift {
        bar.can_redo().then_some(CommandId::TableRedo)
    } else {
        bar.can_undo().then_some(CommandId::TableUndo)
    }
}

/// The `⋯` row of a [`CommandId::KeepHistory`] choice.
pub fn keep_history_title(minutes: u32) -> String {
    if minutes == 0 {
        "Keep history until cleared".to_string()
    } else {
        format!("Keep history {minutes} min after the last copy")
    }
}

pub fn matching(commands: &[Command], query: &str) -> Vec<Command> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    commands
        .iter()
        .filter(|cmd| cmd.matches(&needle))
        .take(MAX_VISIBLE)
        .cloned()
        .collect()
}

/// Quit, earlier copies, and housekeeping. Copies stay out of [`chips`].
pub fn overflow(data: &LaunchData) -> Vec<Command> {
    let mut commands = source_commands(data);
    commands.push(command(
        CommandId::ClearClipboard,
        crate::locale::t("empty_pasteboard"),
        "Pasteboard only",
        "empty pasteboard clear",
    ));
    for item in &data.history {
        commands.push(command(
            CommandId::History(item.index),
            &item.title,
            &item.mark,
            "history",
        ));
    }
    if data.can_clear_history {
        commands.push(command(
            CommandId::ClearHistory,
            crate::locale::t("clear_history"),
            "Forget copies",
            "clear history forget",
        ));
    }
    commands.push(command(
        CommandId::ClearSensitive,
        crate::locale::t("clear_sensitive"),
        "Empty the pasteboard a minute after Copycraft copied a credential, PII or financial data",
        "clear sensitive pasteboard timer",
    ));
    for minutes in crate::settings::HISTORY_MINUTES {
        commands.push(command(
            CommandId::KeepHistory(minutes),
            &keep_history_title(minutes),
            "History is always forgotten when the screen locks, the Mac sleeps or you switch users",
            "keep history forget minutes",
        ));
    }
    for theme in [Theme::System, Theme::Light, Theme::Dark] {
        commands.push(command(
            CommandId::Appearance(theme),
            theme_name(theme),
            crate::locale::t("appearance"),
            "appearance theme",
        ));
    }
    commands.push(command(
        CommandId::Settings,
        crate::locale::t("settings"),
        "Hotkey, date order, blur, Open at Login",
        "settings preferences hotkey blur login",
    ));
    commands.push(command(
        CommandId::About,
        crate::locale::t("about"),
        "Version and offline promise",
        "about version offline",
    ));
    commands.push(command(
        CommandId::Quit,
        crate::locale::t("quit"),
        "Quit Copycraft",
        "quit exit",
    ));
    commands
}

/// Narrowest chip, so short titles ("Copy", "File") keep a comfortable click target.
pub const CHIP_MIN_W: f64 = 56.0;
/// Widest chip. A longer title is cut at the tail by the pill itself.
pub const CHIP_MAX_W: f64 = 220.0;

/// Laid-out width of a chip whose pill measures `measured` points after `sizeToFit`: rounded up
/// to whole points and kept within [`CHIP_MIN_W`]..=[`CHIP_MAX_W`]. NaN gives the minimum.
pub fn chip_width(measured: f64) -> f64 {
    if measured.is_nan() {
        return CHIP_MIN_W;
    }
    measured.ceil().clamp(CHIP_MIN_W, CHIP_MAX_W)
}

/// Flow chips of the given `widths` (from [`chip_width`]) into rows `width` wide.
/// `trailing` is kept clear on the right of the first row only.
pub fn layout_chips(widths: &[f64], width: f64, trailing: f64) -> Vec<ChipFrame> {
    let mut frames = Vec::with_capacity(widths.len());
    let mut x = 0.0;
    let mut row = 0usize;
    for &chip in widths {
        let limit = if row == 0 {
            (width - trailing).max(0.0)
        } else {
            width
        };
        // A first-row chip too wide for the room left of the capsules starts the next row.
        let first_and_crowded = row == 0 && trailing > 0.0;
        if (x > 0.0 || first_and_crowded) && x + chip > limit {
            row += 1;
            x = 0.0;
        }
        frames.push(ChipFrame {
            x,
            row,
            width: chip,
        });
        x += chip + CHIP_GAP;
    }
    frames
}

/// The history capsule for `len` copies with `cursor` on screen (0 is the newest entry).
/// `None` (no capsule) with fewer than two copies: there is nowhere to go. A direction with
/// nowhere to go stays visible and dimmed.
pub fn history_nav(len: usize, cursor: usize) -> Option<HistoryNav> {
    if len < 2 {
        return None;
    }
    let cursor = cursor.min(len - 1);
    Some(HistoryNav {
        can_older: cursor + 1 < len,
        can_newer: cursor > 0,
        position: cursor + 1,
        total: len,
    })
}

pub fn step_history(len: usize, cursor: usize, older: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let nav = history_nav(len, cursor)?;
    let cursor = cursor.min(len - 1);
    if older && nav.can_older {
        Some(cursor + 1)
    } else if !older && nav.can_newer {
        Some(cursor - 1)
    } else {
        None
    }
}

pub fn chips_height(frames: &[ChipFrame]) -> f64 {
    if frames.is_empty() {
        0.0
    } else {
        (frames.iter().map(|frame| frame.row).max().unwrap_or(0) + 1) as f64 * CHIP_PITCH
    }
}

pub fn step_chip(frames: &[ChipFrame], index: usize, dx: isize, dy: isize) -> usize {
    if frames.is_empty() {
        return 0;
    }
    let index = index.min(frames.len() - 1);
    if dy == 0 {
        return (index as isize + dx).clamp(0, frames.len() as isize - 1) as usize;
    }
    let current = frames[index];
    let target = current.row as isize + dy;
    let center = current.x + current.width / 2.0;
    let in_row = (target >= 0)
        .then(|| {
            frames
                .iter()
                .enumerate()
                .filter(|(_, frame)| frame.row == target as usize)
                .min_by(|(_, a), (_, b)| {
                    let da = (a.x + a.width / 2.0 - center).abs();
                    let db = (b.x + b.width / 2.0 - center).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(next, _)| next)
        })
        .flatten();
    // No row that way: the previous or next chip in order instead.
    in_row.unwrap_or_else(|| (index as isize + dy).clamp(0, frames.len() as isize - 1) as usize)
}

pub fn keeps_card_open(id: &CommandId) -> bool {
    matches!(
        id,
        CommandId::History(_)
            | CommandId::HistoryOlder
            | CommandId::HistoryNewer
            | CommandId::Appearance(_)
            | CommandId::ClearSensitive
            | CommandId::KeepHistory(_)
            | CommandId::ClearClipboard
            | CommandId::ClearHistory
            | CommandId::Settings
            | CommandId::About
            | CommandId::Clear
            | CommandId::ChooseFile
            | CommandId::UseClipboard
            | CommandId::ShowAll
            | CommandId::Original
            | CommandId::Format
            | CommandId::Convert
            | CommandId::Decode
            | CommandId::Dataframe
            | CommandId::Schema
            | CommandId::Sample
            | CommandId::Info
            | CommandId::Ocr
            | CommandId::Qr
            | CommandId::Copy
            | CommandId::Save
            | CommandId::TableStep(_)
            | CommandId::TableUndo
            | CommandId::TableRedo
            | CommandId::TableVersion(_)
            | CommandId::TableMenu
            | CommandId::ImageMenu
            | CommandId::ImageStep(_)
            | CommandId::ImageResizeCustom
            | CommandId::TableDateOrder(_)
            | CommandId::TableChooseColumns
            | CommandId::TableOpenWindow
    )
}

fn source_commands(data: &LaunchData) -> Vec<Command> {
    let mut commands = vec![command(
        CommandId::ChooseFile,
        "Choose file",
        "Open a file on the card",
        "choose file open",
    )];
    if data.source_name.is_some() {
        commands.push(command(
            CommandId::UseClipboard,
            "Clipboard",
            "Show the clipboard",
            "clipboard pasteboard",
        ));
    }
    commands
}

fn link_chips(text: &str) -> Vec<Command> {
    let mut commands = vec![command(
        CommandId::Visit,
        "Visit",
        "Open in browser",
        "visit open browser",
    )];
    if toolbar_visibility::shows_format(text) {
        commands.push(command(
            CommandId::Format,
            "Format",
            "URI",
            "format url uri",
        ));
    }
    commands
}

fn text_chips(text: &str) -> Vec<Command> {
    let kind = format::detect(text);
    let mut commands = Vec::new();
    // The cheap check first: kinds that open formatted never get the chip, so they skip the
    // formatter (slow on a large copy).
    if !opens_formatted(text) && toolbar_visibility::shows_format(text) {
        commands.push(command(
            CommandId::Format,
            "Format",
            kind.source_heading(),
            "format pretty print",
        ));
    }
    if kind == FormatKind::Json && crate::validate::broken_json(text).is_none() {
        commands.push(command(
            CommandId::Schema,
            "Schema",
            "Avro schema",
            "schema avro",
        ));
    } else if kind == FormatKind::Xml && crate::xsd_schema::is_xsd(text) {
        if crate::xsd_schema::try_sample(text).is_some() {
            commands.push(command(
                CommandId::Sample,
                "Sample",
                "Sample XML",
                "sample xml example",
            ));
        }
    } else if kind == FormatKind::Xml && crate::xsd_schema::try_schema(text).is_some() {
        commands.push(command(
            CommandId::Schema,
            "Schema",
            "XSD schema",
            "schema xsd xml",
        ));
    }
    // YAML as JSON: a view of the entry (Copy and Save take it, as `.json`). Never on JSON,
    // which is YAML too but detected as JSON first.
    if kind == FormatKind::Yaml && crate::transform::yaml_to_json(text).is_some() {
        commands.push(command(
            CommandId::Convert,
            TO_JSON_TITLE,
            "YAML as JSON",
            "to json convert yaml",
        ));
    }
    if toolbar_visibility::shows_convert(text) {
        commands.push(command(
            CommandId::Convert,
            "Convert",
            "Another format",
            "convert json key value",
        ));
    }
    if toolbar_visibility::shows_decode(kind) && decode::try_decode(text).is_some() {
        commands.push(command(
            CommandId::Decode,
            "Decode",
            "Encoded text",
            "decode jwt base64 percent",
        ));
    }
    if toolbar_visibility::shows_dataframe_button(kind, text) {
        commands.push(command(
            CommandId::TableMenu,
            table_menu_title(),
            "Steps on the table",
            "table steps dedupe duplicates undo redo",
        ));
    }
    finish_modes(commands)
}

fn image_chips(scan: Option<&ImageScan>, edits: bool) -> Vec<Command> {
    let mut modes = vec![command(
        CommandId::Info,
        "Info",
        "Size and data URL",
        "info data url size",
    )];
    if let Some(scan) = scan {
        if scan.ocr.is_some() {
            modes.push(command(
                CommandId::Ocr,
                "Text",
                "Recognized text",
                "text ocr recognize",
            ));
        }
        if scan.qr.is_some() {
            modes.push(command(
                CommandId::Qr,
                "QR",
                "Barcode payload",
                "qr barcode",
            ));
        }
    }
    if edits {
        modes.push(command(
            CommandId::ImageMenu,
            image_menu_title(),
            "Steps on the picture",
            "image picture resize rotate flip grayscale metadata undo redo",
        ));
    }
    let mut commands = vec![original_command()];
    commands.extend(modes);
    commands
}

fn finish_modes(modes: Vec<Command>) -> Vec<Command> {
    let mut commands = Vec::new();
    if !modes.is_empty() {
        commands.push(original_command());
    }
    commands.extend(modes);
    commands
}

fn original_command() -> Command {
    command(
        CommandId::Original,
        crate::locale::t("original"),
        "Source",
        "original source",
    )
}

/// Copy and Save sit on the well, on the view that is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentActions {
    pub copy: bool,
    pub save: bool,
}

pub fn content_actions(data: &LaunchData) -> ContentActions {
    match data.subject_kind {
        SubjectKind::Empty
        | SubjectKind::NoText
        | SubjectKind::Hidden
        | SubjectKind::Denied
        | SubjectKind::Pending
        | SubjectKind::PasteAsk => ContentActions {
            copy: false,
            save: false,
        },
        SubjectKind::Image => image_content_actions(data),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            if crate::youtube::video_id(text).is_some()
                || crate::page_preview::page_url(text).is_some()
            {
                // Copy the URL. Nothing to save.
                ContentActions {
                    copy: true,
                    save: false,
                }
            } else {
                ContentActions {
                    copy: true,
                    save: true,
                }
            }
        }
    }
}

fn image_content_actions(data: &LaunchData) -> ContentActions {
    let scan = data.image_scan.as_ref();
    let copy = match data.view {
        CardView::Info => scan.is_some(),
        CardView::Ocr | CardView::Qr => image_view_text(scan, data.view).is_some(),
        _ => false,
    };
    ContentActions { copy, save: true }
}

pub fn wipe_commands(commands: &mut [Command]) {
    for command in commands {
        command.wipe();
    }
}

pub fn copy_tip(title: &str) -> String {
    format!("Copy {title}")
}

pub fn save_tip(title: &str) -> String {
    format!("Save {title}")
}

fn image_meta(facts: &ImageFacts) -> String {
    let mut parts = Vec::new();
    if !facts.format.is_empty() {
        parts.push(facts.format.clone());
    }
    if facts.width > 0 && facts.height > 0 {
        parts.push(format!("{}×{}", facts.width, facts.height));
    }
    if facts.byte_len > 0 {
        parts.push(format_bytes(facts.byte_len));
    }
    parts.join("  ")
}

fn text_meta(text: &str) -> String {
    text_meta_from(text, text)
}

/// Size and line count come from `measured`. Classification comes from
/// `classified`, which stays the copied table when the card shows another rendering.
fn text_meta_from(measured: &str, classified: &str) -> String {
    let size = format_bytes(measured.len());
    let lines = measured.lines().count();
    let mut meta = if lines > 1 {
        format!("{lines} lines  {size}")
    } else {
        size
    };
    match crate::sensitivity::labeling(classified) {
        crate::sensitivity::Labeling::Known(found) => {
            let labels = &found.labels;
            if !labels.is_empty() {
                meta.push_str("  ·  ");
                meta.push_str(&crate::sensitivity::label_line(labels));
            }
        }
        crate::sensitivity::Labeling::Checking => {
            meta.push_str("  ·  ");
            meta.push_str(crate::sensitivity::CHECKING);
        }
    }
    meta
}

/// The card's labels are still being checked: it cannot be revealed, and it is not kept with
/// its history entry (the next build has the labels).
pub fn is_checking(card: &WorkCard) -> bool {
    crate::sensitivity::meta_status(&card.meta) == Some(crate::sensitivity::CHECKING)
}

/// Pasted size. Megabytes from 0.01 MB up; kilobytes below that.
fn format_bytes(n: usize) -> String {
    if n < 10_000 {
        format_kilobytes(n)
    } else {
        format_megabytes(n)
    }
}

fn format_megabytes(n: usize) -> String {
    let mb = n as f64 / 1_000_000.0;
    let places = if mb >= 100.0 {
        0
    } else if mb >= 10.0 {
        1
    } else {
        2
    };
    format!("{mb:.places$} MB")
}

fn format_kilobytes(n: usize) -> String {
    if n == 0 {
        return "0 KB".to_string();
    }
    let kb = n as f64 / 1_000.0;
    if kb >= 1.0 {
        return format!("{kb:.1} KB");
    }
    let mut body = format!("{kb:.3}");
    while body.contains('.') && body.ends_with('0') {
        body.pop();
    }
    format!("{body} KB")
}

fn clip_chars(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let body: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{body}…")
    } else {
        body
    }
}

fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::System => crate::locale::t("theme_system"),
        Theme::Light => crate::locale::t("theme_light"),
        Theme::Dark => crate::locale::t("theme_dark"),
    }
}

fn command(id: CommandId, title: &str, detail: &str, keywords: &str) -> Command {
    Command {
        id,
        title: title.to_string(),
        detail: detail.to_string(),
        keywords: keywords.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::picker_columns;
    use super::{ArrowAction, ArrowFocus, arrow_action};
    use super::{CHIP_PILL_H, NAV_BUTTON, NAV_COUNT_W, NEXT_CHEVRON, PREVIOUS_CHEVRON};
    use super::{
        CardView, CommandId, ContentActions, Hist, ImageFacts, ImageScan, LaunchData, NAV_RESERVE,
        NAV_SPAN, SubjectKind, chip_width, chips, content_actions, content_key, copy_tip,
        deferred_save_name, history_label, history_nav, keeps_card_open, layout_chips,
        masks_content, matching, overflow, payload_excerpt, presented_view, save_tip, search_pool,
        stays_revealed, step_chip, step_history, text_save_file, transformed_text, well_mask,
        work_card,
    };
    use super::{OPEN_WINDOW_TITLE, WINDOW_ROWS, table_window_view};
    use super::{PREVIEW_CHARS, PREVIEW_ROWS, excerpt_for, group_thousands, showing_note};
    use super::{TABLE_MENU_TITLE, TableShown, VersionBar, menu_group, table_menu, undo_key};
    use crate::appearance::Theme;
    use mac_ui::keys::Key;

    fn data(kind: SubjectKind, text: Option<&str>) -> LaunchData {
        LaunchData {
            subject_kind: kind,
            subject_text: text.map(str::to_string),
            image: None,
            history: Vec::new(),
            can_clear_history: false,
            history_nav: None,
            theme: Theme::System,
            settings: crate::settings::Settings::default(),
            view: CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
            full: false,
            picture: None,
            table: None,
            image_edit: None,
        }
    }

    #[test]
    fn hidden_copy_shows_no_text_and_no_actions() {
        let hidden = data(SubjectKind::Hidden, None);
        let card = work_card(&hidden);
        assert_eq!(card.title, "Hidden content");
        assert!(card.excerpt.is_empty() && card.meta.is_empty());
        assert!(chips(&hidden).is_empty());
        let actions = content_actions(&hidden);
        assert!(!actions.copy && !actions.save);
    }

    #[test]
    fn remembered_chips_match_the_text() {
        let texts = [
            "{\"a\": [1, 2]}",
            "fn main() { let x = 1; }",
            "name,age\nalice,30\nbob,40",
            "a: 1\nb: [2, 3]\n",
            "aGVsbG8gd29ybGQ=",
            "https://example.com/page",
            "plain words",
        ];
        for text in texts {
            let input = data(SubjectKind::Text, Some(text));
            let first = ids(&super::chips(&input));
            assert_eq!(ids(&super::chips(&input)), first, "{text}");
            super::forget_chips();
            assert_eq!(ids(&super::chips(&input)), first, "{text}");
        }
        // Each text has its own chips, also when they are short and the same length.
        let json = ids(&super::chips(&data(SubjectKind::Text, Some("{\"a\":1}"))));
        let rust = ids(&super::chips(&data(SubjectKind::Text, Some("fn a() {}"))));
        assert!(json.contains(&CommandId::Schema));
        assert!(!rust.contains(&CommandId::Schema));
    }

    fn titles(commands: &[super::Command]) -> Vec<String> {
        commands.iter().map(|cmd| cmd.title.clone()).collect()
    }

    fn ids(commands: &[super::Command]) -> Vec<CommandId> {
        commands.iter().map(|cmd| cmd.id.clone()).collect()
    }

    fn housekeeping(id: &CommandId) -> bool {
        matches!(id, CommandId::Quit | CommandId::ClearHistory)
    }

    #[test]
    fn clear_stays_on_the_open_card() {
        assert!(keeps_card_open(&CommandId::Clear));
    }

    #[test]
    fn messy_json_hides_format_convert_and_dataframe() {
        let input = data(SubjectKind::Text, Some(r#"{"name":"copycraft"}"#));
        let shown = chips(&input);
        assert_eq!(shown[0].id, CommandId::Original);
        assert_eq!(shown[1].id, CommandId::Schema);
        assert_eq!(shown.last().unwrap().id, CommandId::Schema);
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Copy));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Save));
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Dataframe));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Decode));
        assert!(shown.iter().all(|cmd| !housekeeping(&cmd.id)));
    }

    #[test]
    fn pretty_json_skips_format() {
        let pretty = "{\n  \"name\": \"copycraft\"\n}";
        let shown = chips(&data(SubjectKind::Text, Some(pretty)));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
        let card = work_card(&data(SubjectKind::Text, Some(pretty)));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
        assert_eq!(card.excerpt, pretty);
        assert_eq!(ids(&shown), vec![CommandId::Original, CommandId::Schema]);
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
    }

    #[test]
    fn schema_view_shows_the_avro_schema() {
        let src = r#"{"name":"copycraft","version":1,"ok":true}"#;
        let mut input = data(SubjectKind::Text, Some(src));
        assert_eq!(
            ids(&matching(&search_pool(&input), "avro")),
            vec![CommandId::Schema]
        );
        input.view = CardView::Schema;
        let card = work_card(&input);
        let body = transformed_text(src, CardView::Schema).unwrap();
        assert_eq!(card.title, "Avro schema");
        assert_eq!(copy_tip(&card.title), "Copy Avro schema");
        assert_eq!(save_tip(&card.title), "Save Avro schema");
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        assert_eq!(card.excerpt, body);
        assert!(card.excerpt.contains("\"type\": \"record\""));
        assert!(card.excerpt.contains("\"name\": \"document\""));
        assert!(card.excerpt.contains("\"version\""));
        assert!(!card.excerpt.contains("copycraft"));
        assert!(!card.excerpt.ends_with('…'));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
        assert!(card.selectable);
        let file = text_save_file(src, CardView::Schema).unwrap();
        assert_eq!(file.filename, "clipboard.avsc");
        assert_eq!(file.extension, "avsc");
        assert_eq!(file.bytes, body.as_bytes());
    }

    #[test]
    fn xml_opens_formatted_and_coloured_with_xsd() {
        let src = r#"<person id="1"><name>Ada</name><age>36</age></person>"#;
        let mut input = data(SubjectKind::Text, Some(src));
        assert_eq!(
            ids(&matching(&search_pool(&input), "xsd")),
            vec![CommandId::Schema]
        );
        let shown = chips(&input);
        assert_eq!(ids(&shown), vec![CommandId::Original, CommandId::Schema]);
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        let pretty = transformed_text(src, CardView::Format).unwrap();
        assert_ne!(pretty, src);
        assert_eq!(card.excerpt, pretty);
        assert!(card.excerpt.contains("<name>Ada</name>"));
        assert!(card.excerpt.contains("<age>36</age>"));
        assert!(!card.excerpt.ends_with('…'));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
        assert_eq!(card.title, "Valid XML");
        assert_eq!(copy_tip(&card.title), "Copy Valid XML");
        input.view = CardView::Schema;
        let card = work_card(&input);
        let body = transformed_text(src, CardView::Schema).unwrap();
        assert_eq!(card.title, "XSD schema");
        assert_eq!(save_tip(&card.title), "Save XSD schema");
        assert_eq!(card.excerpt, body);
        assert!(card.excerpt.contains("<xs:schema"));
        assert!(card.excerpt.contains("name=\"person\""));
        assert!(card.excerpt.contains("name=\"age\""));
        assert!(card.excerpt.contains("xs:int"));
        assert!(!card.excerpt.contains("Ada"));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
        assert!(card.selectable);
        let file = text_save_file(src, CardView::Schema).unwrap();
        assert_eq!(file.filename, "clipboard.xsd");
        assert_eq!(file.extension, "xsd");
        assert_eq!(file.bytes, body.as_bytes());
    }

    #[test]
    fn xsd_opens_formatted_and_coloured_with_sample() {
        let src = r#"<?xml version="1.0"?><xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="person"><xs:complexType><xs:sequence><xs:element name="name" type="xs:string"/><xs:element name="age" type="xs:int"/></xs:sequence><xs:attribute name="id" type="xs:int" use="required"/></xs:complexType></xs:element></xs:schema>"#;
        let mut input = data(SubjectKind::Text, Some(src));
        assert_eq!(
            ids(&matching(&search_pool(&input), "sample")),
            vec![CommandId::Sample]
        );
        let shown = chips(&input);
        assert_eq!(ids(&shown), vec![CommandId::Original, CommandId::Sample]);
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Schema));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        let pretty = transformed_text(src, CardView::Format).unwrap();
        assert_ne!(pretty, src);
        assert_eq!(card.excerpt, pretty);
        assert!(card.excerpt.contains('\n'));
        assert!(card.excerpt.contains("<xs:schema"));
        assert!(!card.excerpt.ends_with('…'));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
        assert_eq!(card.title, "Valid XML");
        input.view = CardView::Sample;
        let card = work_card(&input);
        let body = transformed_text(src, CardView::Sample).unwrap();
        assert_eq!(card.title, "Sample XML");
        assert_eq!(copy_tip(&card.title), "Copy Sample XML");
        assert_eq!(save_tip(&card.title), "Save Sample XML");
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        assert_eq!(card.excerpt, body);
        assert!(card.excerpt.contains("<person"));
        assert!(card.excerpt.contains("<name>sample</name>"));
        assert!(card.excerpt.contains("<age>1</age>"));
        assert!(!card.excerpt.ends_with('…'));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
        assert!(card.selectable);
        let file = text_save_file(src, CardView::Sample).unwrap();
        assert_eq!(file.filename, "clipboard.xml");
        assert_eq!(file.extension, "xml");
        assert_eq!(file.bytes, body.as_bytes());
    }

    #[test]
    fn tabular_xml_shows_schema_without_convert() {
        let src = "<root><person><name>Jan</name><age>30</age></person><person><name>Anja</name><age>40</age></person></root>";
        let shown = chips(&data(SubjectKind::Text, Some(src)));
        assert_eq!(ids(&shown), vec![CommandId::Original, CommandId::Schema]);
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
        assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Dataframe));
    }

    #[test]
    fn yaml_opens_formatted_and_coloured_with_to_json() {
        let src = "name:   copycraft\nitems:\n  - one\n";
        let input = data(SubjectKind::Text, Some(src));
        assert_eq!(crate::format::detect(src), crate::format::FormatKind::Yaml);
        let shown = chips(&input);
        assert_eq!(titles(&shown), vec!["Original", "To JSON"]);
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Format));
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Schema));
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Dataframe));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        let pretty = transformed_text(src, CardView::Format).unwrap();
        assert_ne!(pretty, src);
        assert_eq!(card.excerpt, pretty);
        assert!(card.excerpt.contains("name:"));
        assert!(card.excerpt.contains("copycraft"));
        assert!(card.excerpt.contains("items:"));
        assert!(card.excerpt.contains("one"));
        assert!(!card.excerpt.contains("name:   "));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Yaml));
        assert_eq!(card.title, "YAML");
        let painted: String =
            crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Yaml)
                .into_iter()
                .map(|(_, text)| text)
                .collect();
        assert_eq!(painted, card.excerpt);
        let save = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
        assert_eq!(save.extension, "yaml");
        assert_eq!(save.filename, "clipboard.yaml");
        assert_eq!(save.bytes, pretty.as_bytes());

        let tidy = "name: copycraft\nitems:\n- one\n";
        let tidy_card = work_card(&data(SubjectKind::Text, Some(tidy)));
        assert_eq!(tidy_card.highlight, Some(crate::format::FormatKind::Yaml));
        assert_eq!(tidy_card.title, "YAML");
        assert!(tidy_card.excerpt.contains("copycraft"));
        assert!(
            chips(&data(SubjectKind::Text, Some(tidy)))
                .iter()
                .all(|cmd| cmd.id != CommandId::Format)
        );
    }

    #[test]
    fn broken_json_stays_json_says_where_and_marks_the_line() {
        let src = "{\n  \"id\": 42,\n  \"tags\": [\"a\", \"b\"],\n}";
        let input = data(SubjectKind::Text, Some(src));
        let card = work_card(&input);
        assert_eq!(card.title, "JSON · Invalid");
        assert_eq!(card.excerpt, src);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
        assert_eq!(
            card.meta,
            "4 lines  0.037 KB  ·  Line 4, column 1: trailing comma"
        );
        assert_eq!(card.error_line, Some(4));
        assert!(chips(&input).is_empty(), "no Schema, no To JSON");
        // Labels stay at the end of the meta line.
        let mail = "{\"mail\": \"jan.devries@example.nl\",}";
        let card = work_card(&data(SubjectKind::Text, Some(mail)));
        assert!(
            card.meta
                .ends_with("Line 1, column 35: trailing comma  ·  PII"),
            "{}",
            card.meta
        );
        // `{a: 1}` and a normal YAML document stay YAML.
        for yaml in [
            "{a: 1}",
            "name: \"x\"\ntags: [\"a\", \"b\"]\nnested:\n  key: \"v\"\n",
        ] {
            let card = work_card(&data(SubjectKind::Text, Some(yaml)));
            assert_eq!(card.title, "YAML", "{yaml}");
            assert_eq!(card.error_line, None);
        }
        assert_eq!(super::line_range("ab\ncd\r\nef", 2), Some(3..5));
        assert_eq!(super::line_range("ab", 2), None);
    }

    #[test]
    fn yaml_to_json_is_a_view_that_copy_and_save_take_as_json() {
        let src = "base: &b\n  email: jan.devries@example.nl\nuser:\n  <<: *b\n  id: 7\n";
        let mut input = data(SubjectKind::Text, Some(src));
        let to_json = chips(&input)
            .into_iter()
            .find(|cmd| cmd.title == "To JSON")
            .expect("To JSON chip");
        assert_eq!(to_json.id, CommandId::Convert);
        input.view = CardView::from_command(&to_json.id).expect("view");
        let card = work_card(&input);
        let body = transformed_text(src, CardView::Convert).expect("json");
        let value: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(value["user"]["email"], "jan.devries@example.nl");
        assert_eq!(value["user"]["id"], 7);
        assert_eq!(card.excerpt, body);
        assert_eq!(card.title, "JSON");
        // The labels are the result's; the well is masked like any copied text.
        assert!(card.meta.ends_with("  ·  PII"), "{}", card.meta);
        assert!(masks_content(&card, CardView::Convert));
        let save = text_save_file(src, CardView::Convert).expect("save");
        assert_eq!(
            (save.filename.as_str(), save.extension),
            ("clipboard.json", "json")
        );
        assert_eq!(save.bytes, body.as_bytes());
        // Never on JSON (also YAML), nor on YAML that does not parse.
        for not_yaml in [r#"{"a": 1}"#, "[1, 2, 3]", "a: [1, 2\nb: 3\n"] {
            let shown = chips(&data(SubjectKind::Text, Some(not_yaml)));
            assert!(shown.iter().all(|cmd| cmd.title != "To JSON"), "{not_yaml}");
        }
    }

    #[test]
    fn song_yaml_opens_formatted_with_to_json() {
        let src = "\
---
doe: \"a deer, a female deer\"
ray: \"a drop of golden sun\"
pi: 3.14159
xmas: true
french-hens: 3
calling-birds:
  - huey
  - dewey
  - louie
  - fred
xmas-fifth-day:
  calling-birds: four
  french-hens: 3
  golden-rings: 5
  partridges:
    count: 1
    location: \"a pear tree\"
  turtle-doves: two
";
        assert_eq!(crate::format::detect(src), crate::format::FormatKind::Yaml);
        let input = data(SubjectKind::Text, Some(src));
        let shown = chips(&input);
        assert_eq!(titles(&shown), vec!["Original", "To JSON"]);
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Format));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        let pretty = transformed_text(src, CardView::Format).unwrap();
        assert_eq!(card.excerpt, pretty);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Yaml));
        assert_eq!(card.title, "YAML");
        assert!(card.excerpt.contains("a deer, a female deer"));
        assert!(card.excerpt.contains("huey"));
        assert!(card.excerpt.contains("a pear tree"));
        assert!(card.excerpt.contains("3.14159"));
        let painted: String =
            crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Yaml)
                .into_iter()
                .map(|(_, text)| text)
                .collect();
        assert_eq!(painted, card.excerpt);
        let save = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
        assert_eq!(save.extension, "yaml");
        assert_eq!(save.bytes, pretty.as_bytes());
    }

    #[test]
    fn table_opens_formatted_and_coloured() {
        let samples = [
            (
                "name,age\nalice,30\nbob,40",
                crate::format::FormatKind::Csv,
                "age",
                "30",
                "40",
            ),
            (
                "name\tage\nalice\t30\nbob\t40",
                crate::format::FormatKind::Tsv,
                "age",
                "30",
                "40",
            ),
            (
                "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900",
                crate::format::FormatKind::Csv,
                "Salaris",
                "3450",
                "2900",
            ),
        ];
        for (src, kind, header, first, second) in samples {
            let input = data(SubjectKind::Text, Some(src));
            assert_eq!(presented_view(src, CardView::Original), CardView::Original);
            let card = work_card(&input);
            assert_eq!(card.highlight, Some(kind), "{src}");
            assert!(card.selectable, "{src}");
            assert!(!card.excerpt.contains('┆'), "{src}\n{}", card.excerpt);
            assert!(!card.excerpt.contains("shape"), "{src}");
            let lines: Vec<&str> = card.excerpt.lines().collect();
            assert!(lines.len() >= 3, "{src}");
            let column = lines[0].find(header).expect(header);
            assert_eq!(
                lines[1].find(first),
                Some(column),
                "{src}\n{}",
                card.excerpt
            );
            assert_eq!(
                lines[2].find(second),
                Some(column),
                "{src}\n{}",
                card.excerpt
            );
            let painted: String = crate::highlight::tokens(&card.excerpt, kind)
                .into_iter()
                .map(|(_, text)| text)
                .collect();
            assert_eq!(painted, card.excerpt, "{src}");
            assert!(
                !chips(&input).iter().any(|cmd| cmd.id == CommandId::Format),
                "{src}"
            );
            assert!(
                chips(&input)
                    .iter()
                    .any(|cmd| cmd.id == CommandId::TableMenu),
                "{src}"
            );
            let file = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
            assert_ne!(file.extension, "parquet", "{src}");
            assert_eq!(file.bytes, src.as_bytes(), "{src}");
        }
        let quoted = "name,city\nalice,\"Den Haag, NL\"\nbob,Utrecht";
        let card = work_card(&data(SubjectKind::Text, Some(quoted)));
        let lines: Vec<&str> = card.excerpt.lines().collect();
        let city = lines[0].find("city").expect("city");
        assert_eq!(lines[1].find('"'), Some(city), "{}", card.excerpt);
        assert_eq!(lines[2].find("Utrecht"), Some(city), "{}", card.excerpt);
        assert!(card.excerpt.contains("\"Den Haag, NL\""));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Csv));
        let salary = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
        let card = work_card(&data(SubjectKind::Text, Some(salary)));
        assert_eq!(card.title, "CSV");
        assert!(card.meta.contains("PII · financial"), "{}", card.meta);
        assert!(!card.excerpt.contains('\u{00b7}'));
        let tsv = work_card(&data(
            SubjectKind::Text,
            Some("name\tage\nalice\t30\nbob\t40"),
        ));
        assert!(tsv.excerpt.contains('\u{00b7}'), "{}", tsv.excerpt);
        assert!(!tsv.excerpt.contains('\t'), "{}", tsv.excerpt);
    }

    #[test]
    fn table_hides_convert() {
        for src in [
            "name,age\nalice,30\nbob,40",
            "name\tage\nalice\t30\nbob\t40",
            "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900",
        ] {
            let shown = chips(&data(SubjectKind::Text, Some(src)));
            assert!(
                !shown.iter().any(|cmd| cmd.id == CommandId::Convert),
                "{src}"
            );
            assert!(
                shown.iter().any(|cmd| cmd.id == CommandId::TableMenu),
                "{src}"
            );
        }
    }

    #[test]
    fn encoded_text_offers_decode() {
        let shown = chips(&data(
            SubjectKind::Text,
            Some("eyJuYW1lIjoiY29weWNyYWZ0In0="),
        ));
        assert!(shown.iter().any(|cmd| cmd.id == CommandId::Decode));
    }

    #[test]
    fn prose_with_email_has_no_redact_button() {
        let text = "mail me at jan.devries@email.nl please";
        let input = data(SubjectKind::Text, Some(text));
        let card = work_card(&input);
        assert!(card.meta.contains("PII"), "{}", card.meta);
    }

    #[test]
    fn sensitive_copy_labels_the_meta_without_a_redact_button() {
        let text = "aws creds AKIAIOSFODNN7EXAMPLE rotated";
        let input = data(SubjectKind::Text, Some(text));
        let card = work_card(&input);
        assert!(card.meta.contains("credential"));
        assert!(!card.meta.contains("AKIAIOSFODNN7EXAMPLE"));
        let rust = "fn main() {\n    let key = \"AKIAIOSFODNN7EXAMPLE\";\n}\n";
        let rust_input = data(SubjectKind::Text, Some(rust));
        let card = work_card(&rust_input);
        assert!(card.meta.contains("credential"));
        assert!(!card.meta.contains("PII"), "{}", card.meta);
        assert!(card.excerpt.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn rust_code_is_not_labeled_pii_and_hides_redact() {
        let src = "\
use std::io;
use std::collections::HashMap;

fn main() {
    let name: String = String::new();
    let host = \"127.0.0.1\";
}
";
        let input = data(SubjectKind::Text, Some(src));
        let card = work_card(&input);
        assert!(!card.meta.contains("PII"), "{}", card.meta);
        assert!(!card.meta.contains("financial"), "{}", card.meta);
        assert!(!card.meta.contains("credential"), "{}", card.meta);
    }

    #[test]
    fn credential_url_stays_a_link_card() {
        let url = "https://deploy:s3cr3t@github.com/acme/app.git";
        let input = data(SubjectKind::Text, Some(url));
        let card = work_card(&input);
        assert!(card.link_page.is_some());
        assert_eq!(card.title, "Link");
        assert!(!card.title.contains("deploy"));
        assert!(!card.title.contains("s3cr3t"));
        assert!(card.meta.contains("credential"));
        assert!(!card.meta.contains("s3cr3t"));
        assert!(!card.meta.contains("deploy"));
        assert!(crate::page_preview::url_has_userinfo(url));
        assert!(!crate::page_preview::url_has_userinfo(
            "https://github.com/acme/app"
        ));
        assert_eq!(
            chips(&input)
                .iter()
                .map(|cmd| cmd.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Visit"]
        );
    }

    #[test]
    fn failed_blur_hides_the_excerpt_until_reveal() {
        let secret = "https://deploy:s3cr3t@github.com/acme/app.git";
        let failed = well_mask(true, false);
        assert!(!failed.blur);
        assert!(failed.shade);
        assert!(!failed.show_body);
        let shown = if failed.show_body { secret } else { "" };
        assert!(shown.is_empty());
        assert!(!shown.contains("deploy"));
        assert!(!shown.contains("s3cr3t"));
        let blurred = well_mask(true, true);
        assert!(blurred.blur);
        assert!(!blurred.shade);
        assert!(blurred.show_body);
        let open = well_mask(false, false);
        assert!(!open.blur);
        assert!(!open.shade);
        assert!(open.show_body);
        let card = work_card(&data(SubjectKind::Text, Some(secret)));
        assert!(masks_content(&card, CardView::Original));
        assert_ne!(
            content_key(&data(SubjectKind::Text, Some(secret))),
            content_key(&data(SubjectKind::Text, Some("another copy")))
        );
    }

    #[test]
    fn long_prose_keeps_copy_and_save_on_the_content() {
        let text = "please summarize these notes for the team. ".repeat(40);
        let input = data(SubjectKind::Text, Some(&text));
        assert!(chips(&input).is_empty());
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
    }

    #[test]
    fn code_is_formatted_without_original_or_format() {
        let src = "fn main() {\nlet message = \"this identifier is definitely longer than forty eight characters\";\n}\n";
        let input = data(SubjectKind::Text, Some(src));
        assert!(chips(&input).is_empty());
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let body = transformed_text(src, CardView::Format).unwrap();
        assert_ne!(body, src);
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Rust));
        assert_eq!(card.title, "Formatted Rust");
        assert!(card.excerpt.contains("forty eight characters"));
        assert!(card.excerpt.contains("    let message"));
        assert!(!card.excerpt.contains('…'));

        let java = "public class Hi {\npublic static void main(String[] a) {\nSystem.out.println(1);\n}\n}\n";
        let input = data(SubjectKind::Text, Some(java));
        assert!(chips(&input).is_empty());
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Java));
        assert_eq!(card.title, "Formatted Java");
        assert!(card.excerpt.contains("    System.out.println"));
    }

    #[test]
    fn pretty_code_stays_on_the_card_without_original() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        let input = data(SubjectKind::Text, Some(src));
        assert!(chips(&input).is_empty());
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Rust));
        assert!(card.excerpt.contains("let x"));
        assert_eq!(
            presented_view(r#"{"a":1}"#, CardView::Original),
            CardView::Format
        );
    }

    #[test]
    fn python_cards_are_highlighted_and_checked() {
        let sound = "def greet(name):\n    print(f\"Hello {name}\")\n";
        let input = data(SubjectKind::Text, Some(sound));
        let card = work_card(&input);
        assert_eq!(card.title, "Python");
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Python));
        assert_eq!(card.excerpt, sound, "no formatter: shown as copied");
        let shown = chips(&input);
        assert!(!shown.iter().any(|cmd| matches!(
            cmd.id,
            CommandId::Format | CommandId::Convert | CommandId::Schema
        )));

        let missing = "def greet(name):\n    print(f\"Hello {name}\"\n";
        assert_eq!(
            work_card(&data(SubjectKind::Text, Some(missing))).title,
            "Python · missing )"
        );
        let no_body = "import os\n\ndef main():\nprint(os.getcwd())\n";
        assert_eq!(
            work_card(&data(SubjectKind::Text, Some(no_body))).title,
            "Python · expected indent"
        );
        let mixed = "def main():\n    x = 1\n\tprint(x)\n";
        assert_eq!(
            work_card(&data(SubjectKind::Text, Some(mixed))).title,
            "Python · mixed tabs and spaces"
        );
        assert_eq!(
            work_card(&data(SubjectKind::Text, Some("f = open(\"demofile.txt\")"))).title,
            "Content"
        );
    }

    #[test]
    fn html_shows_no_schema_and_an_html_title() {
        for src in [
            "<!DOCTYPE html>\n<html><head><title>Hi</title></head><body><p>Hello</p></body></html>",
            "<html><body><p>Jan</p><p>Anja</p></body></html>",
        ] {
            let input = data(SubjectKind::Text, Some(src));
            let shown = chips(&input);
            assert!(
                !shown.iter().any(|cmd| cmd.id == CommandId::Schema),
                "{src}"
            );
            assert!(
                !shown.iter().any(|cmd| cmd.id == CommandId::Sample),
                "{src}"
            );
            assert!(
                !shown.iter().any(|cmd| cmd.id == CommandId::Convert),
                "{src}"
            );
            let card = work_card(&input);
            assert_eq!(card.title, "HTML", "{src}");
            assert_eq!(card.highlight, Some(crate::format::FormatKind::Html));
            assert_eq!(
                text_save_file(src, CardView::Format).unwrap().extension,
                "html"
            );
        }
        // Plain XML keeps its Schema chip.
        let xml =
            "<root><person><name>Jan</name></person><person><name>Anja</name></person></root>";
        assert!(
            chips(&data(SubjectKind::Text, Some(xml)))
                .iter()
                .any(|cmd| cmd.id == CommandId::Schema)
        );
    }

    #[test]
    fn unbalanced_code_says_so_in_the_title() {
        let java = "public class Main {\n  public static void main(String[] args) {\n    System.out.println(\"Hello World\");\n  }\n";
        let input = data(SubjectKind::Text, Some(java));
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Java));
        assert_eq!(card.title, "Java · missing }");
        // Still formatted, and no bracket is added.
        assert!(card.excerpt.contains("        System.out.println"));
        assert_eq!(card.excerpt.matches('}').count(), java.matches('}').count());

        let rust = "fn main() {\n    run());\n}\n";
        let card = work_card(&data(SubjectKind::Text, Some(rust)));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Rust));
        assert_eq!(card.title, "Rust · unbalanced )");

        let balanced = format!("{java}}}\n");
        assert_eq!(
            work_card(&data(SubjectKind::Text, Some(&balanced))).title,
            "Formatted Java"
        );
    }

    #[test]
    fn image_card_is_a_picture_with_encoding_chips() {
        let mut input = data(SubjectKind::Image, None);
        input.image = Some(ImageFacts {
            format: "PNG".into(),
            width: 1280,
            height: 720,
            byte_len: 184_000,
        });
        let card = work_card(&input);
        assert_eq!(card.title, "Image");
        assert_eq!(card.meta, "PNG  1280×720  0.18 MB");
        assert!(card.shows_image);
        assert!(card.excerpt.is_empty());
        assert_eq!(titles(&chips(&input)), vec!["Original", "Info"]);
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: true
            }
        );
        input.view = CardView::Info;
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: true
            }
        );
        let info = work_card(&input);
        assert_eq!(info.title, "Image info");
        assert!(!info.shows_image);
        assert!(info.selectable);
        assert!(info.excerpt.contains("1280×720"));
    }

    fn edited_picture(version: usize, labels: &[&str]) -> LaunchData {
        let mut input = data(SubjectKind::Image, None);
        input.image = Some(ImageFacts {
            format: "PNG".into(),
            width: 1024,
            height: 768,
            byte_len: 184_000,
        });
        input.image_edit = Some(super::ImageShown {
            entry: 7,
            labels: labels.iter().map(|label| label.to_string()).collect(),
            version,
            working: false,
            note: None,
        });
        input
    }

    #[test]
    fn a_picture_with_versions_has_image_menu_and_version_bar() {
        use crate::image_edit::{ImageOp, Resize};
        let plain = edited_picture(0, &["Original"]);
        assert_eq!(titles(&chips(&plain)), vec!["Original", "Info", "Image ▾"]);
        assert_eq!(VersionBar::of(&plain), None, "one version: no capsule");
        assert!(keeps_card_open(&CommandId::ImageMenu));
        assert!(keeps_card_open(&CommandId::ImageResizeCustom));
        assert!(keeps_card_open(&CommandId::ImageStep(ImageOp::Grayscale)));

        let menu = super::image_menu_items(plain.image_edit.as_ref().unwrap());
        let menu_titles: Vec<&str> = menu.iter().map(|(cmd, _)| cmd.title.as_str()).collect();
        assert_eq!(
            menu_titles,
            [
                "50%",
                "25%",
                "Longest side 2048",
                "Longest side 1024",
                "Longest side 512",
                "Custom…",
                "Rotate 90° left",
                "Rotate 90° right",
                "Flip horizontal",
                "Flip vertical",
                "Remove metadata",
                "Grayscale",
            ]
        );
        let grouped: Vec<Option<&str>> = menu.iter().map(|(cmd, _)| menu_group(&cmd.id)).collect();
        assert_eq!(grouped[..6], [Some("Resize"); 6]);
        assert!(grouped[6..].iter().all(Option::is_none));
        assert_eq!(
            menu[3].0.id,
            CommandId::ImageStep(ImageOp::Resize(Resize::Longest(1024)))
        );

        let edited = edited_picture(1, &["Original", "Resized to 1024×768", "Grayscale"]);
        let bar = VersionBar::of(&edited).expect("capsule");
        assert_eq!(bar.title(), "v2/3");
        assert_eq!(
            undo_key("z", true, false, false, Some(&bar)),
            Some(CommandId::TableUndo)
        );
        let menu = super::image_menu_items(edited.image_edit.as_ref().unwrap());
        let tail: Vec<(&str, &str)> = menu[12..]
            .iter()
            .map(|(cmd, _)| (cmd.title.as_str(), cmd.detail.as_str()))
            .collect();
        assert_eq!(
            tail,
            [
                ("Undo image step", "Resized to 1024×768"),
                ("Redo image step", "Grayscale")
            ]
        );
        // The steps are found by search too.
        assert!(
            search_pool(&edited)
                .iter()
                .any(|cmd| cmd.id == CommandId::ImageStep(ImageOp::FlipVertical))
        );
        // Only in the picture view.
        let mut info = edited.clone();
        info.view = CardView::Info;
        assert_eq!(VersionBar::of(&info), None);
        // A picture without versions (the clipboard's, not in history) has no Image ▾.
        let mut bare = edited_picture(0, &["Original"]);
        bare.image_edit = None;
        assert_eq!(titles(&chips(&bare)), vec!["Original", "Info"]);
    }

    #[test]
    fn a_picture_version_shows_its_label_and_stays_the_same_entry() {
        let original = edited_picture(0, &["Original", "Resized to 512×384"]);
        assert_eq!(work_card(&original).meta, "PNG  1024×768  0.18 MB");
        let mut resized = edited_picture(1, &["Original", "Resized to 512×384"]);
        resized.image = Some(ImageFacts {
            format: "PNG".into(),
            width: 512,
            height: 384,
            byte_len: 52_000,
        });
        resized.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
        assert_eq!(
            work_card(&resized).meta,
            "PNG  512×384  0.05 MB  ·  Resized to 512×384"
        );
        // Every version is the same entry: the reveal stays.
        assert_eq!(content_key(&original), content_key(&resized));
        let mut other = resized.clone();
        other.image_edit.as_mut().unwrap().entry = 8;
        assert_ne!(content_key(&other), content_key(&resized));

        let mut working = resized.clone();
        working.image_edit.as_mut().unwrap().working = true;
        assert!(work_card(&working).meta.ends_with("  ·  Working…"));
        let mut noted = original.clone();
        noted.image_edit.as_mut().unwrap().note = Some("Already that size or smaller".into());
        assert_eq!(
            work_card(&noted).meta,
            "PNG  1024×768  0.18 MB  ·  Already that size or smaller"
        );
    }

    #[test]
    fn plain_text_offers_copy_and_save_on_the_content() {
        let input = data(SubjectKind::Text, Some("Emmi Silvennoinen"));
        assert!(chips(&input).is_empty());
        let card = work_card(&input);
        assert_eq!(copy_tip(&card.title), "Copy Content");
        assert_eq!(save_tip(&card.title), "Save Content");
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
    }

    #[test]
    fn markdown_opens_formatted_and_coloured_without_redact() {
        let src = "# Notes\nCall jan@example.com\n- bring the report\n";
        let input = data(SubjectKind::Text, Some(src));
        let shown = chips(&input);
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Format));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Markdown));
        assert_eq!(card.title, "Formatted Markdown");
        assert!(card.excerpt.contains("# Notes\n\n"));
        assert!(card.excerpt.contains("jan@example.com"));
        assert!(card.meta.contains("PII"), "{}", card.meta);
        let save = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
        assert_eq!(save.extension, "md");
        assert_eq!(save.filename, "clipboard.md");
    }

    #[test]
    fn json_opens_formatted_and_coloured() {
        let src = r#"{"name":"copycraft","ok":true}"#;
        let input = data(SubjectKind::Text, Some(src));
        assert!(!chips(&input).iter().any(|cmd| cmd.id == CommandId::Format));
        assert_eq!(presented_view(src, CardView::Original), CardView::Format);
        let card = work_card(&input);
        let pretty = transformed_text(src, CardView::Format).unwrap();
        assert_ne!(pretty, src);
        assert_eq!(card.excerpt, pretty);
        assert!(card.excerpt.contains("\"ok\""));
        assert!(!card.excerpt.ends_with('…'));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
        assert_eq!(card.title, "Valid JSON");
        assert_eq!(copy_tip(&card.title), "Copy Valid JSON");
        assert_eq!(save_tip(&card.title), "Save Valid JSON");
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
    }

    #[test]
    fn ocr_and_qr_bodies_label_secrets_and_redact_scrubs_them() {
        let ocr = "pay NL91ABNA0417164300 key AKIAIOSFODNN7EXAMPLE";
        let mut input = data(SubjectKind::Image, None);
        input.image = Some(ImageFacts {
            format: "PNG".into(),
            width: 32,
            height: 32,
            byte_len: 1_200,
        });
        input.image_scan = Some(ImageScan {
            info: "PNG\nSize: 32×32".into(),
            data_url: None,
            ocr: Some(ocr.into()),
            qr: None,
        });
        let picture = work_card(&input);
        assert!(picture.shows_image);
        assert!(!picture.meta.contains("credential"));
        input.view = CardView::Ocr;
        let card = work_card(&input);
        assert!(card.meta.contains("credential"), "{}", card.meta);
        assert!(card.meta.contains("financial"), "{}", card.meta);
        assert!(card.excerpt.contains("NL91ABNA0417164300"));
        assert!(card.excerpt.contains("AKIAIOSFODNN7EXAMPLE"));

        let mut qr = data(SubjectKind::Image, None);
        qr.image_scan = Some(ImageScan {
            info: "Image".into(),
            data_url: None,
            ocr: None,
            qr: Some("NL91ABNA0417164300".into()),
        });
        qr.view = CardView::Qr;
        let card = work_card(&qr);
        assert!(card.meta.contains("financial"), "{}", card.meta);
        assert!(card.excerpt.contains("NL91ABNA0417164300"));
    }

    #[test]
    fn overflow_empties_only_the_pasteboard() {
        let menu = overflow(&data(SubjectKind::Text, Some("secret")));
        let item = menu
            .iter()
            .find(|cmd| cmd.id == CommandId::ClearClipboard)
            .unwrap();
        assert_eq!(item.title, "Empty pasteboard");
        assert_eq!(item.detail, "Pasteboard only");
        assert!(menu.iter().all(|cmd| cmd.id != CommandId::Clear));
        assert!(keeps_card_open(&CommandId::Clear));
    }

    #[test]
    fn image_scan_adds_the_actions_from_the_old_preview() {
        let mut input = data(SubjectKind::Image, None);
        let url = format!("data:image/png;base64,{}", "a".repeat(80));
        input.image_scan = Some(ImageScan {
            info: "Image\nSize: 10×10".into(),
            data_url: Some(url.clone()),
            ocr: Some("hello".into()),
            qr: Some("https://example.com".into()),
        });
        assert_eq!(
            titles(&chips(&input)),
            vec!["Original", "Info", "Text", "QR"]
        );
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: true
            }
        );
        input.view = CardView::Info;
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        assert_eq!(copy_tip("Image info"), "Copy Image info");
        input.view = CardView::Ocr;
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        input.view = CardView::Qr;
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: true
            }
        );
        input.view = CardView::Info;
        let info = work_card(&input);
        assert_eq!(info.title, "Image info");
        assert!(!info.shows_image);
        assert!(info.selectable);
        assert!(info.excerpt.contains("10×10"));
        assert!(info.excerpt.contains(&url));
    }

    #[test]
    fn youtube_url_is_a_link_card() {
        let url = "https://www.youtube.com/watch?v=bEN9Dyg48b0";
        let card = work_card(&data(SubjectKind::Text, Some(url)));
        assert_eq!(card.title, "YouTube");
        assert_eq!(card.link_page.as_deref(), Some(url));
        assert_eq!(card.excerpt, url);
        assert!(card.selectable);
        assert!(!card.shows_image);
        let input = data(SubjectKind::Text, Some(url));
        assert_eq!(titles(&chips(&input)), vec!["Visit"]);
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: false
            }
        );
    }

    #[test]
    fn html_url_previews_like_a_page() {
        let url = "https://www.example.com/news/story";
        let input = data(SubjectKind::Text, Some(url));
        let card = work_card(&input);
        assert_eq!(card.title, "Link");
        assert_eq!(card.link_page.as_deref(), Some(url));
        assert_eq!(card.excerpt, url);
        assert!(card.selectable);
        // Blurred until revealed, like any copy.
        assert!(masks_content(&card, CardView::Original));
        assert_eq!(titles(&chips(&input)), vec!["Visit"]);
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: true,
                save: false
            }
        );
    }

    #[test]
    fn bare_host_is_a_link_card() {
        let input = data(SubjectKind::Text, Some("grok.com"));
        let card = work_card(&input);
        assert_eq!(card.title, "Link");
        assert_eq!(card.link_page.as_deref(), Some("grok.com"));
        assert_eq!(titles(&chips(&input)), vec!["Visit", "Format"]);
    }

    #[test]
    fn page_card_adds_format_when_the_url_needs_it() {
        let messy = "https://example.com/search?q=a/b";
        assert_eq!(
            titles(&chips(&data(SubjectKind::Text, Some(messy)))),
            vec!["Visit", "Format"]
        );
    }

    #[test]
    fn file_urls_stay_text() {
        let card = work_card(&data(
            SubjectKind::Text,
            Some("https://example.com/report.pdf"),
        ));
        assert_eq!(card.title, "URL");
        assert!(card.link_page.is_none());
        assert_eq!(
            titles(&chips(&data(
                SubjectKind::Text,
                Some("https://example.com/report.pdf")
            ))),
            Vec::<String>::new()
        );
        assert_eq!(
            content_actions(&data(
                SubjectKind::Text,
                Some("https://example.com/report.pdf")
            )),
            ContentActions {
                copy: true,
                save: true
            }
        );
    }

    #[test]
    fn dataframe_view_of_a_phone_csv_stays_up() {
        let src = "\
Id,Naam,Telefoonnummer,Salaris
1,Jan de Vries,06-12345678,3450
2,Anja Bakker,06-87654321,2900";
        let body = transformed_text(src, CardView::Dataframe).expect("dataframe");
        assert!(body.contains('┆'));
        assert!(body.contains("Jan de Vries"));
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        let card = work_card(&input);
        assert_eq!(card.title, "Dataframe");
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Dataframe));
        assert!(card.selectable);
        assert!(card.excerpt.contains("shape"), "{}", card.excerpt);
        assert!(card.excerpt.contains("Jan de Vries"), "{}", card.excerpt);
        assert!(card.excerpt.contains("Anja Bakker"), "{}", card.excerpt);
        assert!(card.excerpt.contains("Telefoonnummer"), "{}", card.excerpt);
        assert!(card.excerpt.contains("Salaris"), "{}", card.excerpt);
        assert!(card.excerpt.contains('┆'), "{}", card.excerpt);
        assert!(card.meta.contains("PII · financial"), "{}", card.meta);
    }

    #[test]
    fn a_table_below_a_definition_line_opens_as_a_table_with_a_header_note() {
        let src = crate::dataframe::tests::ENERGY_FIXTURE;
        let card = work_card(&data(SubjectKind::Text, Some(src)));
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Csv));
        assert!(card.meta.contains("  ·  Header on line 2"), "{}", card.meta);
        // The definition line stays above the aligned table, as copied.
        let first = card.excerpt.lines().next().unwrap_or_default();
        assert!(
            first
                .trim_start_matches('\u{feff}')
                .starts_with("Definition:")
        );
        let second = card.excerpt.lines().nth(1).unwrap_or_default();
        assert!(second.starts_with("Date"), "{:?}", second.get(..12));
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        // 20 columns: Table shows the grid directly (shortened names); Copy keeps the real ones.
        let table = read(src, crate::dataframe::ReadOptions::default());
        input.table = Some(table);
        let grid = work_card(&input);
        assert_eq!(grid.title, "Dataframe");
        assert!(grid.excerpt.contains("shape: (40, 20)"), "{}", grid.excerpt);
        assert!(grid.excerpt.contains("┆ … PV3"), "{}", grid.excerpt);
        assert!(!grid.excerpt.contains("Sunbox"), "{}", grid.excerpt);
        assert!(grid.meta.contains("Header on line 2"), "{}", grid.meta);
        assert!(grid.preview_note.is_none());
        // No Show columns / Show table chip — only Original and Table ▾.
        assert!(ids(&super::chips(&input)).contains(&CommandId::TableMenu));
        let copied = transformed_text(src, CardView::Dataframe).expect("copy");
        assert!(copied.contains("Sunbox 7"), "{copied}");
        assert!(
            ids(&super::chips(&data(SubjectKind::Text, Some(src)))).contains(&CommandId::TableMenu)
        );
    }

    #[test]
    fn the_table_window_shows_the_version_with_its_meta_columns_versions_and_menu() {
        let src =
            "name,iban\nann,NL91ABNA0417164300\nann,NL91ABNA0417164300\nbob,NL20INGB0001234567";
        let table = deduped(src);
        let view = table_window_view(&table, src);
        assert!(view.grid.contains("bob"), "{}", view.grid);
        assert!(view.placeholder.is_empty());
        assert!(view.meta.starts_with("2 rows × 2 columns"), "{}", view.meta);
        assert!(view.meta.contains("Duplicates removed"), "{}", view.meta);
        assert!(view.meta.contains("financial"), "{}", view.meta);
        assert!(!view.checking);
        let names: Vec<&str> = view.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["name", "iban"]);
        assert_eq!(view.version_title(), "v2/2");
        assert!(view.can_undo() && !view.can_redo());
        // The card's own items are not in the window's menu; the steps are.
        let ids: Vec<&CommandId> = view.menu.iter().map(|(c, _)| &c.id).collect();
        for card_only in [
            CommandId::TableChooseColumns,
            CommandId::TableOpenWindow,
            CommandId::TableUndo,
            CommandId::TableRedo,
        ] {
            assert!(!ids.contains(&&card_only), "{card_only:?}");
        }
        assert!(ids.contains(&&CommandId::TableStep(crate::table::TableOp::Dedupe)));
        // The card offers the window; it keeps the card open.
        assert!(
            table_menu(Some(&table))
                .iter()
                .any(|c| c.id == CommandId::TableOpenWindow && c.title == OPEN_WINDOW_TITLE)
        );
        assert!(keeps_card_open(&CommandId::TableOpenWindow));
        // Without a frame yet: a placeholder, no grid.
        let mut working = table.clone();
        working.frame = None;
        let waiting = table_window_view(&working, src);
        assert!(waiting.grid.is_empty() && waiting.placeholder == "Working on the table…");
        let mut wiped = view.clone();
        wiped.wipe();
        assert!(wiped.grid.is_empty() && wiped.meta.is_empty() && wiped.columns.is_empty());
    }

    #[test]
    fn the_table_window_says_when_it_shows_part_of_a_long_table() {
        let mut src = String::from("n,m\n");
        for i in 0..(WINDOW_ROWS + 5) {
            src.push_str(&format!("{i},{}\n", i * 2));
        }
        let table = stepped(
            &src,
            crate::table::TableOp::Sort {
                column: "n".to_string(),
                descending: true,
            },
        );
        let view = table_window_view(&table, &src);
        assert!(
            view.meta.contains(&format!(
                "Showing 1,000 of {}",
                group_thousands(WINDOW_ROWS + 5)
            )),
            "{}",
            view.meta
        );
    }

    /// `src` after one Dedupe step, as the card gets it.
    fn deduped(src: &str) -> TableShown {
        stepped(src, crate::table::TableOp::Dedupe)
    }

    /// `src` after one `op`, as the card gets it.
    fn stepped(src: &str, op: crate::table::TableOp) -> TableShown {
        use crate::table::TableVersions;
        let mut versions = TableVersions::default();
        let job = versions.push(op, src).expect("push");
        let done = job
            .run(&std::sync::atomic::AtomicBool::new(false))
            .expect("job");
        assert!(versions.finish(done));
        TableShown {
            frame: versions.frame().cloned(),
            frame_id: versions.frame_id(),
            version: versions.cursor(),
            labels: versions.labels(),
            working: false,
            error: None,
            options: versions.options(),
            notes: versions.notes(),
            overview: None,
        }
    }

    #[test]
    fn the_dataframe_view_shows_the_table_version() {
        let src = "name,n\na,1\na,1\nb,2";
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        let original = work_card(&input);
        assert!(
            original.excerpt.contains("shape: (3, 2)"),
            "{}",
            original.excerpt
        );
        input.table = Some(deduped(src));
        let card = work_card(&input);
        assert!(card.excerpt.contains("shape: (2, 2)"), "{}", card.excerpt);
        assert!(card.meta.starts_with("3 lines"), "{}", card.meta);
        // The step of the version shown, right after the size.
        assert!(
            card.meta.contains("KB  ·  Duplicates removed"),
            "{}",
            card.meta
        );
        // Another version is the same entry: it stays revealed ([`stays_revealed`]).
        let plain = data(SubjectKind::Text, Some(src));
        let mut versioned = plain.clone();
        versioned.table = input.table.clone();
        assert_eq!(content_key(&plain), content_key(&versioned));
        // The Original view shows the text as copied.
        versioned.view = CardView::Original;
        assert_eq!(work_card(&versioned).excerpt, work_card(&plain).excerpt);
    }

    #[test]
    fn a_table_version_being_worked_out_says_so_and_errors_show_in_the_meta() {
        let src = "name,n\na,1\na,1\nb,2";
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        let mut table = deduped(src);
        table.frame = None;
        table.working = true;
        input.table = Some(table.clone());
        let card = work_card(&input);
        assert!(card.excerpt.is_empty());
        assert_eq!(card.placeholder, "Working on the table…");
        table = deduped(src);
        table.error = Some("20 versions at most".to_string());
        input.table = Some(table);
        assert!(work_card(&input).meta.contains("20 versions at most"));
        // The original with a table attached is the plain Dataframe card.
        let mut original = deduped(src);
        original.version = 0;
        original.labels.truncate(1);
        input.table = None;
        let plain = work_card(&input);
        input.table = Some(original);
        assert_eq!(work_card(&input).excerpt, plain.excerpt);
    }

    #[test]
    fn undo_and_redo_are_in_the_search_not_in_the_table_menu() {
        let mut table = deduped("name,n\na,1\na,1");
        let in_menu = |table: &TableShown| -> Vec<CommandId> {
            table_menu(Some(table)).into_iter().map(|c| c.id).collect()
        };
        assert!(!in_menu(&table).contains(&CommandId::TableUndo));
        table.version = 0;
        assert!(!in_menu(&table).contains(&CommandId::TableRedo));
        assert!(keeps_card_open(&CommandId::TableUndo));
        let mut input = data(SubjectKind::Text, Some("name,n\na,1\na,1"));
        assert!(
            !search_pool(&input)
                .iter()
                .any(|c| c.id == CommandId::TableRedo)
        );
        input.table = Some(table);
        assert!(
            search_pool(&input)
                .iter()
                .any(|c| c.id == CommandId::TableRedo)
        );
    }

    #[test]
    fn a_table_gets_the_table_menu_chip_and_its_steps_in_search() {
        let input = data(SubjectKind::Text, Some("name,n\na,1\na,1"));
        let titles: Vec<String> = chips(&input).into_iter().map(|c| c.title).collect();
        assert!(titles.iter().any(|t| t == TABLE_MENU_TITLE), "{titles:?}");
        assert!(
            search_pool(&input)
                .iter()
                .any(|c| c.id == CommandId::TableStep(crate::table::TableOp::Dedupe))
        );
        assert!(keeps_card_open(&CommandId::TableMenu));
        let prose = data(SubjectKind::Text, Some("just some words"));
        assert!(!chips(&prose).iter().any(|c| c.id == CommandId::TableMenu));
        // Before the table is read the menu has the one-click steps.
        let menu = table_menu(None);
        assert_eq!(menu.len(), crate::table::TableOp::ONE_CLICK.len());
        // The Dataframe view has no chip: Table ▾ opens it.
        assert!(!chips(&input).iter().any(|c| c.id == CommandId::Dataframe));
    }

    #[test]
    fn the_table_menu_has_six_items_and_sorts_by_each_column_in_submenus() {
        use crate::table::TableOp;
        let table = deduped("name,n\na,1\na,1");
        let menu = table_menu(Some(&table));
        // The top level, in order; the sorts are submenus.
        let mut top: Vec<String> = Vec::new();
        for c in &menu {
            let item = menu_group(&c.id).map_or(c.title.clone(), |group| format!("{group} ›"));
            if !top.contains(&item) {
                top.push(item);
            }
        }
        assert_eq!(
            top,
            [
                "Remove duplicate rows",
                "Remove empty rows and columns",
                "Choose columns…",
                "Sort ascending ›",
                "Sort descending ›",
                "Open in window"
            ]
        );
        let sort_up: Vec<&str> = menu
            .iter()
            .filter(|c| menu_group(&c.id) == Some("Sort ascending"))
            .map(|c| c.title.as_str())
            .collect();
        assert_eq!(sort_up, ["name", "n"]);
        assert_eq!(
            TableOp::Sort {
                column: "Price".into(),
                descending: true
            }
            .label(),
            "Sorted by Price ↓"
        );
        assert_eq!(menu_group(&CommandId::TableUndo), None);
        // Without a frame there are no columns to offer yet.
        let mut loading = table.clone();
        loading.frame = None;
        assert!(
            table_menu(Some(&loading))
                .iter()
                .all(|c| menu_group(&c.id).is_none())
        );
    }

    /// `src` read as `options`, as the card gets it.
    fn read(src: &str, options: crate::dataframe::ReadOptions) -> TableShown {
        use crate::table::TableVersions;
        let mut versions = TableVersions::default();
        let job = match versions.reread(options, src) {
            Some(job) => job,
            None => versions.load(src).expect("load"),
        };
        let done = job
            .run(&std::sync::atomic::AtomicBool::new(false))
            .expect("job");
        assert!(versions.finish(done));
        TableShown {
            frame: versions.frame().cloned(),
            frame_id: versions.frame_id(),
            version: versions.cursor(),
            labels: versions.labels(),
            working: false,
            error: None,
            options: versions.options(),
            notes: versions.notes(),
            overview: None,
        }
    }

    #[test]
    fn dates_in_either_order_ask_with_a_chip() {
        use crate::dataframe::ReadOptions;
        let src = "Export\nwhen,n\n01/02/2026,1\n03/04/2026,2";
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        input.table = Some(read(src, ReadOptions::default()));
        let question = chips(&input)
            .into_iter()
            .find(|c| c.id == CommandId::TableDateOrder(true))
            .expect("question");
        assert_eq!(question.title, "Dates are mm/dd/yyyy");
        // The Original view does not ask.
        input.view = CardView::Original;
        assert!(
            !chips(&input)
                .iter()
                .any(|c| matches!(c.id, CommandId::TableDateOrder(_)))
        );
        // Read month first: the card renders the frame and says so; the chip asks back.
        let month_first = ReadOptions { month_first: true };
        input.view = CardView::Dataframe;
        let plain_key = content_key(&input);
        input.table = Some(read(src, month_first));
        let card = work_card(&input);
        assert!(card.meta.contains("Header on line 2"), "{}", card.meta);
        assert!(
            card.meta.contains("Dates read as mm/dd/yyyy"),
            "{}",
            card.meta
        );
        assert!(card.excerpt.contains("2026-01-02"), "{}", card.excerpt);
        assert!(!card.meta.contains("Original"), "{}", card.meta);
        assert_eq!(content_key(&input), plain_key);
        assert!(
            chips(&input)
                .iter()
                .any(|c| c.id == CommandId::TableDateOrder(false))
        );
    }

    #[test]
    fn the_version_bar_shows_in_the_dataframe_view_with_two_versions_or_more() {
        let src = "name,n\na,1\na,1";
        let mut input = data(SubjectKind::Text, Some(src));
        input.table = Some(deduped(src));
        assert_eq!(VersionBar::of(&input), None);
        input.view = CardView::Dataframe;
        let bar = VersionBar::of(&input).expect("bar");
        assert_eq!(bar.title(), "v2/2");
        assert_eq!(bar.spoken(), "Version 2 of 2, Duplicates removed");
        assert!(bar.can_undo() && !bar.can_redo());
        let menu = bar.menu();
        assert_eq!(menu.len(), 2);
        assert_eq!(menu[0].0.title, "1. Original");
        assert_eq!(menu[0].0.id, CommandId::TableVersion(0));
        assert!(!menu[0].1 && menu[1].1);
        assert!(keeps_card_open(&CommandId::TableVersion(0)));
        // The original alone has no bar.
        let mut original = deduped(src);
        original.labels.truncate(1);
        original.version = 0;
        input.table = Some(original);
        assert_eq!(VersionBar::of(&input), None);
    }

    #[test]
    fn choose_columns_lists_the_version_shown_and_applies_as_one_step() {
        use crate::column_picker::ColumnPicker;
        let src = crate::dataframe::tests::ENERGY_FIXTURE;
        let table = read(src, crate::dataframe::ReadOptions::default());
        let items = table_menu(Some(&table));
        let choose = items
            .iter()
            .find(|c| c.id == CommandId::TableChooseColumns)
            .expect("Choose columns…");
        assert_eq!(choose.title, "Choose columns…");
        assert_eq!(menu_group(&choose.id), None);
        assert!(keeps_card_open(&CommandId::TableChooseColumns));
        let columns = picker_columns(&table).expect("columns");
        assert_eq!(columns.len(), 20);
        assert_eq!(columns[0].kind, "date");
        let pv1 = &columns[14];
        assert_eq!(pv1.name, "Sunbox 7 X1500 Max - PV1 Generation (kWh)");
        assert_eq!(pv1.shown, "… PV1 Generation (kWh)");
        assert_eq!(pv1.kind, "number");
        // Untick 12: one step, the 8 kept in the table's order.
        let mut picker = ColumnPicker::new(columns);
        for index in (1..20).step_by(2).chain([2, 4]) {
            picker.set_kept(index, false);
        }
        assert_eq!(picker.count_line(), "Keeping 8 of 20");
        let step = picker.step().expect("step");
        assert_eq!(step.label(), "Kept 8 of 20 columns");
        let frame = table.frame.as_ref().expect("frame");
        let kept = step.apply(frame).expect("apply").expect("changed");
        let names: Vec<String> = kept
            .get_column_names()
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(names.len(), 8);
        assert_eq!(names[0], "Date");
        let order: Vec<usize> = names
            .iter()
            .map(|name| frame.get_column_index(name).expect("column"))
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        // A one-column table has nothing to choose.
        let one = stepped(
            src,
            crate::table::TableOp::SelectColumns {
                columns: vec!["Date".into()],
                kept_of: None,
            },
        );
        assert!(
            !table_menu(Some(&one))
                .iter()
                .any(|c| c.id == CommandId::TableChooseColumns)
        );
    }

    #[test]
    fn table_chip_row_is_original_and_table_only_no_show_columns() {
        use crate::table::TableOp;
        let src = crate::dataframe::tests::ENERGY_FIXTURE;
        let names: Vec<String> = crate::dataframe::parse_table(src)
            .expect("table")
            .get_column_names()
            .iter()
            .take(6)
            .map(|name| name.to_string())
            .collect();
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = CardView::Dataframe;
        let mut table = stepped(
            src,
            TableOp::SelectColumns {
                columns: names,
                kept_of: None,
            },
        );
        // Even if overview were requested, display is the grid.
        table.overview = Some(true);
        input.table = Some(table.clone());
        let card = work_card(&input);
        assert!(card.excerpt.contains("shape: (40, 6)"), "{}", card.excerpt);
        assert!(ids(&chips(&input)).contains(&CommandId::TableMenu));
        table.overview = Some(false);
        input.table = Some(table);
        let card = work_card(&input);
        assert!(card.excerpt.contains("shape: (40, 6)"), "{}", card.excerpt);
        let narrow = "name,n\na,1\nb,2";
        let mut input = data(SubjectKind::Text, Some(narrow));
        input.view = CardView::Dataframe;
        let table = read(narrow, crate::dataframe::ReadOptions::default());
        input.table = Some(table);
        assert!(work_card(&input).excerpt.contains("shape: (2, 2)"));
        assert!(ids(&chips(&input)).contains(&CommandId::TableMenu));
    }

    /// The sensitivity labels a card's meta line shows.
    fn meta_labels(meta: &str) -> Vec<crate::sensitivity::Label> {
        crate::sensitivity::warning_marks(meta)
            .into_iter()
            .map(|mark| mark.label)
            .collect()
    }

    /// Every way one entry is shown again: each view and chip, the table while its job runs and
    /// when it is done, steps, versions, the picker's step.
    fn shown_again(src: &str) -> Vec<(String, LaunchData)> {
        use crate::table::TableOp;
        let plain = data(SubjectKind::Text, Some(src));
        let mut out = Vec::new();
        for view in [
            CardView::Original,
            CardView::Format,
            CardView::Convert,
            CardView::Dataframe,
            CardView::Schema,
            CardView::Sample,
            CardView::Info,
        ] {
            let mut shown = plain.clone();
            shown.view = view;
            out.push((format!("{view:?}"), shown));
        }
        let mut table = plain.clone();
        table.view = CardView::Dataframe;
        let read = read(src, crate::dataframe::ReadOptions::default());
        let mut working = read.clone();
        working.frame = None;
        working.working = true;
        table.table = Some(working);
        out.push(("table job running".into(), table.clone()));
        table.table = Some(read.clone());
        out.push(("table job done".into(), table.clone()));
        let mut full = table.clone();
        full.full = true;
        out.push(("Show all".into(), full));
        for overview in [true, false] {
            let mut toggled = read.clone();
            toggled.overview = Some(overview);
            table.table = Some(toggled);
            out.push((format!("overview {overview}"), table.clone()));
        }
        let first = read
            .frame
            .as_ref()
            .and_then(|frame| {
                frame
                    .get_column_names()
                    .first()
                    .map(|name| name.to_string())
            })
            .expect("a column");
        for op in [
            TableOp::Dedupe,
            TableOp::Sort {
                column: first.clone(),
                descending: true,
            },
            TableOp::SelectColumns {
                columns: vec![first],
                kept_of: Some(2),
            },
        ] {
            table.table = Some(stepped(src, op.clone()));
            out.push((format!("step {op:?}"), table.clone()));
        }
        out
    }

    #[test]
    fn a_revealed_entry_stays_revealed_in_every_view_version_and_step() {
        use crate::sensitivity::Label;
        let energy = crate::dataframe::tests::ENERGY_FIXTURE;
        let pii = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
        for src in [energy, pii] {
            let key = content_key(&data(SubjectKind::Text, Some(src)));
            for (what, shown) in shown_again(src) {
                assert_eq!(content_key(&shown), key, "{what}");
                assert!(stays_revealed(true, key, content_key(&shown)), "{what}");
                // The card is built whatever it shows.
                let _ = work_card(&shown);
            }
        }
        // Views of one entry show different labels; that does not mask it again.
        let mut original = data(SubjectKind::Text, Some(pii));
        let labels = meta_labels(&work_card(&original).meta);
        assert_eq!(labels, [Label::Pii, Label::Financial]);
        original.view = CardView::Dataframe;
        original.table = Some(stepped(
            pii,
            crate::table::TableOp::SelectColumns {
                columns: vec!["Id".into(), "Naam".into()],
                kept_of: Some(3),
            },
        ));
        let card = work_card(&original);
        assert!(
            !meta_labels(&card.meta).contains(&Label::Financial),
            "{}",
            card.meta
        );
        let key = content_key(&original);
        assert!(stays_revealed(true, key, key));
        // Not revealed stays not revealed.
        assert!(!stays_revealed(false, key, key));
    }

    #[test]
    fn a_revealed_entry_stays_revealed_when_its_labels_arrive() {
        // Long enough for the background checker: "Checking…" first, then the labels.
        let mut src = String::from("name,email\n");
        while src.len() <= crate::sensitivity::SYNC_LIMIT {
            src.push_str("Jan,jan@example.com\n");
        }
        let shown = data(SubjectKind::Text, Some(&src));
        let key = content_key(&shown);
        let checking = work_card(&shown);
        // Neither the card's labels nor their status are part of the entry's identity: the
        // card rebuilt when they arrive is the same entry, and stays revealed.
        let _ = crate::sensitivity::labeling(&src);
        let arrived = work_card(&shown);
        assert_eq!(content_key(&shown), key);
        assert!(stays_revealed(true, key, content_key(&shown)));
        assert!(!checking.excerpt.is_empty() && !arrived.excerpt.is_empty());
    }

    #[test]
    fn another_entry_is_masked_again() {
        let first = data(SubjectKind::Text, Some("Id;Naam\n1;Piet"));
        let second = data(SubjectKind::Text, Some("Id;Naam\n1;Jan"));
        assert!(!stays_revealed(
            true,
            content_key(&first),
            content_key(&second)
        ));
        let mut opened = first.clone();
        opened.source_name = Some("piet.csv".into());
        assert!(!stays_revealed(
            true,
            content_key(&first),
            content_key(&opened)
        ));
    }

    #[test]
    fn command_z_undoes_and_shift_command_z_redoes_a_table_step() {
        let bar = VersionBar {
            labels: vec!["Original".into(), "Duplicates removed".into()],
            current: 1,
        };
        assert_eq!(
            undo_key("z", true, false, false, Some(&bar)),
            Some(CommandId::TableUndo)
        );
        // Nothing to redo at the newest version; the key is left alone.
        assert_eq!(undo_key("Z", true, true, false, Some(&bar)), None);
        let back = VersionBar { current: 0, ..bar };
        assert_eq!(
            undo_key("Z", true, true, false, Some(&back)),
            Some(CommandId::TableRedo)
        );
        assert_eq!(undo_key("z", true, false, false, Some(&back)), None);
        assert_eq!(undo_key("z", false, false, false, Some(&back)), None);
        assert_eq!(undo_key("z", true, true, true, Some(&back)), None);
        assert_eq!(undo_key("x", true, false, false, Some(&back)), None);
        assert_eq!(undo_key("z", true, false, false, None), None);
    }

    #[test]
    fn dataframe_meta_says_when_dates_could_be_either_order() {
        let mut input = data(
            SubjectKind::Text,
            Some("when,amount\n01/02/2024,1\n03/04/2024,2"),
        );
        input.view = CardView::Dataframe;
        let card = work_card(&input);
        assert!(
            card.meta.contains("  ·  Dates read as dd/mm/yyyy"),
            "{}",
            card.meta
        );
        input.full = true;
        assert!(work_card(&input).meta.contains("Dates read as dd/mm/yyyy"));
    }

    #[test]
    fn a_long_table_below_a_title_counts_rows_from_the_header() {
        let mut src = String::from("Title of the export\nname,age\n");
        for i in 0..(PREVIEW_ROWS + 50) {
            src.push_str(&format!("p{i},{i}\n"));
        }
        let card = work_card(&data(SubjectKind::Text, Some(&src)));
        assert_eq!(
            card.preview_note.as_deref(),
            Some(format!("Showing 199 of {} rows", PREVIEW_ROWS + 50).as_str())
        );
    }

    #[test]
    fn salary_table_puts_the_labels_on_the_size_line() {
        let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
        let card = work_card(&data(SubjectKind::Text, Some(src)));
        assert!(card.meta.contains("lines"));
        assert!(card.meta.contains("KB"));
        assert!(card.meta.contains("PII · financial"));
        assert!(!card.meta.contains("Jan de Vries"));
        let mut grid = data(SubjectKind::Text, Some(src));
        grid.view = CardView::Dataframe;
        let card = work_card(&grid);
        assert_eq!(card.title, "Dataframe");
        assert!(card.meta.contains("PII · financial"), "{}", card.meta);
    }

    #[test]
    fn choose_file_leads_the_menu_and_a_file_can_return_to_the_clipboard() {
        let clipboard = data(SubjectKind::Text, Some("hello"));
        let menu = overflow(&clipboard);
        assert_eq!(menu[0].id, CommandId::ChooseFile);
        assert_eq!(menu[0].title, "Choose file");
        assert!(menu.iter().all(|cmd| cmd.id != CommandId::UseClipboard));
        assert!(keeps_card_open(&CommandId::ChooseFile));
        assert!(keeps_card_open(&CommandId::UseClipboard));

        let mut file = clipboard;
        file.source_name = Some("notes.txt".to_string());
        let menu = overflow(&file);
        assert_eq!(menu[0].id, CommandId::ChooseFile);
        assert_eq!(menu[1].id, CommandId::UseClipboard);
        assert_eq!(menu[1].title, "Clipboard");
        assert_ne!(
            content_key(&file),
            content_key(&data(SubjectKind::Text, Some("hello")))
        );
    }

    #[test]
    fn chosen_file_keeps_the_kind_title_and_names_the_meta_line() {
        let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
        let mut input = data(SubjectKind::Text, Some(src));
        input.source_name = Some("people.tsv".to_string());
        let card = work_card(&input);
        assert_eq!(card.title, "TSV");
        assert!(card.meta.starts_with("people.tsv"));
        assert!(card.meta.contains("PII · financial"));
        assert!(card.excerpt.contains("Jan de Vries"));
    }

    #[test]
    fn dropped_picture_is_an_image_card_with_its_name() {
        let mut input = data(SubjectKind::Image, None);
        input.source_name = Some("photo.heic".to_string());
        input.image = Some(ImageFacts {
            format: "HEIC".into(),
            width: 4032,
            height: 3024,
            byte_len: 2_000_000,
        });
        input.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
        let card = work_card(&input);
        assert_eq!(card.title, "Image");
        assert_eq!(card.meta, "photo.heic  ·  HEIC  4032×3024  2.00 MB");
        assert!(card.shows_image);
        assert!(masks_content(&card, CardView::Original));
        assert_eq!(titles(&chips(&input)), vec!["Original", "Info"]);
        assert!(
            overflow(&input)
                .iter()
                .any(|cmd| cmd.id == CommandId::UseClipboard)
        );

        // Another picture alike in name, format and size is another item: not revealed yet.
        let mut other = data(SubjectKind::Image, None);
        other.source_name = input.source_name.clone();
        other.image = input.image.clone();
        other.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
        assert_ne!(content_key(&input), content_key(&other));
        other.picture = input.picture.clone();
        assert_eq!(content_key(&input), content_key(&other));
    }

    #[test]
    fn unreadable_file_uses_its_name_and_a_note() {
        let mut input = data(SubjectKind::NoText, None);
        input.source_name = Some("photo.png".to_string());
        input.source_note = Some("This file is not text".to_string());
        let card = work_card(&input);
        assert_eq!(card.title, "photo.png");
        assert_eq!(card.placeholder, "This file is not text");
        assert!(card.excerpt.is_empty());
        assert!(card.meta.is_empty());
        assert!(!card.shows_image);
    }

    #[test]
    fn pasted_size_uses_kilobytes_when_small() {
        assert_eq!(super::format_bytes(0), "0 KB");
        assert_eq!(super::format_bytes(12), "0.012 KB");
        assert_eq!(super::format_bytes(1_500), "1.5 KB");
        assert_eq!(super::format_bytes(9_000), "9.0 KB");
        assert_eq!(super::format_bytes(184_000), "0.18 MB");
        assert_eq!(super::format_bytes(2_000_000), "2.00 MB");
        assert_eq!(super::format_bytes(15_500_000), "15.5 MB");
        assert_eq!(super::format_bytes(250_000_000), "250 MB");
        let card = work_card(&data(SubjectKind::Text, Some("hello")));
        assert_eq!(card.meta, "0.005 KB");
    }

    #[test]
    fn text_card_shows_the_payload_not_a_label() {
        let card = work_card(&data(SubjectKind::Text, Some("{\n  \"a\": 1\n}")));
        assert_eq!(card.title, "Valid JSON");
        assert_eq!(card.excerpt, "{\n  \"a\": 1\n}");
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
        assert!(card.meta.contains("lines"));
        assert!(card.meta.contains("KB"));
        assert!(!card.excerpt.contains("Preview"));
        assert!(!card.shows_image);
    }

    #[test]
    fn a_clipboard_without_text_points_to_dropping_a_file() {
        let card = work_card(&data(SubjectKind::NoText, None));
        assert_eq!(
            card.placeholder,
            "No text on the clipboard\nDrop a text file or image here to open it"
        );
        assert!(card.excerpt.is_empty());
    }

    #[test]
    fn a_copy_not_read_yet_says_so_and_the_first_one_explains_the_alert() {
        let pending = data(SubjectKind::Pending, None);
        let card = work_card(&pending);
        assert!(
            card.placeholder
                .starts_with("New copy — open the card to view\n")
        );
        assert!(card.excerpt.is_empty() && card.meta.is_empty());
        assert!(chips(&pending).is_empty());
        assert!(!content_actions(&pending).copy);
        let ask = data(SubjectKind::PasteAsk, None);
        let card = work_card(&ask);
        assert!(
            card.placeholder.contains("Choose Allow"),
            "{}",
            card.placeholder
        );
        assert!(card.placeholder.contains("Paste from Other Apps"));
        assert!(chips(&ask).is_empty());
    }

    #[test]
    fn always_deny_says_so_instead_of_nothing_copied() {
        let input = data(SubjectKind::Denied, None);
        let card = work_card(&input);
        assert!(
            card.placeholder
                .starts_with("Clipboard access denied in Privacy & Security\n"),
            "{}",
            card.placeholder
        );
        assert!(!card.placeholder.contains("Nothing copied"));
        assert!(card.excerpt.is_empty() && card.meta.is_empty());
        assert!(chips(&input).is_empty());
        assert!(!content_actions(&input).copy);
    }

    #[test]
    fn empty_clipboard_has_no_chips_and_keeps_quit_in_the_menu() {
        let input = data(SubjectKind::Empty, None);
        assert!(chips(&input).is_empty());
        let card = work_card(&input);
        assert_eq!(
            card.placeholder,
            "Nothing copied\nDrop a text file or image here to open it"
        );
        assert!(card.excerpt.is_empty());
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: false
            }
        );
        let quiet = data(SubjectKind::NoText, None);
        assert_eq!(
            content_actions(&quiet),
            ContentActions {
                copy: false,
                save: false
            }
        );
        let menu = overflow(&input);
        assert_eq!(menu.last().unwrap().id, CommandId::Quit);
        assert!(menu.iter().all(|cmd| cmd.id != CommandId::Copy));
        assert!(menu.iter().all(|cmd| cmd.id != CommandId::Save));
        assert!(matching(&search_pool(&input), "quit").is_empty());
    }

    #[test]
    fn query_finds_appearance_and_history_but_not_quit() {
        let mut input = data(SubjectKind::Text, Some("hello world"));
        input.history.push(Hist {
            index: 3,
            title: "notes from yesterday".into(),
            mark: "¶".into(),
        });
        let pool = search_pool(&input);
        assert!(matching(&pool, "quit").is_empty());
        assert_eq!(
            ids(&matching(&pool, "dark")),
            vec![CommandId::Appearance(Theme::Dark)]
        );
        assert!(
            chips(&input)
                .iter()
                .all(|cmd| !matches!(cmd.id, CommandId::Appearance(_) | CommandId::History(_)))
        );
        assert_eq!(
            ids(&matching(&pool, "yesterday")),
            vec![CommandId::History(3)]
        );
    }

    #[test]
    fn default_chips_leave_history_for_search() {
        let mut input = data(SubjectKind::Text, Some("hello world"));
        for index in 0..10 {
            input.history.push(Hist {
                index,
                title: format!("older {index}"),
                mark: "¶".into(),
            });
        }
        input.can_clear_history = true;
        let shown = chips(&input);
        assert!(shown.iter().all(|cmd| !housekeeping(&cmd.id)));
        assert!(
            shown
                .iter()
                .all(|cmd| !matches!(cmd.id, CommandId::History(_)))
        );
        assert_eq!(
            ids(&matching(&search_pool(&input), "older 9")),
            vec![CommandId::History(9)]
        );
        assert!(
            overflow(&input)
                .iter()
                .any(|cmd| cmd.id == CommandId::ClearHistory)
        );
        assert!(
            overflow(&input)
                .iter()
                .any(|cmd| cmd.id == CommandId::History(9))
        );
    }

    #[test]
    fn well_masks_copied_content_until_it_changes() {
        let secret = data(SubjectKind::Text, Some("correct horse battery staple"));
        let card = work_card(&secret);
        assert!(masks_content(&card, CardView::Original));
        assert!(!card.excerpt.contains("Click to reveal"));
        let empty = work_card(&data(SubjectKind::Empty, None));
        assert!(!masks_content(&empty, CardView::Original));
        assert_eq!(
            content_key(&secret),
            content_key(&data(
                SubjectKind::Text,
                Some("correct horse battery staple")
            ))
        );
        let mut formatted = secret.clone();
        formatted.view = CardView::Format;
        assert_eq!(content_key(&secret), content_key(&formatted));
        assert_ne!(
            content_key(&secret),
            content_key(&data(SubjectKind::Text, Some("another copy")))
        );
        let mut picture = data(SubjectKind::Image, None);
        picture.image = Some(ImageFacts {
            format: "PNG".into(),
            width: 1280,
            height: 720,
            byte_len: 184_000,
        });
        assert!(masks_content(&work_card(&picture), CardView::Original));
        let mut info = picture.clone();
        info.view = CardView::Info;
        assert!(!masks_content(&work_card(&info), CardView::Info));
        assert_eq!(content_key(&picture), content_key(&info));
        let mut other = picture.clone();
        if let Some(image) = other.image.as_mut() {
            image.byte_len = 200_000;
        }
        assert_ne!(content_key(&picture), content_key(&other));
    }

    #[test]
    fn history_label_is_the_mark_and_size() {
        let secret = "correct horse battery staple";
        let label = history_label("Aa", secret.len());
        assert_eq!(label, "Aa  0.028 KB");
        assert!(!label.contains(secret));
        assert!(!label.contains("horse"));
        assert_eq!(history_label("{}", 1_500), "{}  1.5 KB");
        assert_eq!(history_label("img", 184_000), "img  0.18 MB");
    }

    #[test]
    fn prose_card_shows_the_whole_document() {
        let src = "\
# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.

## Commands

Kleinste opdracht die de change dekt.
";
        let card = work_card(&data(SubjectKind::Text, Some(src)));
        assert_eq!(card.excerpt, src);
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Markdown));
        assert_eq!(card.title, "Markdown");
        assert!(
            chips(&data(SubjectKind::Text, Some(src)))
                .iter()
                .all(|cmd| cmd.id != CommandId::Format)
        );
        assert!(
            card.excerpt
                .contains("Kleinste opdracht die de change dekt.")
        );
        assert!(card.excerpt.contains("thiserror"));
        assert!(!card.excerpt.contains('…'));
    }

    #[test]
    fn excerpt_keeps_lines_and_cuts_a_long_one() {
        assert_eq!(payload_excerpt("alpha\nbeta"), "alpha\nbeta");
        let long = "x".repeat(80);
        let cut = payload_excerpt(&long);
        assert!(cut.ends_with('…'));
        let many = (0..12)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let clipped = payload_excerpt(&many);
        assert_eq!(clipped.lines().count(), 6);
        assert!(clipped.ends_with('…'));
        assert!(cut.chars().count() <= 49);
    }

    #[test]
    fn chip_width_follows_the_measured_pill() {
        assert_eq!(chip_width(71.2), 72.0);
        assert_eq!(chip_width(120.0), 120.0);
        assert_eq!(chip_width(30.0), super::CHIP_MIN_W);
        assert_eq!(chip_width(0.0), super::CHIP_MIN_W);
        assert_eq!(chip_width(f64::NAN), super::CHIP_MIN_W);
        assert_eq!(chip_width(-5.0), super::CHIP_MIN_W);
        assert_eq!(chip_width(900.0), super::CHIP_MAX_W);
        assert_eq!(chip_width(f64::INFINITY), super::CHIP_MAX_W);
    }

    #[test]
    fn chips_keep_their_measured_widths() {
        let widths = [chip_width(61.5), chip_width(88.0), chip_width(40.0)];
        let frames = layout_chips(&widths, 412.0, 0.0);
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|frame| frame.row == 0));
        assert_eq!(frames[0].x, 0.0);
        assert_eq!(frames[0].width, 62.0);
        assert_eq!(frames[1].x, 62.0 + super::CHIP_GAP);
        assert_eq!(frames[1].width, 88.0);
        assert_eq!(frames[2].x, 62.0 + 88.0 + 2.0 * super::CHIP_GAP);
        assert_eq!(frames[2].width, super::CHIP_MIN_W);
    }

    #[test]
    fn a_chip_wider_than_the_row_gets_its_own_row() {
        let frames = layout_chips(&[80.0, 220.0, 80.0], 200.0, 0.0);
        assert_eq!(frames[0].row, 0);
        assert_eq!((frames[1].row, frames[1].x), (1, 0.0));
        assert_eq!((frames[2].row, frames[2].x), (2, 0.0));
        assert!(layout_chips(&[], 200.0, 0.0).is_empty());
    }

    #[test]
    fn chips_wrap_and_arrows_move_between_pills() {
        let widths = [78.0, 72.0, 86.0, 56.0, 74.0, 80.0];
        let frames = layout_chips(&widths, 220.0, 0.0);
        assert!(frames.len() == widths.len());
        assert_eq!(frames[0].row, 0);
        assert!(frames.last().unwrap().row > 0);
        assert_eq!(step_chip(&frames, 0, 1, 0), 1);
        assert_eq!(step_chip(&frames, 0, -1, 0), 0);
        let down = step_chip(&frames, 0, 0, 1);
        assert_ne!(frames[down].row, frames[0].row);
        assert_eq!(step_chip(&frames, down, 0, -1), 0);
        assert_eq!(step_chip(&[], 0, 1, 0), 0);
        // No row that way: ↑ / ↓ go to the previous / next chip, and stop at the ends.
        let last = frames.len() - 1;
        assert_eq!(step_chip(&frames, last, 0, 1), last);
        assert_eq!(step_chip(&frames, 0, 0, -1), 0);
        let one_row = layout_chips(&[60.0, 60.0, 60.0], 400.0, 0.0);
        assert!(one_row.iter().all(|frame| frame.row == 0));
        assert_eq!(step_chip(&one_row, 0, 0, 1), 1);
        assert_eq!(step_chip(&one_row, 1, 0, 1), 2);
        assert_eq!(step_chip(&one_row, 2, 0, 1), 2);
        assert_eq!(step_chip(&one_row, 2, 0, -1), 1);
    }

    #[test]
    fn left_and_right_step_history_like_the_chevrons() {
        use ArrowAction::{Chip, History, Text};
        use ArrowFocus::{Card, TextWithContent};
        // ← is ‹ Previous (toward 1, the newest), → is › Next (toward the oldest).
        assert_eq!(
            arrow_action(Key::Left, false, Card, true),
            Some(History(PREVIOUS_CHEVRON))
        );
        assert_eq!(
            arrow_action(Key::Right, false, Card, true),
            Some(History(NEXT_CHEVRON))
        );
        assert_eq!(PREVIOUS_CHEVRON.command(), CommandId::HistoryNewer);
        assert_eq!(NEXT_CHEVRON.command(), CommandId::HistoryOlder);
        // Text in the search or find field: plain ← → move the caret.
        assert_eq!(
            arrow_action(Key::Left, false, TextWithContent, true),
            Some(Text)
        );
        assert_eq!(
            arrow_action(Key::Right, false, TextWithContent, false),
            Some(Text)
        );
        // ⌘← / ⌘→ always step history.
        for focus in [Card, TextWithContent] {
            for has_history in [true, false] {
                assert_eq!(
                    arrow_action(Key::Left, true, focus, has_history),
                    Some(History(PREVIOUS_CHEVRON))
                );
                assert_eq!(
                    arrow_action(Key::Right, true, focus, has_history),
                    Some(History(NEXT_CHEVRON))
                );
            }
        }
        // No history capsule: ← → move between the chips, as before.
        assert_eq!(
            arrow_action(Key::Left, false, Card, false),
            Some(Chip { dx: -1, dy: 0 })
        );
        assert_eq!(
            arrow_action(Key::Right, false, Card, false),
            Some(Chip { dx: 1, dy: 0 })
        );
        // ↑ ↓ move between the chips; other keys are not arrows.
        for (key, dy) in [(Key::Up, -1), (Key::Down, 1)] {
            for focus in [Card, TextWithContent] {
                assert_eq!(
                    arrow_action(key, false, focus, true),
                    Some(Chip { dx: 0, dy })
                );
            }
        }
        assert_eq!(arrow_action(Key::Return, false, Card, true), None);
        assert_eq!(arrow_action(Key::Escape, true, Card, true), None);
    }

    #[test]
    fn chevrons_name_their_keys() {
        assert_eq!(PREVIOUS_CHEVRON.tooltip(), "Previous (← or ⌘←)");
        assert_eq!(NEXT_CHEVRON.tooltip(), "Next (→ or ⌘→)");
        assert_eq!(
            PREVIOUS_CHEVRON.spoken_shortcut(),
            "Left Arrow, or Command-Left Arrow"
        );
        assert_eq!(
            NEXT_CHEVRON.spoken_shortcut(),
            "Right Arrow, or Command-Right Arrow"
        );
    }

    #[test]
    fn history_nav_stays_on_the_card() {
        assert_eq!(history_nav(0, 0), None, "no history: no capsule");
        assert_eq!(history_nav(1, 0), None, "one copy: nowhere to go");
        let newest = history_nav(3, 0).unwrap();
        assert!(newest.can_older);
        assert!(!newest.can_newer);
        let middle = history_nav(3, 1).unwrap();
        assert!(middle.can_older && middle.can_newer);
        let oldest = history_nav(3, 2).unwrap();
        assert!(!oldest.can_older);
        assert!(oldest.can_newer);
        assert_eq!(newest.total, 3);
        assert_eq!(middle.total, 3);
        assert_eq!(step_history(3, 0, true), Some(1));
        assert_eq!(step_history(3, 0, false), None);
        assert_eq!(step_history(3, 2, true), None);
        assert_eq!(step_history(3, 2, false), Some(1));
        assert_eq!(step_history(0, 0, true), None);
        assert_eq!(step_history(1, 0, true), None);
        assert_eq!(step_history(1, 0, false), None);
    }

    #[test]
    fn history_position_counts_from_the_newest() {
        let newest = history_nav(2, 0).unwrap();
        assert_eq!(newest.position, 1);
        assert_eq!(newest.label(), "1 / 2");
        assert_eq!(newest.spoken(), "Item 1 of 2");
        let oldest = history_nav(2, 1).unwrap();
        assert_eq!(oldest.label(), "2 / 2");
        assert_eq!(oldest.spoken(), "Item 2 of 2");
        // › (next) counts up, ‹ (previous) counts down.
        let next = step_history(5, 0, NEXT_CHEVRON.older).unwrap();
        assert_eq!(history_nav(5, next).unwrap().label(), "2 / 5");
        let previous = step_history(5, next, PREVIOUS_CHEVRON.older).unwrap();
        assert_eq!(history_nav(5, previous).unwrap().label(), "1 / 5");
        // A cursor past the end shows the oldest copy.
        assert_eq!(history_nav(3, 9).unwrap().label(), "3 / 3");
        assert_eq!(history_nav(20, 19).unwrap().label(), "20 / 20");
    }

    #[test]
    fn chevrons_draw_only_their_image_or_glyph() {
        for chevron in [PREVIOUS_CHEVRON, NEXT_CHEVRON] {
            assert_eq!(chevron.title(true), "", "{chevron:?}: image only");
            assert_eq!(chevron.title(false), chevron.glyph);
            assert_eq!(chevron.glyph.chars().count(), 1);
            assert!(!chevron.title(false).contains(chevron.name));
        }
        assert_eq!(PREVIOUS_CHEVRON.name, "Previous");
        assert_eq!(NEXT_CHEVRON.name, "Next");
        assert_eq!(PREVIOUS_CHEVRON.symbol, "chevron.left");
        assert_eq!(NEXT_CHEVRON.symbol, "chevron.right");
    }

    #[test]
    fn chevrons_follow_the_numbers() {
        assert_eq!(PREVIOUS_CHEVRON.command(), CommandId::HistoryNewer);
        assert_eq!(NEXT_CHEVRON.command(), CommandId::HistoryOlder);
        // At 1 / 3 only › works, at 3 / 3 only ‹, in between both.
        let first = history_nav(3, 0).unwrap();
        assert!(!PREVIOUS_CHEVRON.enabled(&first) && NEXT_CHEVRON.enabled(&first));
        let middle = history_nav(3, 1).unwrap();
        assert!(PREVIOUS_CHEVRON.enabled(&middle) && NEXT_CHEVRON.enabled(&middle));
        let last = history_nav(3, 2).unwrap();
        assert!(PREVIOUS_CHEVRON.enabled(&last) && !NEXT_CHEVRON.enabled(&last));
        // Walking with › from 1 reaches N, then stops; ‹ walks back to 1.
        let mut cursor = 0;
        let mut seen = vec![history_nav(3, cursor).unwrap().position];
        while let Some(next) = step_history(3, cursor, NEXT_CHEVRON.older) {
            cursor = next;
            seen.push(history_nav(3, cursor).unwrap().position);
        }
        assert_eq!(seen, vec![1, 2, 3]);
        while let Some(previous) = step_history(3, cursor, PREVIOUS_CHEVRON.older) {
            cursor = previous;
        }
        assert_eq!(history_nav(3, cursor).unwrap().position, 1);
    }

    #[test]
    fn history_capsule_fits_the_longest_position() {
        // "20 / 20" in 12 pt monospaced digits is about 40 pt wide.
        const { assert!(NAV_COUNT_W >= 40.0) };
        // Square chevron hit areas as tall as the capsule (a chip).
        const { assert!(NAV_BUTTON == CHIP_PILL_H) };
        assert_eq!(NAV_SPAN, NAV_BUTTON * 2.0 + NAV_COUNT_W);
    }

    #[test]
    fn history_arrows_keep_the_first_row_clear() {
        let widths = [62.0, 74.0, 78.0, 72.0, 86.0, 56.0];
        let width = 412.0;
        let frames = layout_chips(&widths, width, NAV_RESERVE);
        let nav_left = width - NAV_SPAN;
        assert!(frames.iter().any(|frame| frame.row > 0));
        for frame in frames.iter().filter(|frame| frame.row == 0) {
            assert!(frame.x + frame.width <= nav_left - super::CHIP_GAP + 0.01);
        }
    }

    #[test]
    fn save_in_the_dataframe_view_writes_csv() {
        let tsv = "Naam\tWanneer\nJan\t13/03/2026\nPiet\t14/03/2026";
        let file = text_save_file(tsv, CardView::Dataframe).expect("file");
        assert_eq!(
            (file.filename.as_str(), file.extension),
            ("clipboard.csv", "csv")
        );
        assert_eq!(
            std::str::from_utf8(&file.bytes).expect("UTF-8"),
            "Naam,Wanneer\nJan,2026-03-13\nPiet,2026-03-14\n"
        );
        assert_eq!(
            deferred_save_name(tsv, CardView::Dataframe),
            Some(("clipboard.csv".to_string(), "csv"))
        );
        assert_eq!(
            crate::open_file::save_name("export.tsv", "csv"),
            "export.csv"
        );
    }

    #[test]
    fn deferred_save_name_matches_the_built_file() {
        let sources = [
            "{\"name\":\"copycraft\",\"n\":3}",
            "name: copycraft\nn: 3\n",
            "<a><b>x</b></a>",
            "# Title\n\n- item\n",
            "name,age\nalice,30\nbob,40",
            "plain words",
        ];
        for src in sources {
            for view in [CardView::Original, CardView::Format, CardView::Dataframe] {
                let Some(file) = text_save_file(src, view) else {
                    continue;
                };
                let (filename, extension) = deferred_save_name(src, view).unwrap();
                assert_eq!(filename, file.filename, "{view:?} {src:?}");
                assert_eq!(extension, file.extension, "{view:?} {src:?}");
            }
        }
        assert!(deferred_save_name("{}", CardView::Convert).is_none());
        assert!(deferred_save_name("{}", CardView::Schema).is_none());
    }

    fn csv_rows(rows: usize) -> String {
        let mut csv = String::from("id,name,city\n");
        for i in 0..rows {
            csv.push_str(&format!("{i},name{i},City {}\n", i % 7));
        }
        csv
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1,000");
        assert_eq!(group_thousands(23_220), "23,220");
        assert_eq!(group_thousands(1_234_567), "1,234,567");
        assert_eq!(
            showing_note(200, 48_213, "rows"),
            "Showing 200 of 48,213 rows"
        );
    }

    #[test]
    fn short_text_is_not_a_preview() {
        let (excerpt, note) = excerpt_for("one\ntwo", false);
        assert_eq!(excerpt, "one\ntwo");
        assert!(note.is_none());
    }

    #[test]
    fn many_lines_preview_the_first_rows() {
        let body: String = (0..300).map(|i| format!("line {i}\n")).collect();
        let (excerpt, note) = excerpt_for(&body, false);
        assert!(excerpt.starts_with("line 0\n"));
        assert!(excerpt.contains(&format!("line {}", PREVIEW_ROWS - 1)));
        assert!(!excerpt.contains(&format!("line {}\n", PREVIEW_ROWS)));
        assert_eq!(note.as_deref(), Some("Showing 200 of 300 lines"));
        let (all, none) = excerpt_for(&body, true);
        assert_eq!(all, body);
        assert!(none.is_none());
    }

    #[test]
    fn one_long_line_previews_characters() {
        let body = "x".repeat(PREVIEW_CHARS * 2);
        let (excerpt, note) = excerpt_for(&body, false);
        assert!(excerpt.chars().count() < PREVIEW_CHARS + 5);
        assert_eq!(note.as_deref(), Some("Showing 20,000 of 40,000 characters"));
    }

    #[test]
    fn trailing_blank_lines_do_not_make_a_preview() {
        let body: String = (0..PREVIEW_ROWS)
            .map(|i| format!("{i}\n"))
            .collect::<String>()
            + "\n\n";
        assert!(excerpt_for(&body, false).1.is_none());
    }

    #[test]
    fn large_table_card_previews_rows_and_show_all_renders_them() {
        let src = csv_rows(300);
        let mut d = data(SubjectKind::Text, Some(&src));
        for view in [CardView::Original, CardView::Dataframe] {
            d.view = view;
            d.full = false;
            let card = work_card(&d);
            assert_eq!(
                card.preview_note.as_deref(),
                Some("Showing 200 of 300 rows"),
                "{view:?}"
            );
            assert!(card.excerpt.contains("name199"), "{view:?}");
            assert!(!card.excerpt.contains("name299"), "{view:?}");
            d.full = true;
            let full = work_card(&d);
            assert!(full.preview_note.is_none(), "{view:?}");
            if view == CardView::Original {
                assert!(full.excerpt.contains("name299"));
            }
        }
    }

    #[test]
    fn copy_and_save_use_the_whole_table_not_the_preview() {
        let src = csv_rows(300);
        let save = text_save_file(&src, CardView::Original).unwrap();
        assert_eq!(save.bytes, src.as_bytes());
        let copied = transformed_text(&src, CardView::Original).unwrap();
        assert_eq!(copied, src);
    }

    #[test]
    fn small_table_card_has_no_preview_note() {
        let src = csv_rows(20);
        let mut d = data(SubjectKind::Text, Some(&src));
        for view in [CardView::Original, CardView::Dataframe] {
            d.view = view;
            assert!(work_card(&d).preview_note.is_none(), "{view:?}");
        }
    }
}
