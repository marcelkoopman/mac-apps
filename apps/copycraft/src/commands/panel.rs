//! ⌘K: every action of the card on screen in one list, typed to filter.

use super::*;

/// What ⌘K lists: Copy and Save from the well ([`content_actions`]), then the card's own
/// commands from search (`pool`, [`search_pool`]: its chips and the Text ▾, Image ▾, Table ▾
/// and AI rows, pin and remove, the diff). History rows, appearance, recording, the screenshot
/// and other pins' snippets are not the card's and stay out. A command with a shortcut has it
/// after its title (⌘S, ⌘Z).
pub fn card_actions(data: &LaunchData, pool: &[Command]) -> Vec<Command> {
    let well = content_actions(data);
    let shown = shown_history(data).map(|item| item.id);
    let mut list = Vec::new();
    if well.copy {
        list.push(command(
            CommandId::Copy,
            crate::locale::t("copy"),
            crate::locale::t("panel_copy_detail"),
            "copy kopieer clipboard klembord",
        ));
    }
    if well.save {
        list.push(command(
            CommandId::Save,
            crate::locale::t("save_card"),
            crate::locale::t("panel_save_detail"),
            "save bewaar file bestand",
        ));
    }
    for cmd in pool {
        let own = match &cmd.id {
            CommandId::SnippetFill(id) => shown == Some(*id),
            id => is_card_action(id),
        };
        if own && !list.iter().any(|known: &Command| known.id == cmd.id) {
            list.push(cmd.clone());
        }
    }
    for cmd in &mut list {
        if let Some(keys) = shortcut_label(&cmd.id) {
            cmd.title = format!("{}  {keys}", cmd.title);
        }
    }
    list
}

/// Whether `id` acts on the card on screen (see [`card_actions`]).
pub fn is_card_action(id: &CommandId) -> bool {
    !matches!(
        id,
        CommandId::History(_)
            | CommandId::Appearance(_)
            | CommandId::Pause
            | CommandId::IgnoreNext
            | CommandId::IgnoreFrontApp
            | CommandId::TakeScreenshot
            | CommandId::SnippetFill(_)
    )
}

/// The chord the card runs for `id`, as drawn: [`menu_key`], plus ⌘S for Save.
pub fn shortcut_label(id: &CommandId) -> Option<String> {
    if *id == CommandId::Save {
        return Some("⌘S".to_string());
    }
    let (key, shift) = menu_key(id)?;
    let shift = if shift { "⇧" } else { "" };
    Some(format!("{shift}⌘{}", key.to_uppercase()))
}

/// The rows ⌘K shows for `query`: all of them while it is empty, else those that match. At
/// most [`MAX_VISIBLE`], like search; typing narrows the rest down.
pub fn panel_matching(actions: &[Command], query: &str) -> Vec<Command> {
    let mut needle = query.trim().to_lowercase();
    let found = actions
        .iter()
        .filter(|cmd| needle.is_empty() || cmd.matches(&needle))
        .take(MAX_VISIBLE)
        .cloned()
        .collect();
    needle.zeroize();
    found
}
