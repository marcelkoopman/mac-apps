use super::*;

#[test]
fn a_picture_has_image_menu_and_no_version_bar() {
    use crate::image_edit::{ImageOp, Resize};
    let plain = edited_picture(None, false, false);
    assert_eq!(titles(&chips(&plain)), vec!["Info", "Image ▾"]);
    assert_eq!(VersionBar::of(&plain), None);
    assert!(keeps_card_open(&CommandId::ImageMenu));
    assert!(keeps_card_open(&CommandId::ImageResizeCustom));
    assert!(keeps_card_open(&CommandId::ImageStep(ImageOp::Grayscale)));

    let menu = crate::commands::image_menu_items();
    let menu_titles: Vec<&str> = menu.iter().map(|(cmd, _)| cmd.title.as_str()).collect();
    assert_eq!(
        menu_titles,
        [
            "50%",
            "25%",
            "Longest side 2048",
            "Longest side 1024",
            "Longest side 512",
            "Custom…",
            "Rotate 90° Left",
            "Rotate 90° Right",
            "Flip Horizontal",
            "Flip Vertical",
            "Remove Metadata",
            "Grayscale",
        ]
    );
    // Each step makes a new copy: no undo or redo rows, and search offers none.
    assert!(
        !search_pool(&plain)
            .iter()
            .any(|cmd| matches!(cmd.id, CommandId::StepBack | CommandId::StepForward))
    );
    let grouped: Vec<Option<&str>> = menu.iter().map(|(cmd, _)| menu_group(&cmd.id)).collect();
    assert_eq!(grouped[..6], [Some("Resize"); 6]);
    assert!(grouped[6..].iter().all(Option::is_none));
    assert!(menu[..12].iter().all(|(cmd, _)| cmd.enabled));
    assert_eq!(
        menu[3].0.id,
        CommandId::ImageStep(ImageOp::Resize(Resize::Longest(1024)))
    );

    // A step's copy: still no capsule ("1 of 2"), but ⌘Z and ⇧⌘Z go between the copies.
    let edited = edited_picture(Some("Grayscale"), true, false);
    assert_eq!(VersionBar::of(&edited), None);
    assert_eq!(
        image_undo_key("z", true, false, false, edited.image_edit.as_ref()),
        Some(CommandId::StepBack)
    );
    assert_eq!(
        image_undo_key("Z", true, true, false, edited.image_edit.as_ref()),
        None,
        "nothing to redo"
    );
    assert_eq!(
        image_undo_key("z", false, false, false, edited.image_edit.as_ref()),
        None
    );
    assert_eq!(image_undo_key("z", true, false, false, None), None);
    // The steps are found by search too.
    assert!(
        search_pool(&edited)
            .iter()
            .any(|cmd| cmd.id == CommandId::ImageStep(ImageOp::FlipVertical))
    );
    // A picture without Image ▾ (the clipboard's, not in history).
    let mut bare = edited_picture(None, false, false);
    bare.image_edit = None;
    assert_eq!(titles(&chips(&bare)), vec!["Info"]);
}

#[test]
fn overflow_empties_only_the_pasteboard() {
    let menu = overflow(&data(SubjectKind::Text, Some("secret")));
    let item = menu
        .iter()
        .find(|cmd| cmd.id == CommandId::ClearClipboard)
        .unwrap();
    assert_eq!(item.title, "Empty Clipboard");
    assert_eq!(item.detail, "Clipboard only");
    assert!(menu.iter().all(|cmd| cmd.id != CommandId::Clear));
    assert!(keeps_card_open(&CommandId::Clear));
}

#[test]
fn the_version_bar_shows_in_the_dataframe_view_with_two_versions_or_more() {
    let src = "name,n\na,1\na,1";
    let mut input = data(SubjectKind::Text, Some(src));
    input.table = Some(deduped(src));
    assert_eq!(VersionBar::of(&input), None);
    input.view = CardView::Dataframe;
    let bar = VersionBar::of(&input).expect("bar");
    assert_eq!(bar.title(), "v2/2");
    assert_eq!(bar.spoken(), "Version 2 of 2, Duplicates removed");
    assert!(bar.can_undo() && !bar.can_redo());
    let menu = bar.menu();
    assert_eq!(menu.len(), 2);
    assert_eq!(menu[0].0.title, "1. Original");
    assert_eq!(menu[0].0.id, CommandId::TableVersion(0));
    assert!(!menu[0].1 && menu[1].1);
    assert!(keeps_card_open(&CommandId::TableVersion(0)));
    // The original alone has no bar.
    let mut original = deduped(src);
    original.labels.truncate(1);
    original.version = 0;
    input.table = Some(original);
    assert_eq!(VersionBar::of(&input), None);
}

#[test]
fn command_z_undoes_and_shift_command_z_redoes_a_table_step() {
    let bar = VersionBar {
        labels: vec!["Original".into(), "Duplicates removed".into()],
        current: 1,
    };
    assert_eq!(
        undo_key("z", true, false, false, Some(&bar)),
        Some(CommandId::StepBack)
    );
    // Nothing to redo at the newest version; the key is left alone.
    assert_eq!(undo_key("Z", true, true, false, Some(&bar)), None);
    let back = VersionBar { current: 0, ..bar };
    assert_eq!(
        undo_key("Z", true, true, false, Some(&back)),
        Some(CommandId::StepForward)
    );
    assert_eq!(undo_key("z", true, false, false, Some(&back)), None);
    assert_eq!(undo_key("z", false, false, false, Some(&back)), None);
    assert_eq!(undo_key("z", true, true, true, Some(&back)), None);
    assert_eq!(undo_key("x", true, false, false, Some(&back)), None);
    assert_eq!(undo_key("z", true, false, false, None), None);
}

#[test]
fn choose_file_leads_the_menu_and_a_file_can_return_to_the_clipboard() {
    let clipboard = data(SubjectKind::Text, Some("hello"));
    let menu = overflow(&clipboard);
    assert_eq!(menu[0].id, CommandId::ChooseFile);
    assert_eq!(menu[0].title, "Choose File…");
    assert!(menu.iter().all(|cmd| cmd.id != CommandId::UseClipboard));
    assert!(keeps_card_open(&CommandId::ChooseFile));
    assert!(keeps_card_open(&CommandId::UseClipboard));

    let mut file = clipboard;
    file.source_name = Some("notes.txt".to_string());
    let menu = overflow(&file);
    assert_eq!(menu[0].id, CommandId::ChooseFile);
    assert_eq!(menu[1].id, CommandId::UseClipboard);
    assert_eq!(menu[1].title, "Clipboard");
    assert_ne!(
        content_key(&file),
        content_key(&data(SubjectKind::Text, Some("hello")))
    );
}

/// ⋯ › New: the five generators in a submenu of the File section, after Choose File and
/// Clipboard; each keeps the card open (it shows the new value).
#[test]
fn new_lists_the_generators_after_the_file_items() {
    use crate::commands::{MenuSection, menu_section, menu_submenu};
    use crate::generate::Generator;
    let menu = overflow(&data(SubjectKind::Text, Some("hello")));
    let generated: Vec<CommandId> = menu
        .iter()
        .map(|cmd| cmd.id.clone())
        .filter(|id| matches!(id, CommandId::Generate(_)))
        .collect();
    assert_eq!(generated, Generator::ALL.map(CommandId::Generate).to_vec());
    assert_eq!(menu[1].id, CommandId::Generate(Generator::UuidV4));
    for generator in Generator::ALL {
        let id = CommandId::Generate(generator);
        assert_eq!(menu_section(&id), MenuSection::File);
        assert_eq!(menu_submenu(&id), Some("New"));
        assert!(keeps_card_open(&id));
    }
}

#[test]
fn query_finds_appearance_and_history_but_not_quit() {
    let mut input = data(SubjectKind::Text, Some("hello world"));
    input.history.push(Hist {
        index: 3,
        title: "notes from yesterday".into(),
        mark: "¶".into(),
        id: 3,
        text: true,
        search: Zeroizing::new(String::new()),
        pinned: false,
    });
    let pool = search_pool(&input);
    assert!(matching(&pool, "quit").is_empty());
    assert_eq!(
        ids(&matching(&pool, "dark")),
        vec![CommandId::Appearance(Theme::Dark)]
    );
    assert!(
        chips(&input)
            .iter()
            .all(|cmd| !matches!(cmd.id, CommandId::Appearance(_) | CommandId::History(_)))
    );
    assert_eq!(
        ids(&matching(&pool, "yesterday")),
        vec![CommandId::History(3)]
    );
}

#[test]
fn default_chips_leave_history_for_search() {
    let mut input = data(SubjectKind::Text, Some("hello world"));
    for index in 0..10 {
        input.history.push(Hist {
            index,
            title: format!("older {index}"),
            mark: "¶".into(),
            id: index as u64,
            text: true,
            search: Zeroizing::new(String::new()),
            pinned: false,
        });
    }
    input.can_clear_history = true;
    let shown = chips(&input);
    assert!(shown.iter().all(|cmd| !housekeeping(&cmd.id)));
    assert!(
        shown
            .iter()
            .all(|cmd| !matches!(cmd.id, CommandId::History(_)))
    );
    assert_eq!(
        ids(&matching(&search_pool(&input), "older 9")),
        vec![CommandId::History(9)]
    );
    assert!(
        overflow(&input)
            .iter()
            .any(|cmd| cmd.id == CommandId::ClearHistory)
    );
    assert!(
        overflow(&input)
            .iter()
            .any(|cmd| cmd.id == CommandId::History(9))
    );
}

#[test]
fn search_hits_the_body_while_a_masked_title_stays_a_label() {
    let mut input = data(SubjectKind::Text, Some("hello world"));
    let phrase = "secret phrase past the label";
    input.history.push(Hist {
        index: 1,
        title: "¶  28 B".into(),
        mark: "¶".into(),
        id: 4,
        text: true,
        search: Zeroizing::new(phrase.into()),
        pinned: true,
    });
    input.settings.privacy_filter = true;
    let masked = matching(&search_pool(&input), "phrase");
    assert_eq!(ids(&masked), vec![CommandId::History(1)]);
    assert_eq!(masked[0].title, "¶  28 B");
    assert!(!masked[0].title.contains("secret"));
    assert!(!masked[0].title.contains("phrase"));
    input.settings.privacy_filter = false;
    let open = matching(&search_pool(&input), "phrase");
    // The pin is an image on the chip, not a mark in its title.
    assert_eq!(open[0].title, phrase);
    assert!(open[0].pinned && masked[0].pinned);
    let mut long = "a".repeat(90);
    long.push_str("unique-needle");
    input.history[0].search = Zeroizing::new(long);
    input.history[0].pinned = false;
    let hit = matching(&search_pool(&input), "unique-needle");
    assert_eq!(ids(&hit), vec![CommandId::History(1)]);
    assert!(!hit[0].pinned);
    assert!(!hit[0].title.contains("unique-needle"));
    assert!(hit[0].title.ends_with('…'));
}

#[test]
fn two_picks_offer_a_diff_and_a_third_menu_says_add() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.history.push(history_item(0, "first", true));
    input.history.push(history_item(1, "second", false));
    input.shown_id = Some(0);
    input.diff_picks = vec![0];
    let menu = overflow(&input);
    let shown_at = menu
        .iter()
        .position(|cmd| cmd.id == CommandId::DiffShown)
        .expect("on the menu");
    let history_at = menu
        .iter()
        .position(|cmd| cmd.id == CommandId::History(0))
        .expect("history");
    assert!(shown_at < history_at);
    assert_eq!(menu[shown_at].title, crate::locale::t("diff_remove"));
    assert!(menu[shown_at].enabled);
    assert!(
        matching(&search_pool(&input), "diff")
            .iter()
            .any(|cmd| cmd.id == CommandId::DiffShown)
    );
    let second = menu
        .iter()
        .find(|cmd| cmd.id == CommandId::DiffToggle(1))
        .expect("image row");
    assert_eq!(second.title, crate::locale::t("diff_compare"));
    assert!(!second.enabled);
    let first = menu
        .iter()
        .find(|cmd| cmd.id == CommandId::DiffToggle(0))
        .expect("text row");
    assert_eq!(first.title, crate::locale::t("diff_remove"));
    assert!(first.enabled);
    assert!(menu.iter().any(|cmd| cmd.title.starts_with("A  ")));
    assert!(menu.iter().all(|cmd| cmd.id != CommandId::DiffSwap));

    input.history.push(history_item(2, "third", true));
    input.diff_picks = vec![0, 2];
    let menu = overflow(&input);
    assert!(menu.iter().any(|cmd| {
        cmd.id == CommandId::DiffSwap && cmd.title == crate::locale::t("diff_swap")
    }));
    let third = menu
        .iter()
        .find(|cmd| cmd.id == CommandId::History(2))
        .expect("third");
    assert!(third.title.starts_with("B  "));
    let idle = menu
        .iter()
        .find(|cmd| cmd.id == CommandId::DiffToggle(1))
        .expect("not chosen");
    assert_eq!(idle.title, crate::locale::t("diff_add"));
    assert!(!idle.enabled);
}

#[test]
fn command_z_puts_a_removed_copy_back_before_a_table_undo() {
    use crate::commands::{keeps_card_open, restore_key};
    assert_eq!(
        restore_key("z", true, false, false, true),
        Some(CommandId::RestoreHistory)
    );
    assert_eq!(
        restore_key("Z", true, false, false, true),
        Some(CommandId::RestoreHistory)
    );
    // Nothing removed, ⇧⌘Z, another modifier, no ⌘ or another key: not a restore.
    assert_eq!(restore_key("z", true, false, false, false), None);
    assert_eq!(restore_key("z", true, true, false, true), None);
    assert_eq!(restore_key("z", true, false, true, true), None);
    assert_eq!(restore_key("z", false, false, false, true), None);
    assert_eq!(restore_key("x", true, false, false, true), None);
    assert!(keeps_card_open(&CommandId::RestoreHistory));
    assert!(keeps_card_open(&CommandId::RemoveHistory(1)));
}

#[test]
fn delete_removes_the_marked_strip_card_but_not_while_searching() {
    use crate::commands::delete_key;
    assert_eq!(
        delete_key(false, true, 1),
        Some(CommandId::RemoveHistory(1))
    );
    assert_eq!(delete_key(true, true, 1), None);
    assert_eq!(delete_key(false, false, 0), None);
}

#[test]
fn swap_lives_under_more_also_with_a_full_chip_row() {
    use crate::commands::{card_chips, split_chips};
    let mut input = data(SubjectKind::Text, Some(r#"{"name":"copycraft"}"#));
    input.history.push(history_item(0, "first", true));
    input.history.push(history_item(1, "second", true));
    input.diff_picks = vec![0, 1];
    let (shown, more) = split_chips(card_chips(&input));
    assert!(shown.iter().all(|cmd| cmd.id != CommandId::DiffSwap));
    let menu: Vec<_> = more.into_iter().chain(overflow(&input)).collect();
    assert_eq!(
        menu.iter()
            .filter(|cmd| cmd.id == CommandId::DiffSwap)
            .count(),
        1
    );
    // Were Swap ever a chip, a fifth one would still go under ⋯.
    let chip = |id: CommandId| crate::commands::Command {
        id,
        title: String::new(),
        detail: String::new(),
        keywords: String::new(),
        enabled: true,
        pinned: false,
    };
    let row = [
        CommandId::Convert,
        CommandId::Format,
        CommandId::Schema,
        CommandId::Sample,
        CommandId::DiffSwap,
    ];
    let (shown, more) = split_chips(row.into_iter().map(chip).collect());
    assert!(shown.iter().all(|cmd| cmd.id != CommandId::DiffSwap));
    assert!(more.iter().any(|cmd| cmd.id == CommandId::DiffSwap));
}

#[test]
fn a_pinned_history_row_carries_the_pin_not_a_mark() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.history.push(history_item(0, "¶  5 B", true));
    input.history.push(history_item(1, "¶  9 B", true));
    input.history[1].pinned = true;
    let menu = overflow(&input);
    let row = |index| {
        menu.iter()
            .find(|cmd| cmd.id == CommandId::History(index))
            .expect("history row")
    };
    assert!(!row(0).pinned);
    assert!(row(1).pinned);
    assert_eq!(row(1).title, "¶  9 B", "the title has no mark");
    assert!(
        menu.iter()
            .filter(|cmd| !matches!(cmd.id, CommandId::History(_)))
            .all(|cmd| !cmd.pinned)
    );
    assert_eq!(crate::commands::PIN_SYMBOL, "pin.fill");
    let label = crate::commands::pinned_label("¶  9 B");
    assert_eq!(label, format!("¶  9 B, {}", crate::commands::pin_label()));
    assert!(!label.contains('•'));
}

#[test]
fn a_text_step_makes_a_new_copy_without_a_capsule_or_undo_rows() {
    use crate::commands::{TextShown, text_menu_items, text_undo_key};
    let mut card = data(SubjectKind::Text, Some("b\na\n"));
    card.text_edit = Some(TextShown {
        can_undo: true,
        ..TextShown::default()
    });
    // No "v2/2": the step's copy is a history entry of its own, as an Image ▾ step's.
    assert_eq!(VersionBar::of(&card), None);
    let edit = card.text_edit.clone().expect("Text ▾");
    assert!(
        text_menu_items(&edit)
            .iter()
            .all(|(cmd, _)| !matches!(cmd.id, CommandId::StepBack | CommandId::StepForward))
    );
    assert!(
        !search_pool(&card)
            .iter()
            .any(|cmd| matches!(cmd.id, CommandId::StepBack | CommandId::StepForward))
    );
    // ⌘Z shows the copy the step was taken on; ⇧⌘Z has nothing to go to yet.
    assert_eq!(
        text_undo_key("z", true, false, false, Some(&edit)),
        Some(CommandId::StepBack)
    );
    assert_eq!(text_undo_key("Z", true, true, false, Some(&edit)), None);
    let back = TextShown {
        can_redo: true,
        ..TextShown::default()
    };
    assert_eq!(
        text_undo_key("Z", true, true, false, Some(&back)),
        Some(CommandId::StepForward)
    );
    assert_eq!(text_undo_key("z", true, false, false, Some(&back)), None);
    assert_eq!(text_undo_key("z", false, false, false, Some(&edit)), None);
    assert_eq!(text_undo_key("z", true, false, true, Some(&edit)), None);
    assert_eq!(text_undo_key("z", true, false, false, None), None);
}
