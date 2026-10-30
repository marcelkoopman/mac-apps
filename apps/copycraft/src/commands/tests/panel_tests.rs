use super::*;
use crate::commands::{
    MAX_VISIBLE, card_actions, is_card_action, panel_matching, search_pool, shortcut_label,
};

#[test]
fn command_k_lists_the_cards_own_actions_from_the_search_pool() {
    let mut input = data(SubjectKind::Text, Some("{\"naam\": \"Voorbeeld\"}"));
    input.history.push(history_item(0, "JSON", true));
    let mut other = history_item(1, "Text", true);
    other.pinned = true;
    other.snippet = Some("Antwoord".into());
    input.history.push(other);
    input.shown_id = Some(0);
    let pool = search_pool(&input);
    let panel = card_actions(&input, &pool);
    let ids = ids(&panel);
    // Copy and Save from the well lead, Save with its chord.
    assert_eq!(ids[..2], [CommandId::Copy, CommandId::Save]);
    assert!(panel[1].title.ends_with("⌘S"));
    // The card's chips and rows are there, as search has them (one source).
    assert!(ids.contains(&CommandId::Pin));
    assert!(ids.contains(&CommandId::DeleteHistory));
    for id in &ids[2..] {
        assert!(pool.iter().any(|cmd| cmd.id == *id), "{id:?}");
    }
    // Not the card's: history rows, appearance, recording, screenshot, other snippets.
    assert!(ids.iter().all(|id| !matches!(
        id,
        CommandId::History(_)
            | CommandId::Appearance(_)
            | CommandId::Pause
            | CommandId::IgnoreNext
            | CommandId::IgnoreFrontApp
            | CommandId::TakeScreenshot
            | CommandId::SnippetFill(_)
    )));
    // Every id once.
    for id in &ids {
        assert_eq!(ids.iter().filter(|other| *other == id).count(), 1, "{id:?}");
    }
}

#[test]
fn the_snippet_on_screen_is_filled_in_from_command_k() {
    let mut input = data(SubjectKind::Text, Some("Hoi {{naam}}"));
    let mut item = history_item(4, "Text", true);
    item.pinned = true;
    item.snippet = Some("Groet".into());
    input.history.push(item);
    input.shown_id = Some(4);
    let pool = search_pool(&input);
    assert!(ids(&card_actions(&input, &pool)).contains(&CommandId::SnippetFill(4)));
    assert!(!is_card_action(&CommandId::History(4)));
    assert!(is_card_action(&CommandId::Format));
}

#[test]
fn nothing_to_act_on_lists_no_copy_or_save() {
    let input = data(SubjectKind::Empty, None);
    let pool = search_pool(&input);
    let ids = ids(&card_actions(&input, &pool));
    assert!(!ids.contains(&CommandId::Copy));
    assert!(!ids.contains(&CommandId::Save));
}

#[test]
fn shortcuts_come_from_the_menu_chords() {
    assert_eq!(shortcut_label(&CommandId::StepBack).as_deref(), Some("⌘Z"));
    assert_eq!(
        shortcut_label(&CommandId::StepForward).as_deref(),
        Some("⇧⌘Z")
    );
    assert_eq!(
        shortcut_label(&CommandId::ChooseFile).as_deref(),
        Some("⌘O")
    );
    assert_eq!(shortcut_label(&CommandId::Save).as_deref(), Some("⌘S"));
    assert_eq!(shortcut_label(&CommandId::Format), None);
}

#[test]
fn the_list_shows_all_while_empty_and_narrows_as_you_type() {
    let input = data(SubjectKind::Text, Some("{\"a\": 1}"));
    let pool = search_pool(&input);
    let panel = card_actions(&input, &pool);
    let all = panel_matching(&panel, "  ");
    assert_eq!(all.len(), panel.len().min(MAX_VISIBLE));
    assert_eq!(all[0].id, CommandId::Copy);
    let narrowed = panel_matching(&panel, "BEWAAR");
    assert!(narrowed.iter().all(|cmd| cmd.matches("bewaar")));
    assert!(ids(&narrowed).contains(&CommandId::Save));
    assert!(panel_matching(&panel, "zzz-geen-actie").is_empty());
}

/// ⌘K opens the list on the card; Esc, a new search and Wipe end it.
#[test]
fn command_k_is_wired_on_the_card() {
    let card = include_str!("../../macos_launcher/mod.rs");
    let chord = &card[card.find("is_command_chord(event, \"k\")").unwrap()..];
    assert!(chord[..200].contains("toggle_action_panel()"));
    let events = include_str!("../../macos_launcher/events.rs");
    let close = &events[events.find("pub(crate) fn close_search()").unwrap()..];
    assert!(close[..close.find("\n}\n").unwrap()].contains("set_action_scope(false)"));
    let open = &events[events.find("fn open_search_in(").unwrap()..];
    assert!(open[..open.find("\n}\n").unwrap()].contains("set_action_scope(panel)"));
    let wipe = &events[events.find("pub fn wipe_shown()").unwrap()..];
    let wipe = &wipe[..wipe.find("\n}\n").unwrap()];
    assert!(wipe.contains("&PANEL") && wipe.contains("ACTION_SCOPE.set(false)"));
    let window = include_str!("../../macos_launcher/window.rs");
    assert!(window.contains("commands::panel_matching(&slot.borrow(), query.as_str())"));
}
