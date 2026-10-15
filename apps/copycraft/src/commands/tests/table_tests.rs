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
    // Open table is the card chip only; Choose columns… is in the window menu; undo/redo
    // stay on ⌘Z and the version capsule.
    let ids: Vec<&CommandId> = view.menu.iter().map(|(c, _)| &c.id).collect();
    for not_in_menu in [
        CommandId::TableOpenWindow,
        CommandId::TableUndo,
        CommandId::TableRedo,
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
