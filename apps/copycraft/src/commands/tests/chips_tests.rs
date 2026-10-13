use super::*;

#[test]
fn remembered_chips_match_the_text() {
    let texts = [
        "{\"a\": [1, 2]}",
        "fn main() { let x = 1; }",
        "name,age\nalice,30\nbob,40",
        "a: 1\nb: [2, 3]\n",
        "aGVsbG8gd29ybGQ=",
        "https://example.com/page",
        "plain words",
    ];
    for text in texts {
        let input = data(SubjectKind::Text, Some(text));
        let first = ids(&crate::commands::chips(&input));
        assert_eq!(ids(&crate::commands::chips(&input)), first, "{text}");
        crate::commands::forget_chips();
        assert_eq!(ids(&crate::commands::chips(&input)), first, "{text}");
    }
    // Each text has its own chips, also when they are short and the same length.
    let json = ids(&crate::commands::chips(&data(
        SubjectKind::Text,
        Some("{\"a\":1}"),
    )));
    let rust = ids(&crate::commands::chips(&data(
        SubjectKind::Text,
        Some("fn a() {}"),
    )));
    assert!(json.contains(&CommandId::Schema));
    assert!(!rust.contains(&CommandId::Schema));
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
fn tabular_xml_shows_schema_without_convert() {
    let src = "<root><person><name>Jan</name><age>30</age></person><person><name>Anja</name><age>40</age></person></root>";
    let shown = chips(&data(SubjectKind::Text, Some(src)));
    assert_eq!(ids(&shown), vec![CommandId::Original, CommandId::Schema]);
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Dataframe));
}

#[test]
fn yaml_opens_formatted_and_coloured_with_to_json() {
    let src = "name:   copycraft\nitems:\n  - one\n";
    let input = data(SubjectKind::Text, Some(src));
    assert_eq!(crate::format::detect(src), crate::format::FormatKind::Yaml);
    let shown = chips(&input);
    assert_eq!(titles(&shown), vec!["Raw", "To JSON"]);
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
    let painted: String = crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Yaml)
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
            .all(|cmd| cmd.id != CommandId::Format)
    );
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
            shown.iter().any(|cmd| cmd.id == CommandId::TableOpenWindow),
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
fn code_is_formatted_without_original_or_format() {
    let src = "fn main() {\nlet message = \"this identifier is definitely longer than forty eight characters\";\n}\n";
    let input = data(SubjectKind::Text, Some(src));
    // Formatting indents it, so Raw is the way to the copy as it came; no Format chip.
    let ids: Vec<CommandId> = chips(&input).into_iter().map(|c| c.id).collect();
    assert_eq!(ids, [CommandId::Original]);
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
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

    let java =
        "public class Hi {\npublic static void main(String[] a) {\nSystem.out.println(1);\n}\n}\n";
    let input = data(SubjectKind::Text, Some(java));
    let ids: Vec<CommandId> = chips(&input).into_iter().map(|c| c.id).collect();
    assert_eq!(ids, [CommandId::Original]);
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
fn html_shows_no_schema_and_an_html_title() {
    for src in [
        "<!DOCTYPE html>\n<html><head><title>Hi</title></head><body><p>Hello</p></body></html>",
        "<html><body><p>Jan</p><p>Anja</p></body></html>",
    ] {
        let input = data(SubjectKind::Text, Some(src));
        let shown = chips(&input);
        assert!(
            !shown.iter().any(|cmd| cmd.id == CommandId::Schema),
            "{src}"
        );
        assert!(
            !shown.iter().any(|cmd| cmd.id == CommandId::Sample),
            "{src}"
        );
        assert!(
            !shown.iter().any(|cmd| cmd.id == CommandId::Convert),
            "{src}"
        );
        let card = work_card(&input);
        assert_eq!(card.title, "HTML", "{src}");
        assert_eq!(card.highlight, Some(crate::format::FormatKind::Html));
        assert_eq!(
            text_save_file(src, CardView::Format).unwrap().extension,
            "html"
        );
    }
    // Plain XML keeps its Schema chip.
    let xml = "<root><person><name>Jan</name></person><person><name>Anja</name></person></root>";
    assert!(
        chips(&data(SubjectKind::Text, Some(xml)))
            .iter()
            .any(|cmd| cmd.id == CommandId::Schema)
    );
}

#[test]
fn page_card_adds_format_when_the_url_needs_it() {
    let messy = "https://example.com/search?q=a/b";
    assert_eq!(
        titles(&chips(&data(SubjectKind::Text, Some(messy)))),
        vec!["Visit", "Format", "Raw", "URL"]
    );
}

#[test]
fn undo_and_redo_are_in_the_search_not_in_the_table_menu() {
    let mut table = deduped("name,n\na,1\na,1");
    let in_menu = |table: &TableShown| -> Vec<CommandId> {
        table_menu(Some(table)).into_iter().map(|c| c.id).collect()
    };
    assert!(!in_menu(&table).contains(&CommandId::TableUndo));
    table.version = 0;
    assert!(!in_menu(&table).contains(&CommandId::TableRedo));
    assert!(keeps_card_open(&CommandId::TableUndo));
    let mut input = data(SubjectKind::Text, Some("name,n\na,1\na,1"));
    assert!(
        !search_pool(&input)
            .iter()
            .any(|c| c.id == CommandId::TableRedo)
    );
    input.table = Some(table);
    assert!(
        search_pool(&input)
            .iter()
            .any(|c| c.id == CommandId::TableRedo)
    );
}

#[test]
fn a_table_gets_the_table_menu_chip_and_its_steps_in_search() {
    let input = data(SubjectKind::Text, Some("name,n\na,1\na,1"));
    let titles: Vec<String> = chips(&input).into_iter().map(|c| c.title).collect();
    assert!(titles.iter().any(|t| t == OPEN_TABLE_TITLE), "{titles:?}");
    assert!(
        search_pool(&input)
            .iter()
            .any(|c| c.id == CommandId::TableStep(crate::table::TableOp::Dedupe))
    );
    assert!(!keeps_card_open(&CommandId::TableOpenWindow));
    let prose = data(SubjectKind::Text, Some("just some words"));
    assert!(
        !chips(&prose)
            .iter()
            .any(|c| c.id == CommandId::TableOpenWindow)
    );
    // Before the table is read the menu has the one-click steps.
    let menu = table_menu(None);
    assert_eq!(menu.len(), crate::table::TableOp::ONE_CLICK.len());
    // No Dataframe chip: Open table opens the window instead of an in-card grid.
    assert!(!chips(&input).iter().any(|c| c.id == CommandId::Dataframe));
    assert!(
        chips(&input)
            .iter()
            .any(|c| c.id == CommandId::TableOpenWindow)
    );
}

#[test]
fn dates_in_either_order_ask_with_a_chip() {
    use crate::dataframe::ReadOptions;
    let src = "Export\nwhen,n\n01/02/2026,1\n03/04/2026,2";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    input.table = Some(read(src, ReadOptions::default()));
    let question = chips(&input)
        .into_iter()
        .find(|c| c.id == CommandId::TableDateOrder(true))
        .expect("question");
    assert_eq!(question.title, "Dates are mm/dd/yyyy");
    // The Original view does not ask.
    input.view = CardView::Original;
    assert!(
        !chips(&input)
            .iter()
            .any(|c| matches!(c.id, CommandId::TableDateOrder(_)))
    );
    // Read month first: the card renders the frame and says so; the chip asks back.
    let month_first = ReadOptions { month_first: true };
    input.view = CardView::Dataframe;
    let plain_key = content_key(&input);
    input.table = Some(read(src, month_first));
    let card = work_card(&input);
    assert!(card.meta.contains("Header on line 2"), "{}", card.meta);
    assert!(
        card.meta.contains("Dates read as mm/dd/yyyy"),
        "{}",
        card.meta
    );
    assert!(card.excerpt.contains("2026-01-02"), "{}", card.excerpt);
    assert!(!card.meta.contains("Original"), "{}", card.meta);
    assert_eq!(content_key(&input), plain_key);
    assert!(
        chips(&input)
            .iter()
            .any(|c| c.id == CommandId::TableDateOrder(false))
    );
}

#[test]
fn json_keeps_its_chips_and_the_diff_view() {
    let mut input = data(SubjectKind::Text, Some("{\"a\":1}"));
    input.diff = Some(diff_shown(crate::diff::Phase::Working));
    let titles: Vec<_> = chips(&input).iter().map(|cmd| cmd.title.clone()).collect();
    assert_eq!(
        titles[..2],
        [crate::locale::t("chip_raw"), crate::locale::t("chip_diff")]
    );
    assert!(
        titles
            .iter()
            .any(|title| title == crate::locale::t("chip_schema"))
    );
    input.view = CardView::Diff;
    assert_eq!(presented_view("{\"a\":1}", input.view), CardView::Diff);
}

#[test]
fn text_menu_on_prose_and_every_code_card_not_on_tables_or_links() {
    use crate::commands::offers_text_menu;
    for code in [
        "fn main() {\n    let a = 1;\n    println!(\"{a}\");\n}\n",
        // Commas in a call make this parse as a table too: still code.
        "def f(a, b):\n    return a + b\n",
        "{\"a\": 1, \"b\": [1, 2]}",
        "<a><b>1</b></a>",
        "# Title\n\n- one\n- two\n",
        "SELECT a, b FROM t WHERE c = 1;",
        "just a sentence of prose",
    ] {
        assert!(offers_text_menu(code), "{code}");
    }
    for other in [
        "name,age\nann,31\nbob,42\n",
        "name\tage\nann\t31\n",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    ] {
        assert!(!offers_text_menu(other), "{other}");
    }
}

#[test]
fn text_menu_chip_comes_first_on_a_code_card() {
    let rust = "fn main() {\n    let v = vec![1, 2];\n}\n";
    let reversed = crate::text_ops::reverse(rust);
    for text in [rust, reversed.as_str()] {
        let mut card = data(SubjectKind::Text, Some(text));
        card.text_edit = Some(crate::commands::TextShown {
            labels: vec!["Original".into()],
            version: 0,
            note: None,
        });
        let ids: Vec<CommandId> = chips(&card).into_iter().map(|cmd| cmd.id).collect();
        let at = ids.iter().position(|id| *id == CommandId::TextMenu);
        let after = ids
            .iter()
            .position(|id| *id == CommandId::Original)
            .map_or(0, |i| i + 1);
        assert_eq!(at, Some(after), "{ids:?}");
    }
}

#[test]
fn raw_shows_only_when_there_is_another_view() {
    let csv = "Naam,Bedrag\nJan,3\nPiet,4\n";
    let mut input = data(SubjectKind::Text, Some(csv));
    let titles: Vec<String> = chips(&input).iter().map(|c| c.title.to_string()).collect();
    assert!(
        !chips(&input).iter().any(|c| c.id == CommandId::Original),
        "{titles:?}"
    );
    // A table version shown on the card from search keeps the way back.
    input.view = CardView::Dataframe;
    assert_eq!(chips(&input)[0].id, CommandId::Original);
    let json = data(SubjectKind::Text, Some("{\"a\": 1}"));
    let raw = chips(&json)
        .into_iter()
        .find(|c| c.id == CommandId::Original)
        .expect("raw");
    assert_eq!(raw.title, crate::locale::t("chip_raw"));
    assert_eq!(raw.detail, crate::locale::t("detail_original"));
    assert_eq!(crate::locale::t("chip_raw"), "Raw");
    assert_eq!(
        crate::locale::t("detail_original"),
        "Show the text as copied"
    );
}

#[test]
fn raw_only_when_formatting_changes_the_copy() {
    let pretty = "fn main() {\n    let x = 1;\n}\n";
    let messy = "fn main() {\nlet x = 1;\n}\n";
    assert!(!crate::commands::differs_formatted(
        pretty,
        &format!("{pretty}  \n\n")
    ));
    assert!(crate::commands::differs_formatted(messy, pretty));
    for (src, raw) in [(pretty, false), (messy, true)] {
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = presented_view(src, CardView::Original);
        assert_eq!(input.view, CardView::Format);
        let has_raw = chips(&input).iter().any(|c| c.id == CommandId::Original);
        assert_eq!(has_raw, raw, "{src:?}");
        // The title says Formatted only when formatting changed something.
        let title = work_card(&input).title;
        assert_eq!(title.starts_with("Formatted"), raw, "{title}");
        assert!(title.contains("Rust"), "{title}");
    }
}

#[test]
fn raw_chosen_shows_the_copy_and_keeps_its_chip() {
    use crate::commands::shown_view;
    let messy = "fn main() {\nlet x = 1;\n}\n";
    // Without the hand-picked Raw, a code card opens formatted; with it, as copied.
    assert_eq!(
        shown_view(messy, CardView::Original, false),
        CardView::Format
    );
    assert_eq!(
        shown_view(messy, CardView::Original, true),
        CardView::Original
    );
    let mut input = data(SubjectKind::Text, Some(messy));
    input.view = CardView::Original;
    assert!(chips(&input).iter().any(|c| c.id == CommandId::Original));
}
