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
    assert_eq!(ids(&shown), vec![CommandId::TextMenu, CommandId::Schema]);
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
    assert_eq!(ids(&shown), vec![CommandId::TextMenu, CommandId::Schema]);
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
    assert_eq!(ids(&shown), vec![CommandId::TextMenu, CommandId::Schema]);
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Dataframe));
}

#[test]
fn yaml_opens_formatted_and_coloured_with_to_json() {
    let src = "name:   copycraft\nitems:\n  - one\n";
    let input = data(SubjectKind::Text, Some(src));
    assert_eq!(crate::format::detect(src), crate::format::FormatKind::Yaml);
    let shown = chips(&input);
    assert_eq!(
        titles(&shown),
        vec![crate::locale::t("text_menu"), "To JSON"]
    );
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
    // Always the formatted view: no Raw and no Format chip. Text ▾ is the action.
    assert_eq!(ids(&chips(&input)), vec![CommandId::TextMenu]);
    assert_eq!(chips(&input)[0].title, crate::locale::t("text_menu"));
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
    assert_eq!(ids(&chips(&input)), vec![CommandId::TextMenu]);
    let card = work_card(&input);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Java));
    assert_eq!(card.title, "Formatted Java");
    assert!(card.excerpt.contains("    System.out.println"));
}

#[test]
fn pretty_code_stays_on_the_card_without_original() {
    let src = "fn main() {\n    let x = 1;\n}\n";
    let input = data(SubjectKind::Text, Some(src));
    assert_eq!(ids(&chips(&input)), vec![CommandId::TextMenu]);
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
        vec!["Visit", "Format", "URL"]
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
    assert_eq!(titles[..1], [crate::locale::t("chip_diff")]);
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
fn a_rust_snippet_and_an_opened_file_offer_lines() {
    let rust = "fn main() {\n    let x = 1;\n}\n";
    let mut card = data(SubjectKind::Text, Some(rust));
    assert_eq!(ids(&chips(&card)), vec![CommandId::TextMenu]);
    assert_eq!(chips(&card)[0].title, crate::locale::t("text_menu"));
    // The menu is there before any version is attached, so the chip is not an empty button.
    assert!(
        search_pool(&card)
            .iter()
            .any(|cmd| cmd.id == CommandId::TextHead)
    );
    card.source_name = Some("main.rs".into());
    card.view = presented_view(rust, CardView::Original);
    assert!(chips(&card).iter().any(|cmd| cmd.id == CommandId::TextMenu));
    assert!(
        !chips(&card)
            .iter()
            .any(|cmd| matches!(cmd.id, CommandId::Format | CommandId::Original))
    );
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
            code: true,
            agent: false,
            tracking: false,
            json: false,
        });
        let ids: Vec<CommandId> = chips(&card).into_iter().map(|cmd| cmd.id).collect();
        let at = ids.iter().position(|id| *id == CommandId::TextMenu);
        assert_eq!(at, Some(0), "{ids:?}");
    }
}

#[test]
fn a_toml_card_is_titled_and_colored() {
    let src = "[package]\nname = \"copycraft\"\nversion = \"0.4.1\"\n";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = presented_view(src, CardView::Original);
    let card = work_card(&input);
    assert_eq!(card.title, "TOML");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Toml));
}

#[test]
fn text_cards_have_no_raw_chip_and_title_formatted_only_when_changed() {
    let pretty = "fn main() {\n    let x = 1;\n}\n";
    let messy = "fn main() {\nlet x = 1;\n}\n";
    assert!(!crate::commands::differs_formatted(
        pretty,
        &format!("{pretty}  \n\n")
    ));
    assert!(crate::commands::differs_formatted(messy, pretty));
    for (src, formatted) in [(pretty, false), (messy, true)] {
        let mut input = data(SubjectKind::Text, Some(src));
        input.view = presented_view(src, CardView::Original);
        assert_eq!(input.view, CardView::Format);
        assert!(
            !chips(&input)
                .iter()
                .any(|c| c.title == crate::locale::t("original"))
        );
        let title = work_card(&input).title;
        assert_eq!(title.starts_with("Formatted"), formatted, "{title}");
    }
    for src in [
        "{\"a\": 1}",
        "Naam,Bedrag\nJan,3\nPiet,4\n",
        "<a><b>1</b></a>",
    ] {
        let input = data(SubjectKind::Text, Some(src));
        assert!(
            !chips(&input)
                .iter()
                .any(|c| c.title == crate::locale::t("original")),
            "{src}"
        );
    }
}

#[test]
fn clean_url_shows_only_when_a_link_holds_tracking_parameters() {
    use crate::commands::{Command, text_menu_items, text_shown};
    let has = |commands: &[Command]| commands.iter().any(|c| c.id == CommandId::TextCleanUrl);
    // A link card: a chip, as a link has no Text ▾.
    let tracked = data(
        SubjectKind::Text,
        Some("https://x.nl/p?utm_source=x&id=5#top"),
    );
    assert!(has(&chips(&tracked)));
    let clean = data(SubjectKind::Text, Some("https://x.nl/p?id=5#top"));
    assert!(!has(&chips(&clean)));
    // Prose with a link: in Text ▾, not as a chip.
    let prose = data(
        SubjectKind::Text,
        Some("Lees dit: https://x.nl/p?fbclid=1 en reageer.\n"),
    );
    assert!(!has(&chips(&prose)));
    let edit = text_shown(&prose).expect("Text ▾");
    assert!(edit.tracking);
    let menu: Vec<Command> = text_menu_items(&edit).into_iter().map(|(c, _)| c).collect();
    assert!(has(&menu));
    let plain = data(SubjectKind::Text, Some("Lees dit: https://x.nl/p?id=1.\n"));
    let edit = text_shown(&plain).expect("Text ▾");
    assert!(!edit.tracking);
    let menu: Vec<Command> = text_menu_items(&edit).into_iter().map(|(c, _)| c).collect();
    assert!(!has(&menu));
}

#[test]
fn format_json_is_in_lines_on_a_json_card_only() {
    use crate::commands::{text_menu_items, text_shown};
    let has = |text: &str| {
        let card = data(SubjectKind::Text, Some(text));
        text_shown(&card).is_some_and(|edit| {
            text_menu_items(&edit)
                .iter()
                .any(|(c, _)| c.id == CommandId::TextJson)
        })
    };
    assert!(has(r#"{"b":1,"a":[1,2]}"#));
    assert!(has("[\n  1,\n  2\n]\n"));
    // JSON with a mistake: the dialog says where.
    assert!(has("{\n  \"a\": 1,\n}\n"));
    assert!(!has("plain words\non two lines\n"));
    assert!(!has("fn main() { let x = 1; }\n"));
}

#[test]
fn case_style_is_a_submenu_of_lines() {
    use crate::commands::{Command, menu_group, text_menu_items, text_shown};
    use crate::text_ops::case_style::CaseStyle;
    let card = data(SubjectKind::Text, Some("HTTPServer\nuser_id\n"));
    let menu = text_menu_items(&text_shown(&card).expect("Text ▾"));
    let cases: Vec<(usize, &Command)> = menu
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| matches!(c.id, CommandId::TextCase(_)))
        .map(|(at, (c, _))| (at, c))
        .collect();
    assert_eq!(cases.len(), CaseStyle::ALL.len());
    // Side by side, so they make one submenu.
    assert!(cases.windows(2).all(|pair| pair[1].0 == pair[0].0 + 1));
    for (_, command) in &cases {
        assert_eq!(menu_group(&command.id), Some(crate::locale::t("case_menu")));
    }
    let titles: Vec<&str> = cases.iter().map(|(_, c)| c.title.as_str()).collect();
    assert_eq!(
        titles[..5],
        [
            "camelCase",
            "PascalCase",
            "snake_case",
            "kebab-case",
            "CONSTANT_CASE"
        ]
    );
    // The step itself: every line, the empty one kept.
    let step = crate::text_ops::TextOp::Case(CaseStyle::Camel);
    assert_eq!(
        step.apply("HTTPServer\n\nuser_id\n").as_deref(),
        Some("httpServer\n\nuserId\n")
    );
}

#[test]
fn encoding_is_a_submenu_of_convert() {
    use crate::commands::{TextSection, menu_group, text_menu_items, text_section, text_shown};
    use crate::text_ops::encoding::Codec;
    let card = data(SubjectKind::Text, Some("foobar\n"));
    let menu = text_menu_items(&text_shown(&card).expect("Text ▾"));
    let at: Vec<usize> = menu
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| matches!(c.id, CommandId::TextEncode(_)))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(at.len(), Codec::ALL.len());
    assert!(at.windows(2).all(|pair| pair[1] == pair[0] + 1));
    for index in at {
        let id = &menu[index].0.id;
        assert_eq!(menu_group(id), Some(crate::locale::t("encoding_menu")));
        assert_eq!(text_section(id), Some(TextSection::Convert));
    }
    let step = crate::text_ops::TextOp::Encode(Codec::Base64Encode);
    assert_eq!(step.apply("foobar").as_deref(), Some("Zm9vYmFy"));
    // A bad decode comes back as a reason, without the text.
    match crate::text_ops::TextResult::work(
        "abc$",
        &crate::text_ops::TextOp::Encode(Codec::Base64Decode),
    ) {
        crate::text_ops::TextResult::Invalid { codec, reason } => {
            assert_eq!(codec, Codec::Base64Decode);
            assert_eq!(reason, crate::text_ops::encoding::Invalid::NotEncoded);
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn hashes_follow_the_codecs_in_the_encoding_submenu() {
    use crate::commands::{TextSection, menu_group, text_menu_items, text_section, text_shown};
    use crate::text_ops::hash::HashKind;
    let card = data(SubjectKind::Text, Some("abc"));
    let menu = text_menu_items(&text_shown(&card).expect("Text ▾"));
    let last_codec = menu
        .iter()
        .rposition(|(c, _)| matches!(c.id, CommandId::TextEncode(_)))
        .expect("codecs");
    let hashes: Vec<CommandId> = menu[last_codec + 1..]
        .iter()
        .take(HashKind::ALL.len())
        .map(|(c, _)| c.id.clone())
        .collect();
    let expected: Vec<CommandId> = HashKind::ALL.into_iter().map(CommandId::TextHash).collect();
    assert_eq!(hashes, expected);
    for id in &hashes {
        assert_eq!(menu_group(id), Some(crate::locale::t("encoding_menu")));
        assert_eq!(text_section(id), Some(TextSection::Convert));
    }
    let step = crate::text_ops::TextOp::Hash(HashKind::Md5);
    assert_eq!(
        step.apply("abc").as_deref(),
        Some("900150983cd24fb0d6963f7d28e17f72")
    );
}

#[test]
fn the_text_menu_is_split_into_lines_convert_and_analysis() {
    use crate::commands::{TextSection, text_menu_items, text_sections, text_shown};
    let card = data(SubjectKind::Text, Some("{\"a\": 1}\n"));
    let menu = text_menu_items(&text_shown(&card).expect("Text ▾"));
    let ids: Vec<CommandId> = menu.iter().map(|(cmd, _)| cmd.id.clone()).collect();
    let sections = text_sections(&ids).expect("the Text ▾ menu has sections");
    assert_eq!(sections.len(), ids.len());
    // Every item has a section, and each section is one run: Lines first, then Convert.
    assert!(sections.iter().all(Option::is_some));
    let mut runs: Vec<TextSection> = Vec::new();
    for section in sections.into_iter().flatten() {
        if runs.last() != Some(&section) {
            assert!(!runs.contains(&section), "{section:?} comes back");
            runs.push(section);
        }
    }
    assert_eq!(runs[0], TextSection::Lines);
    assert!(runs.contains(&TextSection::Convert));
    assert_eq!(
        TextSection::Lines.title(),
        Some(crate::locale::t("text_section_lines"))
    );
    assert_eq!(TextSection::Versions.title(), None);
    // The table menu keeps no headers.
    assert!(text_sections(&[CommandId::TableUndo]).is_none());
    assert!(text_sections(&[]).is_none());
}
