/// Most text lines skipped above a table's header (a title, a "Definition: …" line, a note).
const MAX_PREAMBLE_LINES: usize = 10;

/// Where the table in a copy starts. Exports often put a title or a definition line above the
/// header; those lines are not part of the table and are skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableStart {
    pub separator: u8,
    /// The header's line, 0-based, counting every line of the text (blank ones too).
    pub header_line: usize,
    /// Byte offset of the header in the text.
    pub offset: usize,
    /// Non-empty lines above the header.
    pub skipped: usize,
}

impl TableStart {
    /// The table: the header and everything after it.
    pub fn body<'a>(&self, text: &'a str) -> &'a str {
        text[self.offset..].trim_end()
    }


    /// "Header on line N" when lines above the header were skipped.
    pub fn note(&self) -> Option<String> {
        (self.skipped > 0).then(|| format!("Header on line {}", self.header_line + 1))
    }
}

/// The table in `text`: its header is the first non-empty line, or a later one when at most
/// [`MAX_PREAMBLE_LINES`] lines above it do not fit the table (fewer fields than the header)
/// and the rows below it nearly all have the header's width. `None` when `text` holds no
/// delimited table.
pub fn table_start(text: &str) -> Option<TableStart> {
    let mut skipped = 0;
    let mut line_offset = 0;
    for (index, raw) in text.split('\n').enumerate() {
        let offset = line_offset + (raw.len() - raw.trim_start().len());
        line_offset += raw.len() + 1;
        if raw.trim().is_empty() {
            continue;
        }
        if let Some(start) = table_at(text, index, offset, skipped) {
            return Some(start);
        }
        // Only words are skipped: not a lone `[` or `{` above JSON, not a row of numbers.
        skipped += 1;
        if skipped > MAX_PREAMBLE_LINES || !raw.chars().any(char::is_alphabetic) {
            return None;
        }
    }
    None
}

/// The table in `text` with its header on line `header_line` (0-based, blank lines counted), as
/// the card was told ("Header on line N"): the lines above are skipped whatever they hold.
pub fn table_start_at(text: &str, header_line: usize) -> Option<TableStart> {
    let mut offset = 0;
    let mut skipped = 0;
    for (index, raw) in text.split('\n').enumerate() {
        if index == header_line {
            let lead = raw.len() - raw.trim_start().len();
            let separator = detect_separator(raw.trim())?;
            return Some(TableStart {
                separator,
                header_line,
                offset: offset + lead,
                skipped,
            });
        }
        if !raw.trim().is_empty() {
            skipped += 1;
        }
        offset += raw.len() + 1;
    }
    None
}

fn table_at(text: &str, header_line: usize, offset: usize, skipped: usize) -> Option<TableStart> {
    let body = &text[offset..];
    let header = body.lines().next()?.trim();
    let separator = detect_separator(header)?;
    if !looks_like_delimited_table(body, separator) {
        return None;
    }
    if skipped > 0 {
        let sep = separator as char;
        let width = delimited_field_count(header, sep);
        // A line above as wide as the header would be part of the table.
        let narrower_above = text[..offset]
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .all(|line| delimited_field_count(line, sep) < width);
        if !narrower_above || !looks_like_header(header, sep) || !rows_fit_header(body, sep, width)
        {
            return None;
        }
    }
    Some(TableStart {
        separator,
        header_line,
        offset,
        skipped,
    })
}

/// At least four in five of the first rows below the header have its width. Stricter than
/// [`looks_like_delimited_table`], because skipping lines makes a table out of more texts.
fn rows_fit_header(body: &str, sep: char, width: usize) -> bool {
    let rows: Vec<&str> = body
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect();
    let fitting = rows
        .iter()
        .filter(|line| delimited_field_count(line, sep) == width)
        .count();
    !rows.is_empty() && fitting * 5 >= rows.len() * 4
}

/// Every field of `header` is a name: not empty, not a number, and without code punctuation
/// (`{`, `}`, `=`, `;` or unbalanced brackets), so a run of similar code lines is not taken
/// for a table below a few skipped lines.
fn looks_like_header(header: &str, sep: char) -> bool {
    delimited_fields(header, sep).all(|field| {
        let field = field.trim().trim_matches('"');
        let balanced = |open: char, close: char| field.matches(open).count() == field.matches(close).count();
        !field.is_empty()
            && field.parse::<f64>().is_err()
            && !field.contains(['{', '}', '=', ';'])
            && balanced('(', ')')
            && balanced('[', ']')
    })
}
