use super::*;

fn rows(cells: &[&[&str]]) -> Vec<Vec<String>> {
    cells
        .iter()
        .map(|row| row.iter().map(|cell| cell.to_string()).collect())
        .collect()
}

#[test]
fn an_empty_query_shows_every_row() {
    let grid = rows(&[&["Jan", "Groningen"], &["Anja", "Utrecht"]]);
    assert_eq!(crate::commands::matching_rows(&grid, ""), None);
    assert_eq!(crate::commands::matching_rows(&grid, "   "), None);
}

#[test]
fn search_matches_any_cell_ignoring_case() {
    let grid = rows(&[
        &["Jan de Vries", "Groningen", "007"],
        &["Anja", "Utrecht", "012"],
        &["Émile", "ÜTRECHT", "3"],
        &["Piet", "Zwolle", "1007"],
    ]);
    let found = |query: &str| crate::commands::matching_rows(&grid, query).expect("filter");
    assert_eq!(found("utrecht"), [1]);
    assert_eq!(found("UTRECHT"), [1]);
    assert_eq!(found("trecht"), [1, 2]);
    assert_eq!(found("ütrecht"), [2]);
    assert_eq!(found("émile"), [2]);
    assert_eq!(found("007"), [0, 3]);
    assert_eq!(found("de v"), [0]);
    assert_eq!(found("  jan  "), [0]);
    assert!(found("Amsterdam").is_empty());
}

#[test]
fn contains_ignore_case_handles_ascii_and_other_text() {
    use crate::commands::contains_ignore_case as has;
    assert!(has("Groningen", "ning"));
    assert!(has("GRONINGEN", "ning"));
    assert!(!has("Gro", "groningen"));
    assert!(has("Straße", "straße"));
    assert!(has("ÅLESUND", "ålesund"));
    assert!(has("anything", ""));
    assert!(!has("", "a"));
}

#[test]
fn keep_lines_keeps_the_matching_rows_of_the_body() {
    let body = "a\tb\nc\td\ne\tf";
    assert_eq!(crate::commands::keep_lines(body, &[0, 2]), "a\tb\ne\tf");
    assert_eq!(crate::commands::keep_lines(body, &[1]), "c\td");
    assert_eq!(crate::commands::keep_lines(body, &[]), "");
}

#[test]
fn a_cell_menu_row_maps_back_to_the_table_row() {
    use crate::commands::table_row_shown;
    assert_eq!(table_row_shown(None, 4), Some(4));
    assert_eq!(table_row_shown(Some(&[3, 9]), 1), Some(9));
    assert_eq!(table_row_shown(Some(&[3, 9]), 2), None);
}

#[test]
fn the_meta_line_says_how_many_rows_match() {
    let meta = "64,659 rows × 8 columns  ·  PII  ·  financial";
    assert_eq!(
        crate::commands::filtered_meta(meta, 120, 64_659, 8),
        "120 of 64,659 rows × 8 columns  ·  PII  ·  financial"
    );
    assert_eq!(
        crate::commands::filtered_meta("1 row × 1 column", 0, 1, 1),
        "0 of 1 row × 1 column"
    );
    crate::locale::set_test_language(crate::settings::Language::Nl);
    let nl = crate::commands::filtered_meta("64.659 rijen × 8 kolommen  ·  PII", 120, 64_659, 8);
    let search = crate::locale::t("search_table").to_string();
    crate::locale::set_test_language(crate::settings::Language::En);
    assert_eq!(nl, "120 van 64.659 rijen × 8 kolommen  ·  PII");
    assert_ne!(search, crate::locale::t("search_table"));
}

#[test]
fn the_table_window_view_knows_its_row_count() {
    let src = "naam;plaats\nJan;Groningen\nAnja;Utrecht\n";
    let table = read(src, crate::dataframe::ReadOptions::default());
    let mut view = table_window_view(&table, src);
    assert_eq!(view.rows, 2);
    view.wipe();
    assert_eq!(view.rows, 0);
}

#[test]
fn the_chart_button_needs_a_grid_and_columns_that_fit() {
    let src = "shop;sales\nA;3\nB;4\n";
    let table = read(src, crate::dataframe::ReadOptions::default());
    let mut view = table_window_view(&table, src);
    assert_eq!(view.chart_columns.len(), 2);
    assert!(crate::commands::table_window_chart_on(&view));
    view.wipe();
    assert!(view.chart_columns.is_empty());
    assert!(!crate::commands::table_window_chart_on(&view));
}

#[test]
fn the_chart_button_follows_the_frame_not_the_format() {
    // Pasted TSV, a JSON table and semicolon CSV: one frame each, so a chart each.
    for src in [
        "shop\tsales\nA\t3\nB\t4\n",
        r#"[{"shop":"A","sales":3},{"shop":"B","sales":4}]"#,
        "shop;sales\nA;3\nB;4\n",
    ] {
        let table = read(src, crate::dataframe::ReadOptions::default());
        let view = table_window_view(&table, src);
        assert_eq!(view.chart_columns.len(), 2, "{src}");
        assert!(crate::commands::table_window_chart_on(&view), "{src}");
        let request = crate::chart_data::ChartRequest::first(&view.chart_columns).expect("chart");
        let frame = table.frame.as_ref().expect("frame");
        let data =
            crate::chart_data::build(frame, &request, crate::locale::Lang::En).expect("chart data");
        assert_eq!(data.skipped, 0, "{src}");
    }
}

#[test]
fn the_table_window_toolbar_ends_with_the_search_field() {
    use crate::commands::TableToolbarItem as Item;
    let items = crate::commands::table_window_toolbar_items();
    assert_eq!(items.last(), Some(&Item::Search));
    assert_eq!(
        items.iter().filter(|item| **item == Item::Search).count(),
        1
    );
    assert_eq!(
        &items[..6],
        &[
            Item::Table,
            Item::Chart,
            Item::Copy,
            Item::CopyJson,
            Item::CopyCsv,
            Item::Save
        ]
    );
}

#[test]
fn large_tables_search_on_a_thread_and_only_the_newest_result_counts() {
    use crate::commands::{SEARCH_ON_THREAD_CELLS, SearchGeneration, search_on_thread};
    assert!(!search_on_thread(2_000, 10));
    assert!(search_on_thread(2_001, 10));
    assert!(search_on_thread(SEARCH_ON_THREAD_CELLS + 1, 0));
    assert!(!search_on_thread(0, 50));
    let generation = SearchGeneration::default();
    let first = generation.start();
    let second = generation.start();
    // The first search ends last: its result is dropped.
    assert!(generation.current(second));
    assert!(!generation.current(first));
    // A quick search on the main thread makes the one on the thread stale too.
    generation.start();
    assert!(!generation.current(second));
}

#[test]
fn a_stale_search_stops_within_a_thousand_rows() {
    use crate::commands::{SEARCH_CHECK_ROWS, SearchGeneration, filter_sheet, matching_rows_while};
    use std::cell::Cell;
    let grid: Vec<Vec<String>> = (0..5_000).map(|i| vec![format!("rij {i}")]).collect();
    let body = grid
        .iter()
        .map(|row| row[0].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let generation = SearchGeneration::default();
    let mine = generation.start();
    // Still the newest: every row is looked at.
    let all = matching_rows_while(&grid, "rij", || generation.current(mine)).expect("all");
    assert_eq!(all.len(), 5_000);
    // A newer search (or a closed window) after the first block: it stops at the next look.
    let asked = Cell::new(0);
    let thread_side = generation.clone();
    let stopped = matching_rows_while(&grid, "rij", || {
        asked.set(asked.get() + 1);
        if asked.get() == 2 {
            generation.start();
        }
        thread_side.current(mine)
    });
    assert_eq!(stopped, None);
    assert_eq!(
        asked.get(),
        2,
        "looked again after {SEARCH_CHECK_ROWS} rows"
    );
    assert!(filter_sheet(&grid, &body, "rij", || false).is_none());
}

#[test]
fn filter_sheet_keeps_the_lines_of_the_matching_rows() {
    let grid = rows(&[
        &["Jan", "Groningen"],
        &["Anja", "Utrecht"],
        &["Piet", "Utrecht"],
    ]);
    let body = "Jan\tGroningen\nAnja\tUtrecht\nPiet\tUtrecht";
    let (found, kept) =
        crate::commands::filter_sheet(&grid, body, "utrecht", || true).expect("filter");
    assert_eq!(found, [1, 2]);
    assert_eq!(kept.as_str(), "Anja\tUtrecht\nPiet\tUtrecht");
    assert!(crate::commands::filter_sheet(&grid, body, " ", || true).is_none());
}
