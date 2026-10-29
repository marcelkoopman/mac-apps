use super::*;

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
    assert_eq!(
        titles(&shown),
        vec![crate::locale::t("text_menu"), "To JSON"]
    );
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
    let painted: String = crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Yaml)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(painted, card.excerpt);
    let save = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
    assert_eq!(save.extension, "yaml");
    assert_eq!(save.bytes, pretty.as_bytes());
}

#[test]
fn markdown_opens_formatted_and_coloured_without_redact() {
    // Three addresses: in Markdown one or two are a support address, not personal data
    // ([`crate::recognize::DOC_EMAILS`]).
    let src = "# Notes\nCall jan@example.com\n- bring the report\n- cc piet@example.com, kees@example.com\n";
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
fn the_table_window_shows_the_version_with_its_meta_columns_versions_and_menu() {
    let src = "name,iban\nann,NL91ABNA0417164300\nann,NL91ABNA0417164300\nbob,NL20INGB0001234567";
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
    // Open table is the card chip only; Choose Columns… is in the window menu; undo/redo
    // stay on ⌘Z and the version capsule.
    let ids: Vec<&CommandId> = view.menu.iter().map(|(c, _)| &c.id).collect();
    for not_in_menu in [
        CommandId::TableOpenWindow,
        CommandId::StepBack,
        CommandId::StepForward,
    ] {
        assert!(!ids.contains(&&not_in_menu), "{not_in_menu:?}");
    }
    assert!(ids.contains(&&CommandId::TableChooseColumns));
    assert!(ids.contains(&&CommandId::TableStep(crate::table::TableOp::Dedupe)));
    assert!(!keeps_card_open(&CommandId::TableOpenWindow));
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
fn the_table_window_shows_every_row_of_a_normal_table() {
    // Well under WINDOW_ROWS: the dedicated window shows all rows (no "Showing N of M").
    let mut src = String::from("n,m\n");
    for i in 0..1_005 {
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
        !view.meta.contains("Showing "),
        "expected every row, got meta {}",
        view.meta
    );
    assert!(view.meta.contains("1,005 rows"), "{}", view.meta);
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
fn table_window_title_is_the_file_name_or_table() {
    use crate::commands::table_window_title;
    assert_eq!(table_window_title(None), "Table");
    assert_eq!(table_window_title(Some("  ")), "Table");
    assert_eq!(table_window_title(Some("sales.csv")), "sales.csv");
    let mut view = table_window_view(&deduped("a,b\n1,2"), "a,b\n1,2");
    assert_eq!(view.window_title(), "Table");
    view.source_name = Some("sales.csv".to_string());
    assert_eq!(view.window_title(), "sales.csv");
    view.wipe();
    assert_eq!(view.source_name, None);
}

#[test]
fn a_csv_copies_as_json_with_quotes_commas_and_every_separator() {
    use crate::dataframe::{TableCopy, table_copy};
    let expected = serde_json::json!([
        {"naam": "Jan, de Vries", "plaats": "Zei \"hoi\"", "code": "007"},
        {"naam": "Anja", "plaats": "Utrecht", "code": "012"}
    ]);
    for src in [
        "naam,plaats,code\n\"Jan, de Vries\",\"Zei \"\"hoi\"\"\",007\nAnja,Utrecht,012\n",
        "naam;plaats;code\n\"Jan, de Vries\";\"Zei \"\"hoi\"\"\";007\nAnja;Utrecht;012\n",
        "naam\tplaats\tcode\n\"Jan, de Vries\"\t\"Zei \"\"hoi\"\"\"\t007\nAnja\tUtrecht\t012\n",
    ] {
        let json = table_copy(src, TableCopy::Json).expect("json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parses");
        assert_eq!(value, expected, "{src}");
        // Keys in column order, pretty.
        assert!(json.find("\"naam\"") < json.find("\"code\""));
        assert!(json.contains('\n'));
    }
}

#[test]
fn a_json_array_of_objects_copies_as_csv_quoted_where_needed() {
    use crate::dataframe::{TableCopy, is_json_records, table_copy};
    let src = r#"[
  {"naam": "Jan, de Vries", "zin": "Zei \"hoi\"", "n": 3},
  {"naam": "Anja", "zin": "Klaar", "n": 4}
]"#;
    assert!(is_json_records(src));
    let csv = table_copy(src, TableCopy::Csv).expect("csv");
    assert_eq!(
        csv,
        "naam,zin,n\n\"Jan, de Vries\",\"Zei \"\"hoi\"\"\",3\nAnja,Klaar,4\n"
    );
    // And back: the CSV reads as the same table.
    let json = table_copy(&csv, TableCopy::Json).expect("json");
    let back: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(back[0]["naam"], "Jan, de Vries");
    assert_eq!(back[0]["zin"], "Zei \"hoi\"");
    assert_eq!(back[1]["n"], 4);
    // No array of objects, no table.
    for text in [r#"{"a": 1}"#, "[1, 2, 3]", "[]", "[ ]", "plain text"] {
        assert!(!is_json_records(text), "{text}");
    }
    assert!(table_copy("plain text", TableCopy::Csv).is_none());
    // A nested value has no CSV cell: nothing, and the app says so.
    let nested = r#"[{"a": {"b": 1}}, {"a": {"b": 2}}]"#;
    assert!(is_json_records(nested));
    assert!(table_copy(nested, TableCopy::Csv).is_none());
}

#[test]
fn csv_tsv_and_json_files_open_as_a_table() {
    use crate::commands::is_table_file_name;
    for name in ["rows.csv", "Rows.CSV", "rows.tsv", "rows.json"] {
        assert!(is_table_file_name(name), "{name}");
    }
    for name in ["rows.txt", "rows.xlsx", "csv", "rows.csv.png", "notes"] {
        assert!(!is_table_file_name(name), "{name}");
    }
}

#[test]
fn table_cards_offer_the_other_form_and_the_window_too() {
    use crate::dataframe::TableCopy;
    let copy = |copy| CommandId::TableCopyAs(copy);
    let csv = "naam;plaats\nJan;Groningen\nAnja;Utrecht\n";
    let shown = ids(&chips(&data(SubjectKind::Text, Some(csv))));
    assert!(shown.contains(&CommandId::TableOpenWindow));
    assert!(shown.contains(&copy(TableCopy::Json)));
    assert!(!shown.contains(&copy(TableCopy::Csv)));
    // A copied JSON array stays a JSON card; a dropped one is a table.
    let json = r#"[{"naam": "Jan"}, {"naam": "Anja"}]"#;
    let copied = ids(&chips(&data(SubjectKind::Text, Some(json))));
    assert!(!copied.contains(&CommandId::TableOpenWindow));
    assert!(!copied.contains(&copy(TableCopy::Csv)));
    let mut dropped = data(SubjectKind::Text, Some(json));
    dropped.source_name = Some("rows.json".into());
    let dropped = ids(&chips(&dropped));
    assert!(dropped.contains(&CommandId::TableOpenWindow));
    assert!(dropped.contains(&copy(TableCopy::Csv)));
    // The window's Table ▾ offers the other form of what it read.
    for (src, expected) in [(csv, TableCopy::Json), (json, TableCopy::Csv)] {
        let table = read(src, crate::dataframe::ReadOptions::default());
        let menu: Vec<CommandId> = crate::commands::table_window_menu(&table)
            .into_iter()
            .map(|(command, _)| command.id)
            .collect();
        assert_eq!(menu.last(), Some(&copy(expected)), "{src}");
    }
}

#[test]
fn the_copy_as_titles_read_differently_in_dutch_and_english() {
    let keys = [
        "table_copy_json",
        "table_copy_csv",
        "table_copied_json",
        "table_copied_csv",
        "table_copy_failed",
    ];
    let en: Vec<&str> = keys.iter().map(|key| crate::locale::t(key)).collect();
    crate::locale::set_test_language(crate::settings::Language::Nl);
    let nl: Vec<&str> = keys.iter().map(|key| crate::locale::t(key)).collect();
    crate::locale::set_test_language(crate::settings::Language::En);
    assert_eq!(nl[0], "Kopieer als JSON");
    assert_eq!(nl[1], "Kopieer als CSV");
    for (nl, en) in nl.iter().zip(&en) {
        assert_ne!(nl, en);
    }
}

#[test]
fn the_table_window_toolbar_has_copy_as_json_and_csv_buttons_like_table_menu() {
    use crate::dataframe::{ReadOptions, TableCopy};
    let csv = "naam;plaats\nJan;Groningen\nAnja;Utrecht\n";
    let json = r#"[{"naam": "Jan"}, {"naam": "Anja"}]"#;
    for src in [csv, json] {
        let table = read(src, ReadOptions::default());
        let view = table_window_view(&table, src);
        let buttons = crate::commands::table_window_copy_buttons(&view);
        let ids: Vec<&CommandId> = buttons.iter().map(|(command, _)| &command.id).collect();
        assert_eq!(
            ids,
            [
                &CommandId::TableCopyAs(TableCopy::Json),
                &CommandId::TableCopyAs(TableCopy::Csv)
            ],
            "{src}"
        );
        assert!(buttons.iter().all(|(_, on)| *on), "{src}");
        // The same commands as Table ▾'s item: same title, so the same action and toasts.
        let (menu_item, _) = view.menu.last().expect("menu");
        assert!(
            buttons.iter().any(|(button, _)| button == menu_item),
            "{src}"
        );
        // Off while the table is worked out, and once the window is wiped.
        let mut working = table.clone();
        working.frame = None;
        let waiting = table_window_view(&working, src);
        assert!(
            crate::commands::table_window_copy_buttons(&waiting)
                .iter()
                .all(|(_, on)| !on)
        );
        let mut wiped = view.clone();
        wiped.wipe();
        assert!(
            crate::commands::table_window_copy_buttons(&wiped)
                .iter()
                .all(|(_, on)| !on)
        );
    }
    // A nested JSON array has no CSV form: the button stays on and the app toasts
    // "This table doesn't fit that form" (`table_copy_failed`).
    let nested = r#"[{"a": {"b": 1}}, {"a": {"b": 2}}]"#;
    assert!(crate::dataframe::table_copy(nested, TableCopy::Csv).is_none());
    assert_ne!(
        crate::locale::t("table_copy_failed"),
        crate::locale::t("table_copied_csv")
    );
}
