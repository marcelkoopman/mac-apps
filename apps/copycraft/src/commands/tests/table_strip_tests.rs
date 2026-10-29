use super::*;

const SRC: &str = "naam;plaats\nJan;Groningen\nJan;Groningen\nAnja;Utrecht\n";

#[test]
fn the_version_strip_shows_with_two_or_more_versions_like_the_history_strip() {
    let one = table_window_view(&read(SRC, crate::dataframe::ReadOptions::default()), SRC);
    assert!(!crate::commands::table_strip_shown(&one));
    assert!(!crate::commands::strip_shown(1, false));
    let two = table_window_view(&deduped(SRC), SRC);
    assert!(crate::commands::table_strip_shown(&two));
}

#[test]
fn a_version_card_reads_as_a_history_card_of_the_copy() {
    let mut view = table_window_view(&deduped(SRC), SRC);
    view.set_kind(crate::format::FormatKind::Csv);
    let mut sizes = crate::commands::VersionSizes::default();
    sizes.note(&view);
    let tiles = crate::commands::table_version_tiles(&view, &sizes, None);
    assert_eq!(tiles.len(), 2);
    // The copy's type mark on every card, as on its history card; no "v1" or "Original".
    for tile in &tiles {
        assert_eq!(tile.mark, "csv");
        assert!(!tile.mark.starts_with('v'));
        assert!(!tile.lines.contains("Original") && !tile.lines.contains("Origineel"));
    }
    assert!(!tiles[0].current && tiles[1].current);
    // The original: the copied text's first lines, exactly as the history card shows them.
    assert_eq!(tiles[0].lines, crate::commands::thumb_lines(SRC));
    assert_eq!(tiles[0].lines, "naam;plaats\nJan;Groningen\nJan;Groningen");
    // The version shown: its own first lines (written as CSV).
    assert_eq!(tiles[1].lines, view.lines);
    assert!(tiles[1].lines.starts_with("naam"), "{}", tiles[1].lines);
    assert!(tiles[1].lines.contains("Anja"));
    assert!(tiles[1].lines.lines().count() <= crate::commands::THUMB_LINES);
    // VoiceOver: the version's number and step; masked, the number only.
    assert_eq!(
        tiles[1].spoken,
        format!("Version 2 of 2, {}", view.versions.labels[1])
    );
    assert_eq!(tiles[1].spoken_masked, "Version 2 of 2");
    // The tooltip: the type, the step and the size (a history card's type and age).
    assert_eq!(tiles[0].tooltip, "CSV");
    assert_eq!(
        tiles[1].tooltip,
        format!("CSV · {} · 2 rows × 2 columns", view.versions.labels[1])
    );
    assert_eq!(
        crate::commands::version_tile_click(0),
        CommandId::TableVersion(0)
    );
    // No cell values in what VoiceOver or the tooltip say.
    for tile in &tiles {
        for said in [&tile.spoken, &tile.spoken_masked, &tile.tooltip] {
            assert!(!said.contains("Groningen") && !said.contains("Anja"));
        }
    }
}

#[test]
fn a_version_not_shown_yet_has_its_step_until_it_was_seen() {
    let mut view = table_window_view(&deduped(SRC), SRC);
    let shown_lines = view.lines.clone();
    // The step was made before the window opened: back on the original, never seen.
    view.versions.current = 0;
    view.lines.clear();
    let mut sizes = crate::commands::VersionSizes::default();
    sizes.note(&view);
    let tiles = crate::commands::table_version_tiles(&view, &sizes, None);
    assert_eq!(tiles[1].lines, view.versions.labels[1]);
    // Shown once: its lines stay for the card.
    view.versions.current = 1;
    view.lines = shown_lines.clone();
    sizes.note(&view);
    view.versions.current = 0;
    view.lines.clear();
    sizes.note(&view);
    let tiles = crate::commands::table_version_tiles(&view, &sizes, None);
    assert_eq!(tiles[1].lines, shown_lines);
    // Another table: forgotten.
    let mut other = view.clone();
    other.table_key ^= 2;
    sizes.note(&other);
    let tiles = crate::commands::table_version_tiles(&other, &sizes, None);
    assert_eq!(tiles[1].lines, view.versions.labels[1]);
}

#[test]
fn version_sizes_are_kept_per_table_and_while_the_steps_are_the_same() {
    let mut first = table_window_view(&deduped(SRC), SRC);
    let mut sizes = crate::commands::VersionSizes::default();
    sizes.note(&first);
    // Back on the original (3 rows): both sizes known.
    first.versions.current = 0;
    first.rows = 3;
    sizes.note(&first);
    assert_eq!(sizes.sizes(), &[Some((3, 2)), Some((2, 2))]);
    // Another last step: the original's size stays, the old step's goes.
    let mut other = first.clone();
    other.versions.labels[1] = "Sorted".to_string();
    other.grid.clear();
    sizes.note(&other);
    assert_eq!(sizes.sizes(), &[Some((3, 2)), None]);
    // Another table: nothing carried over.
    let mut another = first.clone();
    another.table_key ^= 2;
    another.grid.clear();
    sizes.note(&another);
    assert_eq!(sizes.sizes(), &[None, None]);
}

#[test]
fn the_size_reads_as_on_the_meta_line_in_both_languages() {
    assert_eq!(
        crate::commands::table_size_text(64_659, 8),
        "64,659 rows × 8 columns"
    );
    assert_eq!(crate::commands::table_size_text(1, 1), "1 row × 1 column");
    crate::locale::set_test_language(crate::settings::Language::Nl);
    let nl = crate::commands::table_size_text(64_659, 8);
    crate::locale::set_test_language(crate::settings::Language::En);
    assert_eq!(nl, "64.659 rijen × 8 kolommen");
}

#[test]
fn the_toolbar_has_no_version_group_any_more() {
    use crate::commands::TableToolbarItem as Item;
    let items = crate::commands::table_window_toolbar_items();
    assert_eq!(
        items,
        [
            Item::Table,
            Item::Copy,
            Item::CopyJson,
            Item::CopyCsv,
            Item::Save,
            Item::Space,
            Item::Hint,
            Item::Space,
            Item::Search
        ]
    );
}

#[test]
fn the_card_asked_for_is_framed_before_its_version_arrives() {
    let view = table_window_view(&deduped(SRC), SRC);
    let sizes = crate::commands::VersionSizes::default();
    let shown = crate::commands::table_version_tiles(&view, &sizes, None);
    assert!(!shown[0].current && shown[1].current);
    let mut switch = crate::commands::VersionSwitch::default();
    switch.begin(0);
    let framed = crate::commands::table_version_tiles(&view, &sizes, switch.target());
    assert!(framed[0].current && !framed[1].current);
}
