use zeroize::Zeroize;

use crate::appearance::Theme;
use crate::clipboard;
use crate::convert;
use crate::dataframe;
use crate::decode;
use crate::format::{self, FormatKind};
use crate::toolbar_visibility;

pub const MAX_VISIBLE: usize = 8;
pub const CHIP_PITCH: f64 = 34.0;
pub const CHIP_PILL_H: f64 = 28.0;

const CHIP_GAP: f64 = 6.0;
/// History arrow buttons. Same height as a chip, wide enough for `<` and `>`.
pub const NAV_BUTTON: f64 = 32.0;
pub const NAV_GAP: f64 = CHIP_GAP;
pub const NAV_SPAN: f64 = NAV_BUTTON + NAV_GAP + NAV_BUTTON;
/// Empty space kept on the right of the first chip row so the arrows fit.
pub const NAV_RESERVE: f64 = CHIP_GAP + NAV_SPAN;
const EXCERPT_LINES: usize = 6;
const EXCERPT_LINE_CHARS: usize = 48;
/// Formatted code stays whole so the card can color real tokens. Past this, the tail is cut.
const CODE_CAP: usize = 80_000;
/// Lines (table rows) a card shows before "Show all". Enough to read the data, quick to lay out.
pub const PREVIEW_ROWS: usize = 200;
/// Characters a card shows before "Show all", for text with very long lines.
pub const PREVIEW_CHARS: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubjectKind {
    Empty,
    NoText,
    Image,
    Text,
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

/// Which way history can move. Index 0 is the newest copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryNav {
    pub can_older: bool,
    pub can_newer: bool,
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
    /// YouTube page, when the card should load a video thumbnail.
    pub link_page: Option<String>,
    pub link_thumb: Option<String>,
    /// Set when the excerpt is a preview of a longer text, for example
    /// "Showing 200 of 23,220 rows". "Show all" renders the rest.
    pub preview_note: Option<String>,
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
    if let Some(image) = &data.image {
        image.format.hash(&mut hasher);
        image.width.hash(&mut hasher);
        image.height.hash(&mut hasher);
        image.byte_len.hash(&mut hasher);
    }
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
            link_thumb: None,
            preview_note: None,
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
                link_thumb: None,
                preview_note,
            };
            apply_text_view(&mut card, text, presented_view(text, data.view), data.full);
            card
        }
        SubjectKind::Empty => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: "Nothing copied".to_string(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            link_thumb: None,
            preview_note: None,
        },
        SubjectKind::NoText => WorkCard {
            title: "Clipboard".to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: "No text on the clipboard".to_string(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            link_thumb: None,
            preview_note: None,
        },
    }
}

fn image_card(data: &LaunchData) -> WorkCard {
    let facts = data.image.as_ref().map(image_meta).unwrap_or_default();
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
            link_thumb: None,
            preview_note: None,
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
        link_thumb: None,
        preview_note: None,
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
fn apply_text_view(card: &mut WorkCard, source: &str, view: CardView, full: bool) {
    if view == CardView::Original {
        show_copied_table(card, source, full);
        return;
    }
    if view == CardView::Dataframe && !full {
        show_dataframe_preview(card, source);
        return;
    }
    let Some(body) = transformed_text(source, view) else {
        return;
    };
    let kind = format::detect(source);
    card.title = if view == CardView::Format && matches!(kind, FormatKind::Json | FormatKind::Xml) {
        text_title(source)
    } else if view == CardView::Format && body == source {
        kind.source_heading().to_string()
    } else {
        text_view_title(source, view, &body)
    };
    // The grid no longer has the table's delimiters, so a Salaris column
    // would lose its financial mark. Classify the copied table.
    card.meta = if view == CardView::Dataframe {
        text_meta_from(&body, source)
    } else {
        text_meta(&body)
    };
    if matches!(view, CardView::Schema | CardView::Sample) {
        set_excerpt(card, &body, full);
        card.highlight = Some(if view == CardView::Sample || kind == FormatKind::Xml {
            FormatKind::Xml
        } else {
            FormatKind::Json
        });
        card.selectable = true;
    } else if view == CardView::Dataframe {
        // The grid's header chrome is already six lines, so a short excerpt hides every row.
        set_excerpt(card, &body, full);
        card.highlight = Some(FormatKind::Dataframe);
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
    if cut.is_some() {
        let rows = source.lines().count().saturating_sub(1);
        let shown_rows = shown.lines().count().saturating_sub(1);
        card.excerpt = body;
        card.preview_note = Some(showing_note(shown_rows, rows, "rows"));
        card.meta = text_meta(source);
    } else {
        set_excerpt(card, &body, full);
        card.meta = text_meta_from(&body, source);
    }
}

/// The first [`PREVIEW_ROWS`] rows of the table as a polars grid, with a note when the table
/// has more. Title and meta as for the full grid; the meta measures the copied table.
fn show_dataframe_preview(card: &mut WorkCard, source: &str) {
    let Some(preview) = dataframe::try_format_preview(source, PREVIEW_ROWS) else {
        return;
    };
    card.title = "Dataframe".to_string();
    card.highlight = Some(FormatKind::Dataframe);
    card.selectable = true;
    if preview.rows > preview.shown_rows {
        card.meta = text_meta(source);
        card.preview_note = Some(showing_note(preview.shown_rows, preview.rows, "rows"));
        card.excerpt = preview.grid;
    } else {
        card.meta = text_meta_from(&preview.grid, source);
        card.excerpt = shown_body(&preview.grid);
        card.preview_note = None;
    }
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

fn group_thousands(n: usize) -> String {
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

fn align_table(source: &str, kind: FormatKind) -> Option<String> {
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
            | FormatKind::Markdown
            | FormatKind::Rust
            | FormatKind::Java
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
        let bytes = dataframe::try_parquet_bytes(source)?;
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

fn page_card(text: &str) -> Option<WorkCard> {
    let page = crate::page_preview::page_url(text)?;
    let host = crate::page_preview::host(page).unwrap_or("Page");
    Some(WorkCard {
        title: host.to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        selectable: false,
        link_page: Some(page.to_string()),
        link_thumb: None,
        preview_note: None,
    })
}

fn youtube_card(text: &str) -> Option<WorkCard> {
    let id = crate::youtube::video_id(text)?;
    Some(WorkCard {
        title: "YouTube".to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        selectable: false,
        link_page: Some(text.trim().to_string()),
        link_thumb: Some(crate::youtube::thumbnail_url(id)),
        preview_note: None,
    })
}

/// Actions for the thing on the clipboard. Housekeeping stays in [`overflow`].
pub fn chips(data: &LaunchData) -> Vec<Command> {
    match data.subject_kind {
        SubjectKind::Image => image_chips(data.image_scan.as_ref()),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            if crate::youtube::video_id(text).is_some()
                || crate::page_preview::page_url(text).is_some()
            {
                link_chips(text)
            } else {
                text_chips(text)
            }
        }
        SubjectKind::Empty | SubjectKind::NoText => Vec::new(),
    }
}

/// Chips, earlier copies, and appearance. Quit stays out.
pub fn search_pool(data: &LaunchData) -> Vec<Command> {
    let mut commands = chips(data);
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
        "Empty pasteboard",
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
            "Clear history",
            "Forget copies",
            "clear history forget",
        ));
    }
    for theme in [Theme::System, Theme::Light, Theme::Dark] {
        commands.push(command(
            CommandId::Appearance(theme),
            theme_name(theme),
            "Appearance",
            "appearance theme",
        ));
    }
    commands.push(command(
        CommandId::Quit,
        "Quit",
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
        if x > 0.0 && x + chip > limit {
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

/// `<` and `>` stay on the card. `cursor` 0 is the newest entry.
/// A direction with nowhere to go stays visible and faded.
pub fn history_nav(len: usize, cursor: usize) -> Option<HistoryNav> {
    if len < 2 {
        return Some(HistoryNav {
            can_older: false,
            can_newer: false,
        });
    }
    let cursor = cursor.min(len - 1);
    Some(HistoryNav {
        can_older: cursor + 1 < len,
        can_newer: cursor > 0,
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
    if target < 0 {
        return index;
    }
    let center = current.x + current.width / 2.0;
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
        .unwrap_or(index)
}

pub fn keeps_card_open(id: &CommandId) -> bool {
    matches!(
        id,
        CommandId::History(_)
            | CommandId::HistoryOlder
            | CommandId::HistoryNewer
            | CommandId::Appearance(_)
            | CommandId::ClearClipboard
            | CommandId::ClearHistory
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
    if toolbar_visibility::shows_format(text) && !opens_formatted(text) {
        commands.push(command(
            CommandId::Format,
            "Format",
            kind.source_heading(),
            "format pretty print",
        ));
    }
    if kind == FormatKind::Json {
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
    if toolbar_visibility::shows_convert(text) {
        commands.push(command(
            CommandId::Convert,
            "Convert",
            "Another format",
            "convert yaml json csv",
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
            CommandId::Dataframe,
            "Dataframe",
            "Table",
            "dataframe table csv tsv",
        ));
    }
    finish_modes(commands)
}

fn image_chips(scan: Option<&ImageScan>) -> Vec<Command> {
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
    command(CommandId::Original, "Original", "Source", "original source")
}

/// Copy and Save sit on the well, on the view that is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentActions {
    pub copy: bool,
    pub save: bool,
}

pub fn content_actions(data: &LaunchData) -> ContentActions {
    match data.subject_kind {
        SubjectKind::Empty | SubjectKind::NoText => ContentActions {
            copy: false,
            save: false,
        },
        SubjectKind::Image => image_content_actions(data),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            if crate::youtube::video_id(text).is_some()
                || crate::page_preview::page_url(text).is_some()
            {
                ContentActions {
                    copy: false,
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
    let labels = crate::sensitivity::labels(classified);
    if !labels.is_empty() {
        meta.push_str("  ·  ");
        meta.push_str(&crate::sensitivity::label_line(&labels));
    }
    meta
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
        Theme::System => "System",
        Theme::Light => "Light",
        Theme::Dark => "Dark",
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
    use super::{
        CardView, CommandId, ContentActions, Hist, ImageFacts, ImageScan, LaunchData, NAV_RESERVE,
        NAV_SPAN, SubjectKind, chip_width, chips, content_actions, content_key, copy_tip,
        deferred_save_name, history_label, history_nav, keeps_card_open, layout_chips,
        masks_content, matching, overflow, payload_excerpt, presented_view, save_tip, search_pool,
        step_chip, step_history, text_save_file, transformed_text, well_mask, work_card,
    };
    use super::{PREVIEW_CHARS, PREVIEW_ROWS, excerpt_for, group_thousands, showing_note};
    use crate::appearance::Theme;

    fn data(kind: SubjectKind, text: Option<&str>) -> LaunchData {
        LaunchData {
            subject_kind: kind,
            subject_text: text.map(str::to_string),
            image: None,
            history: Vec::new(),
            can_clear_history: false,
            history_nav: None,
            theme: Theme::System,
            view: CardView::Original,
            image_scan: None,
            source_name: None,
            source_note: None,
            full: false,
        }
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
    fn yaml_opens_formatted_and_coloured_without_convert() {
        let src = "name:   copycraft\nitems:\n  - one\n";
        let input = data(SubjectKind::Text, Some(src));
        assert_eq!(crate::format::detect(src), crate::format::FormatKind::Yaml);
        let shown = chips(&input);
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Convert));
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
                .all(|cmd| cmd.id != CommandId::Convert && cmd.id != CommandId::Format)
        );
    }

    #[test]
    fn song_yaml_opens_formatted_without_convert() {
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
        assert!(shown.iter().all(|cmd| cmd.id != CommandId::Convert));
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
                    .any(|cmd| cmd.id == CommandId::Dataframe),
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
                shown.iter().any(|cmd| cmd.id == CommandId::Dataframe),
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
    fn credential_url_stays_a_link_card_without_a_preview_fetch() {
        let url = "https://deploy:s3cr3t@github.com/acme/app.git";
        let input = data(SubjectKind::Text, Some(url));
        let card = work_card(&input);
        assert!(card.link_page.is_some());
        assert_eq!(card.title, "github.com");
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
    fn youtube_url_previews_as_a_thumbnail() {
        let url = "https://www.youtube.com/watch?v=bEN9Dyg48b0";
        let card = work_card(&data(SubjectKind::Text, Some(url)));
        assert_eq!(card.title, "YouTube");
        assert_eq!(card.link_page.as_deref(), Some(url));
        assert_eq!(
            card.link_thumb.as_deref(),
            Some("https://i.ytimg.com/vi/bEN9Dyg48b0/hqdefault.jpg")
        );
        assert_eq!(card.excerpt, url);
        assert!(!card.shows_image);
        let input = data(SubjectKind::Text, Some(url));
        assert_eq!(titles(&chips(&input)), vec!["Visit"]);
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: false
            }
        );
    }

    #[test]
    fn html_url_previews_like_a_page() {
        let url = "https://www.example.com/news/story";
        let input = data(SubjectKind::Text, Some(url));
        let card = work_card(&input);
        assert_eq!(card.title, "example.com");
        assert_eq!(card.link_page.as_deref(), Some(url));
        assert!(card.link_thumb.is_none());
        assert_eq!(card.excerpt, url);
        assert_eq!(titles(&chips(&input)), vec!["Visit"]);
        assert_eq!(
            content_actions(&input),
            ContentActions {
                copy: false,
                save: false
            }
        );
    }

    #[test]
    fn bare_host_previews_as_a_page() {
        let input = data(SubjectKind::Text, Some("grok.com"));
        let card = work_card(&input);
        assert_eq!(card.title, "grok.com");
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
    fn empty_clipboard_has_no_chips_and_keeps_quit_in_the_menu() {
        let input = data(SubjectKind::Empty, None);
        assert!(chips(&input).is_empty());
        let card = work_card(&input);
        assert_eq!(card.placeholder, "Nothing copied");
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
    }

    #[test]
    fn history_nav_stays_on_the_card() {
        let empty = history_nav(0, 0).unwrap();
        assert!(!empty.can_older && !empty.can_newer);
        let only = history_nav(1, 0).unwrap();
        assert!(!only.can_older && !only.can_newer);
        let newest = history_nav(3, 0).unwrap();
        assert!(newest.can_older);
        assert!(!newest.can_newer);
        let middle = history_nav(3, 1).unwrap();
        assert!(middle.can_older && middle.can_newer);
        let oldest = history_nav(3, 2).unwrap();
        assert!(!oldest.can_older);
        assert!(oldest.can_newer);
        assert_eq!(step_history(3, 0, true), Some(1));
        assert_eq!(step_history(3, 0, false), None);
        assert_eq!(step_history(3, 2, true), None);
        assert_eq!(step_history(3, 2, false), Some(1));
        assert_eq!(step_history(0, 0, true), None);
        assert_eq!(step_history(1, 0, true), None);
        assert_eq!(step_history(1, 0, false), None);
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
