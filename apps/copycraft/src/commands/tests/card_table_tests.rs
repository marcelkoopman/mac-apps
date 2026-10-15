//! Card tests for tables, dataframe views and the later card kinds.

use super::*;

#[test]
fn a_table_below_a_definition_line_opens_as_a_table_with_a_header_note() {
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Csv));
    assert!(card.meta.contains("  ·  Header on line 2"), "{}", card.meta);
    // The definition line stays above the aligned table, as copied.
    let first = card.excerpt.lines().next().unwrap_or_default();
    assert!(
        first
            .trim_start_matches('\u{feff}')
            .starts_with("Definition:")
    );
    let second = card.excerpt.lines().nth(1).unwrap_or_default();
    assert!(second.starts_with("Date"), "{:?}", second.get(..12));
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    // 20 columns: Table shows the grid directly (shortened names); Copy keeps the real ones.
    let table = read(src, crate::dataframe::ReadOptions::default());
    input.table = Some(table);
    let grid = work_card(&input);
    assert_eq!(grid.title, "Table");
    assert!(!grid.excerpt.contains("shape:"), "{}", grid.excerpt);
    assert!(grid.excerpt.contains("┌"), "{}", grid.excerpt);
    assert!(grid.excerpt.contains("┆ … PV3"), "{}", grid.excerpt);
    assert!(!grid.excerpt.contains("Sunbox"), "{}", grid.excerpt);
    assert!(grid.meta.contains("Header on line 2"), "{}", grid.meta);
    assert!(grid.preview_note.is_none());
    // No Show columns / Show table chip — only Original and Table ▾.
    assert!(ids(&crate::commands::chips(&input)).contains(&CommandId::TableOpenWindow));
    let copied = transformed_text(src, CardView::Dataframe).expect("copy");
    assert!(copied.contains("Sunbox 7"), "{copied}");
    assert!(
        ids(&crate::commands::chips(&data(SubjectKind::Text, Some(src))))
            .contains(&CommandId::TableOpenWindow)
    );
}

#[test]
fn the_dataframe_view_shows_the_table_version() {
    let src = "name,n\na,1\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let original = work_card(&input);
    assert!(original.excerpt.contains("┌"), "{}", original.excerpt);
    input.table = Some(deduped(src));
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    assert!(card.meta.starts_with("3 lines"), "{}", card.meta);
    // The step of the version shown, right after the size.
    assert!(
        card.meta.contains("KB  ·  Duplicates removed"),
        "{}",
        card.meta
    );
    // Another version is the same entry: it stays revealed ([`stays_revealed`]).
    let plain = data(SubjectKind::Text, Some(src));
    let mut versioned = plain.clone();
    versioned.table = input.table.clone();
    assert_eq!(content_key(&plain), content_key(&versioned));
    // The Original view shows the text as copied.
    versioned.view = CardView::Original;
    assert_eq!(work_card(&versioned).excerpt, work_card(&plain).excerpt);
}

#[test]
fn a_table_version_being_worked_out_says_so_and_errors_show_in_the_meta() {
    let src = "name,n\na,1\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let mut table = deduped(src);
    table.frame = None;
    table.working = true;
    input.table = Some(table.clone());
    let card = work_card(&input);
    assert!(card.excerpt.is_empty());
    assert_eq!(card.placeholder, "Working on the table…");
    table = deduped(src);
    table.error = Some("20 versions at most".to_string());
    input.table = Some(table);
    assert!(work_card(&input).meta.contains("20 versions at most"));
    // The original with a table attached is the plain Dataframe card.
    let mut original = deduped(src);
    original.version = 0;
    original.labels.truncate(1);
    input.table = None;
    let plain = work_card(&input);
    input.table = Some(original);
    assert_eq!(work_card(&input).excerpt, plain.excerpt);
}

#[test]
fn table_chip_row_is_original_and_table_only_no_show_columns() {
    use crate::table::TableOp;
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let names: Vec<String> = crate::dataframe::parse_table(src)
        .expect("table")
        .get_column_names()
        .iter()
        .take(6)
        .map(|name| name.to_string())
        .collect();
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let mut table = stepped(
        src,
        TableOp::SelectColumns {
            columns: names,
            kept_of: None,
        },
    );
    // Even if overview were requested, display is the grid.
    table.overview = Some(true);
    input.table = Some(table.clone());
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    assert!(ids(&chips(&input)).contains(&CommandId::TableOpenWindow));
    table.overview = Some(false);
    input.table = Some(table);
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    let narrow = "name,n\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(narrow));
    input.view = CardView::Dataframe;
    let table = read(narrow, crate::dataframe::ReadOptions::default());
    input.table = Some(table);
    assert!(work_card(&input).excerpt.contains("┌"));
    assert!(ids(&chips(&input)).contains(&CommandId::TableOpenWindow));
}

#[test]
fn a_revealed_entry_stays_revealed_in_every_view_version_and_step() {
    use crate::sensitivity::Label;
    let energy = crate::dataframe::tests::ENERGY_FIXTURE;
    let pii = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
    for src in [energy, pii] {
        let key = content_key(&data(SubjectKind::Text, Some(src)));
        for (what, shown) in shown_again(src) {
            assert_eq!(content_key(&shown), key, "{what}");
            assert!(stays_revealed(true, key, content_key(&shown)), "{what}");
            // The card is built whatever it shows.
            let _ = work_card(&shown);
        }
    }
    // Views of one entry show different labels; that does not mask it again.
    let mut original = data(SubjectKind::Text, Some(pii));
    let labels = meta_labels(&work_card(&original).meta);
    assert_eq!(labels, [Label::Pii, Label::Financial]);
    original.view = CardView::Dataframe;
    original.table = Some(stepped(
        pii,
        crate::table::TableOp::SelectColumns {
            columns: vec!["Id".into(), "Naam".into()],
            kept_of: Some(3),
        },
    ));
    let card = work_card(&original);
    assert!(
        !meta_labels(&card.meta).contains(&Label::Financial),
        "{}",
        card.meta
    );
    let key = content_key(&original);
    assert!(stays_revealed(true, key, key));
    // Not revealed stays not revealed.
    assert!(!stays_revealed(false, key, key));
}

#[test]
fn a_revealed_entry_stays_revealed_when_its_labels_arrive() {
    // Long enough for the background checker: "Checking…" first, then the labels.
    let mut src = String::from("name,email\n");
    while src.len() <= crate::sensitivity::SYNC_LIMIT {
        src.push_str("Jan,jan@example.com\n");
    }
    let shown = data(SubjectKind::Text, Some(&src));
    let key = content_key(&shown);
    let checking = work_card(&shown);
    // Neither the card's labels nor their status are part of the entry's identity: the
    // card rebuilt when they arrive is the same entry, and stays revealed.
    let _ = crate::sensitivity::labeling(&src);
    let arrived = work_card(&shown);
    assert_eq!(content_key(&shown), key);
    assert!(stays_revealed(true, key, content_key(&shown)));
    assert!(!checking.excerpt.is_empty() && !arrived.excerpt.is_empty());
}

#[test]
fn another_entry_is_masked_again() {
    let first = data(SubjectKind::Text, Some("Id;Naam\n1;Piet"));
    let second = data(SubjectKind::Text, Some("Id;Naam\n1;Jan"));
    assert!(!stays_revealed(
        true,
        content_key(&first),
        content_key(&second)
    ));
    let mut opened = first.clone();
    opened.source_name = Some("piet.csv".into());
    assert!(!stays_revealed(
        true,
        content_key(&first),
        content_key(&opened)
    ));
}

#[test]
fn dataframe_meta_says_when_dates_could_be_either_order() {
    let mut input = data(
        SubjectKind::Text,
        Some("when,amount\n01/02/2024,1\n03/04/2024,2"),
    );
    input.view = CardView::Dataframe;
    let card = work_card(&input);
    assert!(
        card.meta.contains("  ·  Dates read as dd/mm/yyyy"),
        "{}",
        card.meta
    );
    input.full = true;
    assert!(work_card(&input).meta.contains("Dates read as dd/mm/yyyy"));
}

#[test]
fn a_long_table_below_a_title_counts_rows_from_the_header() {
    let mut src = String::from("Title of the export\nname,age\n");
    for i in 0..(PREVIEW_ROWS + 50) {
        src.push_str(&format!("p{i},{i}\n"));
    }
    let card = work_card(&data(SubjectKind::Text, Some(&src)));
    assert_eq!(
        card.preview_note.as_deref(),
        Some(format!("Showing 199 of {} rows", PREVIEW_ROWS + 50).as_str())
    );
}

#[test]
fn salary_table_puts_the_labels_on_the_size_line() {
    let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert!(card.meta.contains("lines"));
    assert!(card.meta.contains("KB"));
    assert!(card.meta.contains("PII · financial"));
    assert!(!card.meta.contains("Jan de Vries"));
    let mut grid = data(SubjectKind::Text, Some(src));
    grid.view = CardView::Dataframe;
    let card = work_card(&grid);
    assert_eq!(card.title, "Table");
    assert!(card.meta.contains("PII · financial"), "{}", card.meta);
}

#[test]
fn chosen_file_keeps_the_kind_title_and_names_the_meta_line() {
    let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
    let mut input = data(SubjectKind::Text, Some(src));
    input.source_name = Some("people.tsv".to_string());
    let card = work_card(&input);
    assert_eq!(card.title, "TSV");
    assert!(card.meta.starts_with("people.tsv"));
    assert!(card.meta.contains("PII · financial"));
    assert!(card.excerpt.contains("Jan de Vries"));
}

#[test]
fn dropped_picture_is_an_image_card_with_its_name() {
    let mut input = data(SubjectKind::Image, None);
    input.source_name = Some("photo.heic".to_string());
    input.image = Some(ImageFacts {
        format: "HEIC".into(),
        width: 4032,
        height: 3024,
        byte_len: 2_000_000,
    });
    input.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    let card = work_card(&input);
    assert_eq!(card.title, "Image");
    assert_eq!(card.meta, "photo.heic  ·  HEIC  4032×3024  2.00 MB");
    assert!(card.shows_image);
    assert!(masks_content(&card, CardView::Original));
    assert_eq!(titles(&chips(&input)), vec!["Info"]);
    assert!(
        overflow(&input)
            .iter()
            .any(|cmd| cmd.id == CommandId::UseClipboard)
    );

    // Another picture alike in name, format and size is another item: not revealed yet.
    let mut other = data(SubjectKind::Image, None);
    other.source_name = input.source_name.clone();
    other.image = input.image.clone();
    other.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    assert_ne!(content_key(&input), content_key(&other));
    other.picture = input.picture.clone();
    assert_eq!(content_key(&input), content_key(&other));
}

#[test]
fn unreadable_file_uses_its_name_and_a_note() {
    let mut input = data(SubjectKind::NoText, None);
    input.source_name = Some("photo.png".to_string());
    input.source_note = Some("This file is not text".to_string());
    let card = work_card(&input);
    assert_eq!(card.title, "photo.png");
    assert_eq!(card.placeholder, "This file is not text");
    assert!(card.excerpt.is_empty());
    assert!(card.meta.is_empty());
    assert!(!card.shows_image);
}

#[test]
fn text_card_shows_the_payload_not_a_label() {
    let card = work_card(&data(SubjectKind::Text, Some("{\n  \"a\": 1\n}")));
    assert_eq!(card.title, "Valid JSON");
    assert_eq!(card.excerpt, "{\n  \"a\": 1\n}");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    assert!(card.meta.contains("lines"));
    assert!(card.meta.contains("KB"));
    assert!(!card.excerpt.contains("Preview"));
    assert!(!card.shows_image);
}

#[test]
fn a_clipboard_without_text_points_to_dropping_a_file() {
    let card = work_card(&data(SubjectKind::NoText, None));
    assert_eq!(
        card.placeholder,
        "No text on the clipboard\nDrop a text file or image here to open it"
    );
    assert!(card.excerpt.is_empty());
}

#[test]
fn a_copy_not_read_yet_says_so_and_the_first_one_explains_the_alert() {
    let pending = data(SubjectKind::Pending, None);
    let card = work_card(&pending);
    assert!(
        card.placeholder
            .starts_with("New copy — open the card to view\n")
    );
    assert!(card.excerpt.is_empty() && card.meta.is_empty());
    assert!(chips(&pending).is_empty());
    assert!(!content_actions(&pending).copy);
    let ask = data(SubjectKind::PasteAsk, None);
    let card = work_card(&ask);
    assert!(
        card.placeholder.contains("Choose Allow"),
        "{}",
        card.placeholder
    );
    assert!(card.placeholder.contains("Paste from Other Apps"));
    assert!(chips(&ask).is_empty());
}

#[test]
fn always_deny_says_so_instead_of_nothing_copied() {
    let input = data(SubjectKind::Denied, None);
    let card = work_card(&input);
    assert!(
        card.placeholder
            .starts_with("Clipboard access denied in Privacy & Security\n"),
        "{}",
        card.placeholder
    );
    assert!(!card.placeholder.contains("Nothing copied"));
    assert!(card.excerpt.is_empty() && card.meta.is_empty());
    assert!(chips(&input).is_empty());
    assert!(!content_actions(&input).copy);
}

#[test]
fn well_masks_copied_content_until_it_changes() {
    let secret = data(SubjectKind::Text, Some("correct horse battery staple"));
    let card = work_card(&secret);
    assert!(masks_content(&card, CardView::Original));
    assert!(!card.excerpt.contains("Click to reveal"));
    let empty = work_card(&data(SubjectKind::Empty, None));
    assert!(!masks_content(&empty, CardView::Original));
    assert_eq!(
        content_key(&secret),
        content_key(&data(
            SubjectKind::Text,
            Some("correct horse battery staple")
        ))
    );
    let mut formatted = secret.clone();
    formatted.view = CardView::Format;
    assert_eq!(content_key(&secret), content_key(&formatted));
    assert_ne!(
        content_key(&secret),
        content_key(&data(SubjectKind::Text, Some("another copy")))
    );
    let mut picture = data(SubjectKind::Image, None);
    picture.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 1280,
        height: 720,
        byte_len: 184_000,
    });
    assert!(masks_content(&work_card(&picture), CardView::Original));
    let mut info = picture.clone();
    info.view = CardView::Info;
    assert!(!masks_content(&work_card(&info), CardView::Info));
    assert_eq!(content_key(&picture), content_key(&info));
    let mut other = picture.clone();
    if let Some(image) = other.image.as_mut() {
        image.byte_len = 200_000;
    }
    assert_ne!(content_key(&picture), content_key(&other));
}

#[test]
fn history_label_is_the_mark_and_size() {
    let secret = "correct horse battery staple";
    let label = history_label("Aa", secret.len());
    assert_eq!(label, "Aa  0.028 KB");
    assert!(!label.contains(secret));
    assert!(!label.contains("horse"));
    assert_eq!(history_label("{}", 1_500), "{}  1.5 KB");
    assert_eq!(history_label("img", 184_000), "img  0.18 MB");
}

#[test]
fn prose_card_shows_the_whole_document() {
    let src = "\
# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.

## Commands

Kleinste opdracht die de change dekt.
";
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert_eq!(card.excerpt, src);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Markdown));
    assert_eq!(card.title, "Markdown");
    assert!(
        chips(&data(SubjectKind::Text, Some(src)))
            .iter()
            .all(|cmd| cmd.id != CommandId::Format)
    );
    assert!(
        card.excerpt
            .contains("Kleinste opdracht die de change dekt.")
    );
    assert!(card.excerpt.contains("thiserror"));
    assert!(!card.excerpt.contains('…'));
}

#[test]
fn excerpt_keeps_lines_and_cuts_a_long_one() {
    assert_eq!(payload_excerpt("alpha\nbeta"), "alpha\nbeta");
    let long = "x".repeat(80);
    let cut = payload_excerpt(&long);
    assert!(cut.ends_with('…'));
    let many = (0..12)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let clipped = payload_excerpt(&many);
    assert_eq!(clipped.lines().count(), 6);
    assert!(clipped.ends_with('…'));
    assert!(cut.chars().count() <= 49);
}

#[test]
fn large_table_card_previews_rows_and_show_all_renders_them() {
    let src = csv_rows(300);
    let mut d = data(SubjectKind::Text, Some(&src));
    for view in [CardView::Original, CardView::Dataframe] {
        d.view = view;
        d.full = false;
        let card = work_card(&d);
        assert_eq!(
            card.preview_note.as_deref(),
            Some("Showing 200 of 300 rows"),
            "{view:?}"
        );
        assert!(card.excerpt.contains("name199"), "{view:?}");
        assert!(!card.excerpt.contains("name299"), "{view:?}");
        d.full = true;
        let full = work_card(&d);
        assert!(full.preview_note.is_none(), "{view:?}");
        if view == CardView::Original {
            assert!(full.excerpt.contains("name299"));
        }
    }
}

#[test]
fn small_table_card_has_no_preview_note() {
    let src = csv_rows(20);
    let mut d = data(SubjectKind::Text, Some(&src));
    for view in [CardView::Original, CardView::Dataframe] {
        d.view = view;
        assert!(work_card(&d).preview_note.is_none(), "{view:?}");
    }
}

#[test]
fn a_diff_chip_sits_beside_original_and_waits_to_be_copied() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.diff = Some(diff_shown(crate::diff::Phase::Working));
    let titles: Vec<_> = chips(&input).iter().map(|cmd| cmd.title.clone()).collect();
    assert_eq!(
        titles,
        [
            crate::locale::t("chip_diff").to_string(),
            crate::locale::t("text_menu").to_string(),
        ]
    );
    // The spinner text is the diff view. The chip is already there on the source view.
    input.view = CardView::Diff;
    let card = work_card(&input);
    assert_eq!(card.placeholder, crate::locale::t("diff_working"));
    assert!(card.excerpt.is_empty());
    assert!(masks_content(&card, CardView::Diff));
    assert!(!content_actions(&input).copy && !content_actions(&input).save);

    input.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let card = work_card(&input);
    assert_eq!(card.meta, crate::locale::t("diff_none"));
    assert!(content_actions(&input).copy && content_actions(&input).save);

    let lines = match crate::diff::compare("alpha\n", "alpha\nbeta\n") {
        crate::diff::Outcome::Lines(lines) => lines,
        other => panic!("expected lines, got {other:?}"),
    };
    input.diff = Some(diff_shown(crate::diff::Phase::Lines(lines)));
    let card = work_card(&input);
    assert_eq!(card.meta, crate::locale::tf("diff_counts", &[&1, &0]));
    assert!(card.excerpt.contains("+beta"));

    input.diff = Some(diff_shown(crate::diff::Phase::TooLarge));
    assert_eq!(
        work_card(&input).excerpt,
        crate::locale::t("diff_too_large")
    );
}

#[test]
fn the_shown_mark_prefixes_the_meta_line_except_on_the_diff() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.shown_mark = Some("A");
    let card = work_card(&input);
    assert!(card.meta.starts_with("A  ·  "), "{}", card.meta);
    input.view = CardView::Diff;
    input.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let card = work_card(&input);
    assert!(!card.meta.starts_with("A"), "{}", card.meta);
}

#[test]
fn diff_labels_and_a_running_check_stay_on_the_meta_line() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.view = CardView::Diff;
    let mut shown = diff_shown(crate::diff::Phase::Same(Default::default()));
    shown.labels = crate::sensitivity::Labeling::Known(crate::sensitivity::Found {
        labels: vec![crate::sensitivity::Label::Pii],
    });
    shown.sensitive = true;
    input.diff = Some(shown);
    let card = work_card(&input);
    assert!(card.meta.contains("PII"), "{}", card.meta);
    assert!(!crate::commands::is_checking(&card));

    let mut shown = diff_shown(crate::diff::Phase::Working);
    shown.labels = crate::sensitivity::Labeling::Checking;
    input.diff = Some(shown);
    let card = work_card(&input);
    assert!(crate::commands::is_checking(&card), "{}", card.meta);
}

#[test]
fn a_diff_has_its_own_content_key_and_keeps_the_card_open() {
    let mut left = data(SubjectKind::Text, Some("hello"));
    left.view = CardView::Diff;
    left.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let mut right = left.clone();
    right.diff.as_mut().expect("diff").token = 8;
    assert_ne!(content_key(&left), content_key(&right));
    assert_ne!(
        content_key(&left),
        content_key(&data(SubjectKind::Text, Some("hello")))
    );
    assert!(keeps_card_open(&CommandId::Diff));
    assert!(keeps_card_open(&CommandId::DiffToggle(1)));
    assert!(keeps_card_open(&CommandId::DiffShown));
    assert!(keeps_card_open(&CommandId::DiffSwap));
}

#[test]
fn a_table_card_has_the_ai_chip_left_of_the_table_window_chip() {
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let card = data(SubjectKind::Text, Some(src));
    let chips = crate::commands::card_chips(&card);
    let all = ids(&chips);
    let window = all
        .iter()
        .position(|id| *id == CommandId::TableOpenWindow)
        .expect("table window chip");
    assert_eq!(all[window - 1], CommandId::AiMenu);
    // Lines… stays off a table.
    assert!(!all.contains(&CommandId::TextMenu));
    // Dimmed with the reason as tooltip when the model is not there.
    let ai = chips
        .iter()
        .find(|c| c.id == CommandId::AiMenu)
        .expect("AI chip");
    if let Err(reason) = mac_ai::availability() {
        assert!(!ai.enabled);
        assert_eq!(ai.detail, crate::commands::ai_reason(reason));
    }
    // Prose has no table chip and so no table AI chip; a picture neither.
    let prose = crate::commands::card_chips(&data(SubjectKind::Text, Some("just some words here")));
    assert!(!ids(&prose).contains(&CommandId::TableOpenWindow));
    let image = crate::commands::card_chips(&data(SubjectKind::Image, None));
    assert!(!ids(&image).contains(&CommandId::AiMenu));
}

#[test]
fn the_table_ai_menu_has_the_table_actions_only() {
    let ok = crate::commands::ai_table_menu_items(Ok(()));
    let all = ids(&ok);
    assert_eq!(
        all,
        vec![
            CommandId::AiCustom,
            CommandId::Ai(mac_ai::Action::Summarize),
            CommandId::AiTableColumns,
            CommandId::AiTableMarkdown,
        ]
    );
    assert!(ok.iter().all(|c| c.enabled));
    let off = crate::commands::ai_table_menu_items(Err(mac_ai::Unavailable::AppleIntelligenceOff));
    let off_ids = ids(&off);
    assert_eq!(off_ids.last(), Some(&CommandId::AiUnavailable));
    assert!(off_ids.contains(&CommandId::AiTableMarkdown));
    assert!(off.iter().all(|c| !c.enabled));
}
