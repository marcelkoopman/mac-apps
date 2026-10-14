use super::picker_columns;
use super::{ArrowAction, ArrowFocus, arrow_action};
use super::{CHIP_GAP, CHIP_PILL_H, NAV_BUTTON, NAV_COUNT_W, NEXT_CHEVRON, PREVIOUS_CHEVRON};
use super::{
    CardView, CommandId, ContentActions, DiffShown, Hist, ImageFacts, ImageScan, LaunchData,
    NAV_RESERVE, NAV_SPAN, SubjectKind, chip_width, chips, content_actions, content_key, copy_tip,
    deferred_save_name, effective_mask, history_label, history_nav, keeps_card_open, layout_chips,
    masks_content, matching, overflow, payload_excerpt, presented_view, save_tip, search_pool,
    stays_revealed, step_chip, step_history, text_save_file, transformed_text, well_mask,
    work_card,
};
use super::{OPEN_TABLE_TITLE, table_window_view};
use super::{PREVIEW_CHARS, PREVIEW_ROWS, excerpt_for, group_thousands, showing_note};
use super::{TableShown, VersionBar, menu_group, table_menu, undo_key};
use crate::appearance::Theme;
use mac_ui::keys::Key;
use zeroize::Zeroizing;

pub(crate) fn data(kind: SubjectKind, text: Option<&str>) -> LaunchData {
    LaunchData {
        subject_kind: kind,
        subject_text: text.map(str::to_string),
        image: None,
        history: Vec::new(),
        can_clear_history: false,
        can_clear_pins: false,
        paused: false,
        ignore_next: false,
        can_ignore_app: false,
        notice: None,
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
        text_edit: None,
        shown_id: None,
        shown_mark: None,
        diff_picks: Vec::new(),
        diff: None,
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

fn csv_rows(rows: usize) -> String {
    let mut csv = String::from("id,name,city\n");
    for i in 0..rows {
        csv.push_str(&format!("{i},name{i},City {}\n", i % 7));
    }
    csv
}

fn diff_shown(phase: crate::diff::Phase) -> DiffShown {
    DiffShown {
        token: 7,
        sensitive: false,
        labels: crate::sensitivity::Labeling::Known(crate::sensitivity::Found::default()),
        phase,
    }
}

fn history_item(index: usize, title: &str, text: bool) -> Hist {
    Hist {
        index,
        title: title.to_string(),
        mark: "¶".into(),
        id: index as u64,
        text,
        search: Zeroizing::new(String::new()),
        pinned: false,
    }
}

mod actions_tests;
mod card_table_tests;
mod card_tests;
mod chips_tests;
mod history_tests;
mod layout_tests;
mod menu_tests;
mod meta_tests;

mod table_tests;
mod text_tests;

#[test]
fn a_notice_shows_once_on_the_card_and_is_not_kept() {
    let sentence = "Dit is een gewone Nederlandse zin.";
    let mut data = data(SubjectKind::Text, Some(sentence));
    assert!(super::card_may_be_kept(&data));
    let note = crate::locale::t("text_nothing").to_string();
    data.notice = Some(note.clone());
    // The card built with the note shows it once, and is not kept: a kept card from before
    // the note would come back instead and the note would never show.
    assert_eq!(work_card(&data).meta.matches(note.as_str()).count(), 1);
    assert!(!super::card_may_be_kept(&data));
}

#[test]
fn plain_text_cards_number_their_lines() {
    assert_eq!(super::numbered_lines("een\ntwee"), "1  een\n2  twee");
    let ten: String = (1..=10).map(|n| format!("r{n}\n")).collect();
    let numbered = super::numbered_lines(ten.trim_end());
    assert!(numbered.starts_with(" 1  r1\n 2  r2"));
    assert!(numbered.ends_with("10  r10"));
    // A cut preview keeps its unnumbered "…".
    assert_eq!(super::numbered_lines("a\nb\n…"), "1  a\n2  b\n…");
}

/// The choice is global, so no test flips it: it would race with the other card tests.
#[test]
fn line_numbers_are_off_by_default_and_the_button_offers_them() {
    let sentence = "Eerste regel.\nTweede regel.";
    let plain = data(SubjectKind::Text, Some(sentence));
    assert!(super::offers_line_numbers(&plain));
    let code = data(SubjectKind::Text, Some("fn main() {\n    let x = 1;\n}\n"));
    assert!(!super::offers_line_numbers(&code));
    // Off: the well holds the text as copied, so search finds no numbers.
    assert!(!super::line_numbers_on());
    assert_eq!(work_card(&plain).excerpt, sentence);
    assert_eq!(
        super::line_numbers_button_look(false),
        ("list.number", crate::locale::t("show_line_numbers"))
    );
    assert_eq!(
        super::line_numbers_button_look(true),
        ("list.number", crate::locale::t("hide_line_numbers"))
    );
}

#[test]
fn a_single_sentence_gets_the_numbers_button_and_room_for_every_well_button() {
    // A one-line Dutch sentence is `Plain` ("Inhoud"), not `Text`: it gets the button too.
    let sentence = "Dit is een gewone Nederlandse zin die Marcel kopieerde om te tellen.";
    assert_eq!(
        crate::format::detect(sentence),
        crate::format::FormatKind::Plain
    );
    let card = data(SubjectKind::Text, Some(sentence));
    assert!(super::offers_line_numbers(&card));
}
