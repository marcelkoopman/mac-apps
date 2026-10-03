//! Keeps `testdata/README.md` (the manifest) and the app in step: every text file in the
//! manifest is loaded and the card's title, labels and chips, and the row's checks, are
//! asserted for the text a Notion code block copies (the file without its final newline). The
//! file as committed must get the same labels and pass the same checks; its title and chips
//! can differ (the final newline gives a link a Format chip). Images are checked for size and
//! EXIF only: QR, OCR and the picture steps need macOS (Vision, ImageIO).

use std::path::{Path, PathBuf};

use crate::commands::{CardView, LaunchData, SubjectKind, chips, transformed_text, work_card};
use crate::table::TableOp;

const START: &str = "<!-- manifest:start -->";
const END: &str = "<!-- manifest:end -->";
/// An empty Labels or Chips cell.
const NONE: &str = "—";

struct Row {
    file: String,
    notion: String,
    title: String,
    labels: String,
    chips: String,
    checks: Vec<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

/// A cell without the backticks around it.
fn cell(raw: &str) -> String {
    let raw = raw.trim();
    raw.strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .unwrap_or(raw)
        .to_string()
}

fn manifest() -> Vec<Row> {
    let text = std::fs::read_to_string(root().join("README.md")).expect("testdata/README.md");
    let start = text.find(START).expect("manifest start marker") + START.len();
    let end = text.find(END).expect("manifest end marker");
    let mut rows = Vec::new();
    for line in text[start..end].lines().map(str::trim) {
        if !line.starts_with('|') || line.starts_with("|--") || line.starts_with("| File ") {
            continue;
        }
        let cells: Vec<String> = line.trim_matches('|').split(" | ").map(cell).collect();
        assert_eq!(cells.len(), 7, "manifest row needs 7 cells: {line}");
        rows.push(Row {
            file: cells[0].clone(),
            notion: cells[2].clone(),
            title: cells[3].clone(),
            labels: cells[4].clone(),
            chips: cells[5].clone(),
            checks: cells[6]
                .split("; ")
                .map(str::trim)
                .filter(|check| !check.is_empty())
                .map(str::to_string)
                .collect(),
        });
    }
    rows
}

/// The card's input for copied `text`: blank text is an empty clipboard, as
/// [`crate::clipboard`] and the macOS pasteboard read it.
fn data(text: &str, view: CardView) -> LaunchData {
    let blank = text.trim().is_empty();
    LaunchData {
        subject_kind: if blank {
            SubjectKind::Empty
        } else {
            SubjectKind::Text
        },
        subject_text: (!blank).then(|| text.to_string()),
        image: None,
        history: Vec::new(),
        can_clear_history: false,
        history_nav: None,
        theme: crate::appearance::Theme::System,
        settings: crate::settings::Settings::default(),
        view,
        image_scan: None,
        source_name: None,
        source_note: None,
        full: false,
        picture: None,
        table: None,
        image_edit: None,
    }
}

/// Labels on the card's meta line, as shown (`—` for none).
fn shown_labels(meta: &str) -> String {
    let names: Vec<&str> = crate::sensitivity::warning_marks(meta)
        .iter()
        .map(|mark| mark.label.name())
        .collect();
    if names.is_empty() {
        NONE.to_string()
    } else {
        names.join(" · ")
    }
}

fn shown_chips(data: &LaunchData) -> String {
    let titles: Vec<String> = chips(data).into_iter().map(|chip| chip.title).collect();
    if titles.is_empty() {
        NONE.to_string()
    } else {
        titles.join(", ")
    }
}

/// What Visit opens for a link (`menubar::visit_current`, without opening it).
fn visit_url(text: &str) -> Option<String> {
    let raw = crate::page_preview::page_url(text)
        .map(str::to_string)
        .or_else(|| crate::youtube::video_id(text).map(|_| text.trim().to_string()))?;
    let url = crate::format::format_text(&raw);
    let url = crate::page_preview::strip_userinfo(&url);
    crate::url_policy::may_visit(&url).then_some(url)
}

fn table_op(name: &str) -> TableOp {
    if let Some(column) = name
        .strip_prefix("ValueCounts(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return TableOp::ValueCounts {
            column: column.to_string(),
        };
    }
    match name {
        "Dedupe" => TableOp::Dedupe,
        "DropEmpty" => TableOp::DropEmpty,
        "DropConstant" => TableOp::DropConstant,
        "FixTypes" => TableOp::FixTypes,
        "Transpose" => TableOp::Transpose,
        other => panic!("unknown table step {other}"),
    }
}

/// The table after the steps `ops` (`Dedupe>DropEmpty`), each from the version before.
fn after_steps(text: &str, ops: &str) -> polars::prelude::DataFrame {
    let mut versions = crate::table::TableVersions::default();
    for op in ops.split('>') {
        let job = versions.push(table_op(op), text).expect("table step");
        let done = job
            .run(&std::sync::atomic::AtomicBool::new(false))
            .expect("table job");
        assert!(versions.finish(done), "table step {op} was not taken");
    }
    versions.frame().expect("table frame").clone()
}

fn shape(df: &polars::prelude::DataFrame) -> String {
    let (rows, cols) = df.shape();
    format!("{rows}x{cols}")
}

fn dtypes(df: &polars::prelude::DataFrame) -> String {
    df.dtypes()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn view_text(text: &str, view: CardView) -> String {
    transformed_text(text, view).unwrap_or_default()
}

/// The first failed check for `text`, if any.
fn check(text: &str, check: &str) -> Result<(), String> {
    let fail = |actual: &dyn std::fmt::Debug| Err(format!("{check}: got {actual:?}"));
    let card = || work_card(&data(text, CardView::Original));
    let table = || crate::dataframe::parse_table_with(text, Default::default());
    if let Some(step) = check.strip_prefix("step:") {
        let (ops, expect) = step
            .split_once(['=', '~', ':'])
            .ok_or_else(|| format!("bad step check {check}"))?;
        let df = after_steps(text, ops);
        let rest = &step[ops.len()..];
        let ok = if let Some(want) = rest.strip_prefix(":dtypes=") {
            dtypes(&df) == want
        } else if let Some(want) = rest.strip_prefix('=') {
            shape(&df) == want
        } else if let Some(want) = rest.strip_prefix('~') {
            df.to_string().contains(want)
        } else {
            return Err(format!("bad step check {check} ({expect})"));
        };
        return if ok { Ok(()) } else { fail(&df) };
    }
    let (key, op, want) = match check.find(['=', '~']) {
        Some(at) => (&check[..at], &check[at..=at], &check[at + 1..]),
        None => (check, "", ""),
    };
    let matches = |actual: &str| match op {
        "=" => actual == want,
        _ => actual.contains(want),
    };
    let actual: String = match key {
        // The kind of the Notion copy: a final newline makes one line multi-line text.
        "kind" => format!(
            "{:?}",
            crate::format::detect(text.strip_suffix('\n').unwrap_or(text))
        ),
        "meta" => card().meta,
        "placeholder" => card().placeholder,
        "link" => card().link_page.unwrap_or_default(),
        "visit" => visit_url(text).unwrap_or_default(),
        "convert" if op == "=" => {
            let body = transformed_text(text, CardView::Convert).ok_or("no Convert result")?;
            format!("{:?}", crate::format::detect(&body))
        }
        "convert" => view_text(text, CardView::Convert),
        "decode" => view_text(text, CardView::Decode),
        "schema" => view_text(text, CardView::Schema),
        "sample" => view_text(text, CardView::Sample),
        "dataframe" => view_text(text, CardView::Dataframe),
        "dfcard" => work_card(&data(text, CardView::Dataframe)).excerpt,
        "read" => table().map(|(df, _)| shape(&df)).unwrap_or_default(),
        "dtypes" => table().map(|(df, _)| dtypes(&df)).unwrap_or_default(),
        "notes" => match table() {
            Some((_, notes)) if notes.meta_notes().is_empty() => "none".to_string(),
            Some((_, notes)) => notes.meta_notes().join(", "),
            None => String::new(),
        },
        "columns" => {
            let (df, _) = table().ok_or("not a table")?;
            let names: Vec<String> = df
                .get_column_names()
                .iter()
                .map(|name| name.to_string())
                .collect();
            let shown = crate::dataframe::display_names(&names);
            return if shown.iter().any(|name| name == want) {
                Ok(())
            } else {
                fail(&shown)
            };
        }
        _ => return Err(format!("unknown check {check}")),
    };
    if matches(&actual) {
        Ok(())
    } else {
        fail(&actual)
    }
}

/// `full`: also the title and chips (the Notion copy).
fn check_text(row: &Row, text: &str, full: bool, wrong: &mut Vec<String>) {
    let variant = if full { "Notion copy" } else { "as committed" };
    let card = work_card(&data(text, CardView::Original));
    let mut fail = |what: &str, got: &str, want: &str| {
        wrong.push(format!(
            "{} ({variant}): {what} is {got:?}, manifest says {want:?}",
            row.file
        ));
    };
    if full && card.title != row.title {
        fail("title", &card.title, &row.title);
    }
    let labels = shown_labels(&card.meta);
    if labels != row.labels {
        fail("labels", &labels, &row.labels);
    }
    let chips = shown_chips(&data(text, CardView::Original));
    if full && chips != row.chips {
        fail("chips", &chips, &row.chips);
    }
    for item in &row.checks {
        if let Err(problem) = check(text, item) {
            wrong.push(format!("{} ({variant}): {problem}", row.file));
        }
    }
}

fn check_image(row: &Row, wrong: &mut Vec<String>) {
    let path = root().join(&row.file);
    let bytes = std::fs::read(&path).expect("image file");
    for item in row.checks.iter().filter(|item| *item != "mac-only") {
        let ok = if let Some(want) = item.strip_prefix("size=") {
            let image = image::load_from_memory(&bytes).expect("image decodes");
            format!("{}x{}", image.width(), image.height()) == want
        } else if let Some(want) = item.strip_prefix("exif-orientation=") {
            jpeg_orientation(&bytes).map(|n| n.to_string()).as_deref() == Some(want)
        } else if item == "exif-gps" {
            jpeg_exif(&bytes).is_some_and(|exif| has_tag(&exif, 0x8825))
        } else {
            wrong.push(format!("{}: unknown image check {item}", row.file));
            continue;
        };
        if !ok {
            wrong.push(format!("{}: {item} does not hold", row.file));
        }
    }
}

/// The raw EXIF block (TIFF header onward) of a JPEG.
fn jpeg_exif(bytes: &[u8]) -> Option<Vec<u8>> {
    use image::ImageDecoder;
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    decoder.exif_metadata().ok()?
}

fn jpeg_orientation(bytes: &[u8]) -> Option<u8> {
    let exif = jpeg_exif(bytes)?;
    image::metadata::Orientation::from_exif_chunk(&exif).map(|o| o.to_exif())
}

/// Whether IFD0 of a TIFF/EXIF block has an entry with `tag`.
fn has_tag(exif: &[u8], tag: u16) -> bool {
    let little = exif.starts_with(b"II");
    let u16_at = |at: usize| -> Option<u16> {
        let b = exif.get(at..at + 2)?;
        Some(if little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    };
    let Some(b) = exif.get(4..8) else {
        return false;
    };
    let ifd = if little {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    } as usize;
    let count = u16_at(ifd).unwrap_or(0) as usize;
    (0..count).any(|i| u16_at(ifd + 2 + i * 12) == Some(tag))
}

#[test]
fn manifest_matches_the_app() {
    let rows = manifest();
    assert!(rows.len() >= 60, "manifest rows: {}", rows.len());
    let mut wrong = Vec::new();
    for row in &rows {
        if row.checks.iter().any(|check| check == "mac-only") {
            check_image(row, &mut wrong);
            continue;
        }
        let text = std::fs::read_to_string(root().join(&row.file))
            .unwrap_or_else(|err| panic!("{}: {err}", row.file));
        let copied = text.strip_suffix('\n').unwrap_or(&text);
        check_text(row, copied, true, &mut wrong);
        check_text(row, &text, false, &mut wrong);
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_file_is_in_the_manifest_once() {
    let rows = manifest();
    let mut listed: Vec<&str> = rows.iter().map(|row| row.file.as_str()).collect();
    listed.sort_unstable();
    let before = listed.len();
    listed.dedup();
    assert_eq!(before, listed.len(), "a file is listed twice");
    let mut on_disk = Vec::new();
    for dir in std::fs::read_dir(root()).expect("testdata") {
        let dir = dir.expect("entry").path();
        if !dir.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&dir).expect("dir") {
            let path = file.expect("entry").path();
            if path.ends_with("images/make_images.py") {
                continue;
            }
            on_disk.push(
                path.strip_prefix(root())
                    .expect("under testdata")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    on_disk.sort_unstable();
    assert_eq!(
        listed,
        on_disk.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

#[test]
fn notion_languages_are_known() {
    const KNOWN: [&str; 10] = [
        "json",
        "yaml",
        "xml",
        "html",
        "markdown",
        "rust",
        "java",
        "python",
        "plain text",
        NONE,
    ];
    for row in manifest() {
        assert!(
            KNOWN.contains(&row.notion.as_str()),
            "{}: {}",
            row.file,
            row.notion
        );
    }
}

/// Exact bytes the scenarios rely on.
#[test]
fn exact_bytes_survive() {
    let read = |name: &str| std::fs::read(root().join(name)).expect(name);
    let tsv = read("convert/tsv_to_csv.tsv");
    assert_eq!(tsv.iter().filter(|&&b| b == b'\t').count(), 12);
    let mixed = read("detect/python_mixed_tabs.py");
    assert!(mixed.windows(2).any(|w| w == b"\n\t") && mixed.windows(5).any(|w| w == b"\n    "));
    assert_eq!(
        read("detect/python_one_liner.txt"),
        b"f = open(\"demofile.txt\")\n"
    );
    assert!(read("edge/empty.txt").is_empty());
    for row in manifest().iter().filter(|row| row.notion != NONE) {
        let bytes = read(&row.file);
        assert!(!bytes.contains(&b'\r'), "{}: CRLF", row.file);
        {
            let text = String::from_utf8(bytes).expect("UTF-8");
            for curly in ['\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}'] {
                assert!(!text.contains(curly), "{}: curly quote", row.file);
            }
        }
    }
}

/// What the card shows for each text file today, to update the manifest:
/// `cargo test -p copycraft testdata_manifest -- --ignored --nocapture`.
#[test]
#[ignore]
fn print_what_the_card_shows() {
    for row in manifest() {
        if row.checks.iter().any(|check| check == "mac-only") {
            continue;
        }
        let text = std::fs::read_to_string(root().join(&row.file)).expect("file");
        let text = text.strip_suffix('\n').unwrap_or(&text);
        let input = data(text, CardView::Original);
        let card = work_card(&input);
        println!(
            "| `{}` | {} | {} | {} | kind={:?} |",
            row.file,
            card.title,
            shown_labels(&card.meta),
            shown_chips(&input),
            crate::format::detect(text),
        );
    }
}
