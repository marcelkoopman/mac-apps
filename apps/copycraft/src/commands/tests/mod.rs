use super::picker_columns;
use super::{ArrowAction, ArrowFocus, arrow_action};
use super::{CHIP_GAP, CHIP_PILL_H, NAV_BUTTON, NAV_COUNT_W, NEXT_CHEVRON, PREVIOUS_CHEVRON};
use super::{
    CardView, CommandId, ContentActions, DiffShown, Hist, ImageFacts, ImageScan, LaunchData,
    NAV_SPAN, SubjectKind, chip_width, chips, content_actions, content_key, copy_tip,
    deferred_save_name, effective_mask, history_label, history_nav, keeps_card_open, keyed_tip,
    layout_chips, masks_content, matching, overflow, presented_view, save_tip, search_pool,
    stays_revealed, step_chip, step_history, text_save_file, transformed_text, well_mask,
    work_card,
};
use super::{OPEN_TABLE_TITLE, table_window_view};
use super::{PREVIEW_CHARS, PREVIEW_ROWS, excerpt_for, group_thousands, showing_note};
use super::{TableShown, VersionBar, image_undo_key, menu_group, table_menu, undo_key};
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
        can_screenshot: true,
        notice: None,
        history_nav: None,
        strip: Vec::new(),
        strip_cursor: 0,
        can_restore_history: false,
        strip_hint: crate::commands::StripHint::None,
        theme: Theme::System,
        settings: crate::settings::Settings::default(),
        view: CardView::Original,
        image_scan: None,
        pdf: None,
        zip: None,
        pdf_page: 0,
        pdf_zoom: crate::pdf::ZOOM_FIT,
        pdf_zoom_picture: None,
        document_preview: false,
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
        back_to_source: false,
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

/// A picture card with Image ▾: `label` names the step that made this copy, `undo` and `redo`
/// whether ⌘Z and ⇧⌘Z have a copy to go to.
fn edited_picture(label: Option<&str>, undo: bool, redo: bool) -> LaunchData {
    let mut input = data(SubjectKind::Image, None);
    input.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 1024,
        height: 768,
        byte_len: 184_000,
    });
    input.image_edit = Some(super::ImageShown {
        entry: 7,
        label: label.map(str::to_string),
        can_undo: undo,
        can_redo: redo,
        working: false,
        note: None,
        dropped: false,
        location: false,
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
        snippet: None,
        kind: crate::format::FormatKind::Plain,
    }
}

mod actions_tests;
mod card_table_tests;
mod card_tests;
mod chips_tests;
mod history_tests;
mod kind_filter_tests;
mod layout_tests;
mod menu_tests;
mod meta_tests;
mod panel_tests;

mod file_opening_tests;
mod table_search_tests;
mod table_strip_tests;
mod table_switch_tests;
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
fn line_numbers_go_in_front_of_every_line_of_the_body() {
    assert_eq!(super::numbered_line_starts("een\ntwee"), vec![0, 4]);
    // A closing newline starts no line; a cut preview's "…" is no line either.
    assert_eq!(super::numbered_line_starts("a\nb\n"), vec![0, 2]);
    assert_eq!(super::numbered_line_starts("a\nb\n…"), vec![0, 2]);
    assert_eq!(super::numbered_line_starts(""), vec![0]);
    // UTF-16 units, not bytes.
    assert_eq!(super::numbered_line_starts("é😀\nx"), vec![0, 4]);
    assert_eq!(super::gutter_label(1, 10), " 1  ");
    assert_eq!(super::gutter_label(1, 9), "1  ");
    assert_eq!(super::gutter_label(10, 10), "10  ");
    assert_eq!(super::gutter_width(10), 4);
}

#[test]
fn offsets_of_the_body_move_past_the_gutters() {
    let body = "ab\ncd\nef";
    // 3 lines: gutter "1  " is 3 wide.
    assert_eq!(super::gutter_range(body, 0, 0), (3, 0));
    assert_eq!(super::gutter_range(body, 1, 1), (4, 1));
    // "cd" starts the second line: after two gutters.
    assert_eq!(super::gutter_range(body, 3, 2), (9, 2));
    // "b\nc" spans a line break: it takes the second gutter in.
    assert_eq!(super::gutter_range(body, 1, 3), (4, 6));
    // A match ending at a line break stays clear of the next gutter.
    assert_eq!(super::gutter_range(body, 0, 2), (3, 2));
}

#[test]
fn every_text_or_code_card_offers_line_numbers() {
    let samples = [
        "Eerste regel.\nTweede regel.",
        "Dit is een gewone Nederlandse zin die Marcel kopieerde om te tellen.",
        "fn main() {\n    let x = 1;\n}\n",
        "public class A {\n  void f() {}\n}\n",
        "def f(x):\n    return x\n",
        "{\"a\": 1,\n \"b\": [1, 2]}",
        "naam: a\nlijst:\n  - 1\n  - 2\n",
        "# Titel\n\nTekst met *nadruk*.\n\n- een\n- twee\n",
    ];
    for text in samples {
        let data = data(SubjectKind::Text, Some(text));
        let card = work_card(&data);
        assert!(
            super::offers_line_numbers(&data, &card),
            "no numbers button for {:?}",
            crate::format::detect(text)
        );
    }
    // Tables are no text lines.
    let csv = data(SubjectKind::Text, Some("a,b,c\n1,2,3\n4,5,6\n"));
    assert!(!super::offers_line_numbers(&csv, &work_card(&csv)));
    // Nothing shown, nothing to number.
    let empty = data(SubjectKind::Empty, None);
    assert!(!super::offers_line_numbers(&empty, &work_card(&empty)));
}

/// The choice is global, so no test flips it: it would race with the other card tests.
#[test]
fn line_numbers_are_off_by_default_and_the_button_offers_them() {
    let sentence = "Eerste regel.\nTweede regel.";
    let plain = data(SubjectKind::Text, Some(sentence));
    assert!(super::offers_line_numbers(&plain, &work_card(&plain)));
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
fn a_slow_text_card_keeps_the_shown_text_instead_of_an_empty_well() {
    use super::keeps_shown_card;
    // To JSON or a Text ▾ result built slowly: the YAML card stays until the JSON is in.
    assert!(keeps_shown_card(true, true, true));
    // A real card always replaces what is shown.
    assert!(!keeps_shown_card(false, true, true));
    // Nothing (or a picture, a link page) shown, or a picture next: the placeholder shows.
    assert!(!keeps_shown_card(true, false, true));
    assert!(!keeps_shown_card(true, true, false));
}

#[test]
fn the_numbers_button_keeps_its_place_while_its_card_is_rebuilt() {
    use super::{building_card, dock_offers_numbers, is_building_card};
    // A slow YAML card: the numbers button rebuilds it, the "Working…" card comes first.
    let yaml = data(SubjectKind::Text, Some("- a: 1\n- b: 2\n- c: 3"));
    let real = work_card(&yaml);
    let placeholder = building_card();
    assert!(is_building_card(&placeholder));
    assert!(super::offers_line_numbers(&yaml, &real));
    assert!(!super::offers_line_numbers(&yaml, &placeholder));
    // Same entry: the button stays, so the eye button below it does not move up.
    assert!(dock_offers_numbers(
        super::offers_line_numbers(&yaml, &placeholder),
        true,
        true,
        true
    ));
    // The card arrives: as offered.
    assert!(dock_offers_numbers(true, false, true, true));
    // Another entry, or no button before: the placeholder has none.
    assert!(!dock_offers_numbers(false, true, false, true));
    assert!(!dock_offers_numbers(false, true, true, false));
    // A real card without numbers (a table) hides it.
    assert!(!dock_offers_numbers(false, false, true, true));
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
    assert!(super::offers_line_numbers(&card, &work_card(&card)));
}
