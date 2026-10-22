use super::*;

#[test]
fn a_picture_with_versions_has_image_menu_and_version_bar() {
    use crate::image_edit::{ImageOp, Resize};
    let plain = edited_picture(0, &["Original"]);
    assert_eq!(titles(&chips(&plain)), vec!["Info", "Image ▾"]);
    assert_eq!(VersionBar::of(&plain), None, "one version: no capsule");
    assert!(keeps_card_open(&CommandId::ImageMenu));
    assert!(keeps_card_open(&CommandId::ImageResizeCustom));
    assert!(keeps_card_open(&CommandId::ImageStep(ImageOp::Grayscale)));

    let menu = crate::commands::image_menu_items(plain.image_edit.as_ref().unwrap());
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
    let grouped: Vec<Option<&str>> = menu.iter().map(|(cmd, _)| menu_group(&cmd.id)).collect();
    assert_eq!(grouped[..6], [Some("Resize"); 6]);
    assert!(grouped[6..].iter().all(Option::is_none));
    assert_eq!(
        menu[3].0.id,
        CommandId::ImageStep(ImageOp::Resize(Resize::Longest(1024)))
    );

    let edited = edited_picture(1, &["Original", "Resized to 1024×768", "Grayscale"]);
    let bar = VersionBar::of(&edited).expect("capsule");
    assert_eq!(bar.title(), "v2/3");
    assert_eq!(
        undo_key("z", true, false, false, Some(&bar)),
        Some(CommandId::TableUndo)
    );
    let menu = crate::commands::image_menu_items(edited.image_edit.as_ref().unwrap());
    let tail: Vec<(&str, &str)> = menu[12..]
        .iter()
        .map(|(cmd, _)| (cmd.title.as_str(), cmd.detail.as_str()))
        .collect();
    assert_eq!(
        tail,
        [
            ("Undo Image Step", "Resized to 1024×768"),
            ("Redo Image Step", "Grayscale")
        ]
    );
    // The steps are found by search too.
    assert!(
        search_pool(&edited)
            .iter()
            .any(|cmd| cmd.id == CommandId::ImageStep(ImageOp::FlipVertical))
    );
    // Only in the picture view.
    let mut info = edited.clone();
    info.view = CardView::Info;
    assert_eq!(VersionBar::of(&info), None);
    // A picture without versions (the clipboard's, not in history) has no Image ▾.
    let mut bare = edited_picture(0, &["Original"]);
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
        Some(CommandId::TableUndo)
    );
    // Nothing to redo at the newest version; the key is left alone.
    assert_eq!(undo_key("Z", true, true, false, Some(&bar)), None);
    let back = VersionBar { current: 0, ..bar };
    assert_eq!(
        undo_key("Z", true, true, false, Some(&back)),
        Some(CommandId::TableRedo)
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
    assert_eq!(open[0].title, format!("• {phrase}"));
    let mut long = "a".repeat(90);
    long.push_str("unique-needle");
    input.history[0].search = Zeroizing::new(long);
    input.history[0].pinned = false;
    let hit = matching(&search_pool(&input), "unique-needle");
    assert_eq!(ids(&hit), vec![CommandId::History(1)]);
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
