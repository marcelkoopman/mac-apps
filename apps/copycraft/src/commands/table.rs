//! Table/CSV presentation, the table window view and the column picker.

use super::*;

/// A copied CSV or TSV opens aligned and colored as that table.
/// The polars grid stays on the Dataframe button. Copy and save stay on the source.
/// A preview aligns only the header and the first [`PREVIEW_ROWS`] rows.
pub(crate) fn show_copied_table(card: &mut WorkCard, source: &str, full: bool) {
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
pub(crate) fn overview_note(preview: &dataframe::DataframePreview) -> String {
    crate::locale::tf("rows_by_columns", &[&preview.rows, &preview.columns])
}

/// "Header on line N" in the meta line, after the size, when lines above a table's header
/// were skipped ([`dataframe::TableStart::note`]).
pub(crate) fn add_table_note(card: &mut WorkCard, start: Option<dataframe::TableStart>) {
    if let Some(note) = start.and_then(|start| start.note()) {
        add_meta_note(card, &note);
    }
}

/// `note` in the meta line after the size, before the sensitivity labels.
pub(crate) fn add_meta_note(card: &mut WorkCard, note: &str) {
    add_note(&mut card.meta, note);
}

/// `note` right after the size in `meta`, before the labels and earlier notes.
pub(crate) fn add_note(meta: &mut String, note: &str) {
    *meta = match meta.find(META_SEPARATOR) {
        Some(at) => format!("{}{META_SEPARATOR}{note}{}", &meta[..at], &meta[at..]),
        None => format!("{meta}{META_SEPARATOR}{note}"),
    };
}

pub(crate) const META_SEPARATOR: &str = "  ·  ";

/// The table as a polars grid: whole with `full`, else its first [`PREVIEW_ROWS`] rows with a
/// note when the table has more. The meta measures the copied table when rows are left out,
/// and says where the header is and which date order was read when either is a guess.
/// The grid no longer has the table's delimiters, so a Salaris column would lose its financial
/// mark: the meta classifies the copied table.
pub(crate) fn show_dataframe(
    card: &mut WorkCard,
    source: &str,
    full: bool,
    overview: Option<bool>,
) {
    let max_rows = if full { usize::MAX } else { PREVIEW_ROWS };
    let Some(preview) = dataframe::try_format_preview(source, max_rows, overview) else {
        return;
    };
    card.title = crate::locale::t("table_view").to_string();
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
pub(crate) fn add_version_note(card: &mut WorkCard, table: &TableShown) {
    if table.version > 0
        && let Some(label) = table.labels.get(table.version)
    {
        add_meta_note(card, label);
    }
}

/// A table version after one or more steps, from its frame. Size, lines and sensitivity
/// labels are the version's own (as CSV): a step can drop or keep a sensitive column. Which
/// version it is shows in the version capsule ([`VersionBar`]), its step in the meta line.
pub(crate) fn show_table_version(card: &mut WorkCard, table: &TableShown, full: bool) {
    card.title = crate::locale::t("table_view").to_string();
    card.highlight = Some(FormatKind::Dataframe);
    card.preview_note = None;
    let Some(frame) = &table.frame else {
        card.excerpt.clear();
        card.placeholder = crate::locale::t("working_on_table").to_string();
        card.selectable = false;
        card.meta.clear();
        return;
    };
    let max_rows = if full { usize::MAX } else { PREVIEW_ROWS };
    let Some(preview) = dataframe::frame_preview(frame, max_rows, table.overview) else {
        card.excerpt.clear();
        card.placeholder = crate::locale::t("table_empty").to_string();
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
pub(crate) fn set_excerpt(card: &mut WorkCard, body: &str, full: bool) {
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
    let unit = match unit {
        "rows" => crate::locale::t("rows"),
        "lines" => crate::locale::t("lines"),
        "characters" => crate::locale::t("characters"),
        other => other,
    };
    crate::locale::tf(
        "showing_note",
        &[&group_thousands(shown), &group_thousands(total), &unit],
    )
}

pub(crate) fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let separator = match crate::locale::lang() {
        crate::locale::Lang::Nl => '.',
        crate::locale::Lang::En => ',',
    };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// Column gaps are display-only. A tab is drawn as a middle dot so the columns
/// land on character cells; the copied bytes keep their real delimiter.
pub(crate) const TSV_MARK: char = '\u{00b7}';

/// Lines above the header (see [`dataframe::table_start`]) stay as they are, above the table.
pub(crate) fn align_table(source: &str, kind: FormatKind) -> Option<String> {
    let (above, table) = match dataframe::table_start(source) {
        Some(start) if start.skipped > 0 => source.split_at(start.offset),
        _ => ("", source),
    };
    let aligned = align_rows(table, kind)?;
    Some(format!("{above}{aligned}"))
}

pub(crate) fn align_rows(source: &str, kind: FormatKind) -> Option<String> {
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

pub(crate) fn source_separator(source: &str, kind: FormatKind) -> Option<char> {
    match kind {
        FormatKind::Tsv => Some('\t'),
        FormatKind::Csv => csv_separator(source),
        _ => None,
    }
}

pub(crate) fn csv_separator(source: &str) -> Option<char> {
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

pub(crate) fn split_cells(line: &str, sep: char) -> Vec<String> {
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

pub(crate) fn render_row(cells: &[String], widths: &[usize], sep: char) -> String {
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
    if view == CardView::Diff {
        return CardView::Diff;
    }
    // An SVG opens as its picture (the card falls back to the code when it cannot draw it).
    if view == CardView::Original
        && opens_formatted(source)
        && format::detect(source) != FormatKind::Svg
    {
        CardView::Format
    } else {
        view
    }
}

/// Formatting `source` gives other text than the copy (trailing whitespace aside). Remembered
/// for the last few copies: every card update asks.
pub(crate) fn formatting_changes(source: &str) -> bool {
    use std::hash::{Hash, Hasher};
    thread_local! {
        static SEEN: std::cell::RefCell<Vec<(u64, bool)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    if let Some(known) = SEEN.with(|seen| {
        seen.borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
    }) {
        return known;
    }
    let changes = differs_formatted(source, &clipboard::formatted(source));
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        if seen.len() >= 16 {
            seen.remove(0);
        }
        seen.push((key, changes));
    });
    changes
}

/// `formatted` is another text than `source`, ignoring trailing whitespace.
pub fn differs_formatted(source: &str, formatted: &str) -> bool {
    let trim = |text: &str| -> String {
        text.trim_end()
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    };
    trim(source) != trim(formatted)
}

pub(crate) fn opens_formatted(source: &str) -> bool {
    matches!(
        format::detect(source),
        FormatKind::Json
            | FormatKind::Yaml
            | FormatKind::Xml
            | FormatKind::Svg
            | FormatKind::Html
            | FormatKind::Markdown
            | FormatKind::Agent
            | FormatKind::Rust
            | FormatKind::Java
            | FormatKind::Python
            | FormatKind::Toml
    )
}

/// Soft ceiling only for pathological tables (millions of rows as one NSTextView string).
/// Below this, Open table shows every row; above it the meta line says "Showing N of M rows".
pub const WINDOW_ROWS: usize = 100_000;

/// What the table window shows of the entry's table ([`table_window_view`]): the grid of the
/// version shown, its meta line, the columns for the sidebar, the versions for the version bar
/// and its Table ▾ menu. Zeroized when dropped (the grid and meta quote the table).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableWindowView {
    /// The version's grid (every row up to [`WINDOW_ROWS`]); empty while it is worked out.
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
    /// Name of the chosen file the table comes from, for the window title; `None` for a copy.
    pub source_name: Option<String>,
    /// The version's row count (the meta line's size), for the search's "120 of 64,659 rows".
    pub rows: usize,
    /// Which table this is (a hash of its copied text, in memory only): the version strip
    /// keeps the sizes of the versions it has shown for the same table only.
    pub table_key: u64,
    /// The app is still working on the table (a version, a step, the first read): the window
    /// shows its switch ([`super::VersionSwitch`]).
    pub working: bool,
    /// The copy's type mark (`csv`, `tsv`, `{}`), as its history card shows it
    /// ([`TableWindowView::set_kind`]); the version cards wear it.
    pub mark: String,
    /// The type's name for a version card's tooltip (*CSV*), as a history card's tooltip.
    pub kind_name: &'static str,
    /// The first lines of the copied text ([`super::thumb_lines`]): the original's card, as
    /// its history card shows them.
    pub original_lines: String,
    /// The first lines of the version shown, written as CSV; empty while it is worked out.
    pub lines: String,
    /// A version was just taken out ([`CommandId::RemoveTableVersion`]): ⌘Z puts it back
    /// ([`super::table_window_undo_key`]).
    pub can_restore: bool,
}

impl TableWindowView {
    /// The copy's type (from its history entry, or read from the text): the mark and the
    /// tooltip name the version cards wear.
    pub fn set_kind(&mut self, kind: crate::format::FormatKind) {
        self.mark = kind.menu_symbol().to_string();
        self.kind_name = super::thumb_kind_name(kind);
    }

    /// The window title: the chosen file's name, else "Table" (no app name, as the HIG asks).
    pub fn window_title(&self) -> String {
        table_window_title(self.source_name.as_deref())
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
        if let Some(name) = self.source_name.as_mut() {
            name.zeroize();
        }
        self.source_name = None;
        self.rows = 0;
        self.table_key = 0;
        self.working = false;
        self.mark.clear();
        self.kind_name = "";
        self.original_lines.zeroize();
        self.lines.zeroize();
    }
}

/// Title of the table window: `name` (a chosen file) when there is one, else "Table".
pub fn table_window_title(name: Option<&str>) -> String {
    match name.map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => crate::locale::t("table_window_title").to_string(),
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
    view.working = table.working;
    view.versions = VersionBar {
        labels: table.labels.clone(),
        current: table.version,
    };
    view.menu = table_window_menu(table);
    view.table_key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish() | 1
    };
    view.columns = picker_columns(table).unwrap_or_default();
    view.original_lines = super::thumb_lines(source);
    let Some(frame) = &table.frame else {
        view.placeholder = crate::locale::t("working_on_table").to_string();
        return view;
    };
    // Dedicated window: show every row (cap only for enormous tables — see WINDOW_ROWS).
    let (height, _) = frame.shape();
    let max_rows = height.clamp(1, WINDOW_ROWS);
    let Some(preview) = dataframe::frame_preview(frame, max_rows, Some(false)) else {
        view.placeholder = crate::locale::t("table_empty").to_string();
        return view;
    };
    let csv = Zeroizing::new(dataframe::frame_csv(frame).unwrap_or_default());
    view.lines = super::thumb_lines(&csv);
    let original = table.version == 0 && table.options == dataframe::ReadOptions::default();
    let classified: &str = if original { source } else { &csv };
    let mut meta = text_meta_from(&csv, classified);
    let (rows, columns) = frame.shape();
    view.rows = rows;
    let size_end = meta.find(META_SEPARATOR).unwrap_or(meta.len());
    meta.replace_range(..size_end, &super::table_size_text(rows, columns));
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

/// The table window's Table ▾ menu: the steps, Choose Columns…, and the sorts (Open table is
/// the card chip only).
pub fn table_window_menu(table: &TableShown) -> Vec<(Command, bool)> {
    let mut menu: Vec<(Command, bool)> = table_menu_items(Some(table))
        .into_iter()
        .filter(|(command, _)| command.id != CommandId::TableOpenWindow)
        .collect();
    if table.frame.is_some() {
        menu.push((table_copy_command(window_copy(table)), false));
    }
    menu
}

/// The table window's *Copy as JSON* and *Copy as CSV* toolbar buttons, in that order, each
/// with whether it is on: once the version's grid is there and Table ▾ can copy the table
/// (the same [`table_copy_command`]s, so a click does what the menu item does, toasts included).
pub fn table_window_copy_buttons(view: &TableWindowView) -> [(Command, bool); 2] {
    use crate::dataframe::TableCopy;
    let on = !view.grid.is_empty()
        && view
            .menu
            .iter()
            .any(|(command, _)| matches!(command.id, CommandId::TableCopyAs(_)));
    [
        (table_copy_command(TableCopy::Json), on),
        (table_copy_command(TableCopy::Csv), on),
    ]
}

/// A file name that opens in the table window when dropped: `.csv`, `.tsv` or `.json` (whose
/// text still has to read as a table).
pub fn is_table_file_name(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ["csv", "tsv", "json"]
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// The other form of the window's table: JSON for a delimited one, CSV for one read from JSON.
pub fn window_copy(table: &TableShown) -> crate::dataframe::TableCopy {
    match table.notes.and_then(|notes| notes.start) {
        Some(_) => crate::dataframe::TableCopy::Json,
        None => crate::dataframe::TableCopy::Csv,
    }
}

/// What Table ▾ shows for `menu`: the menu itself, or, while it is empty (no table yet, or
/// the window wiped), its steps and Choose Columns… dimmed, so the toolbar item opens a menu
/// that says what is coming instead of beeping. The sorts need the columns and wait.
pub fn table_window_menu_shown(menu: &[(Command, bool)]) -> Vec<(Command, bool)> {
    if !menu.is_empty() {
        return menu.to_vec();
    }
    let mut steps = super::table_steps();
    steps.push(command(
        CommandId::TableChooseColumns,
        super::choose_columns_title(),
        crate::locale::t("choose_columns_detail"),
        "",
    ));
    steps
        .into_iter()
        .map(|mut command| {
            command.enabled = false;
            (command, false)
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
