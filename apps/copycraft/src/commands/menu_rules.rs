//! Pure rules for how the popped menus (⋯, Text ▾, Image ▾, Table ▾) are laid out: where a
//! separator goes inside a submenu, which item is an ⌥ alternate of the one before it, and
//! which shortcut a row shows.
//! AppKit only draws what these say, so the rules are tested on every platform.

use super::*;

/// Whether a separator goes before `id` in a submenu whose item before it is `previous`
/// (`None`: `id` is the first). History ▸: Clear History and Clear Pins sit apart from the
/// copies. Encoding ▸: the four decodes apart from the four encodes. A strip card's menu:
/// Remove from History apart from Pin and the diff action.
pub fn separator_before(previous: Option<&CommandId>, id: &CommandId) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let clear = |id: &CommandId| matches!(id, CommandId::ClearHistory | CommandId::ClearPins);
    match (previous, id) {
        (CommandId::TextEncode(before), CommandId::TextEncode(codec)) => {
            before.encodes() && !codec.encodes()
        }
        // A strip card's menu: Remove from History below a line.
        (CommandId::RemoveHistory(_), _) => false,
        (_, CommandId::RemoveHistory(_)) => true,
        _ => clear(id) && !clear(previous),
    }
}

/// Whether `id` is an ⌥ alternate of the item before it (`previous`): it takes that row's
/// place while ⌥ is held. History ▸: Add to Diff for the copy above it.
pub fn is_alternate(previous: Option<&CommandId>, id: &CommandId) -> bool {
    matches!(
        (previous, id),
        (Some(CommandId::History(_)), CommandId::DiffToggle(_))
    )
}

/// The title of an ⌥ alternate: its action and the copy it acts on, as the row it replaces
/// names that copy.
pub fn alternate_title(action: &str, primary: &str) -> String {
    format!("{action}: {primary}")
}

/// The shortcut a menu row shows: its key and whether ⇧ is part of it (⌘ always is). Only a
/// label: the card's own `performKeyEquivalent:` runs these chords whether a menu is open or
/// not (⌘O Choose File, ⌘, Settings, ⌘Y the strip, ⌘Z / ⇧⌘Z [`undo_key`]). While a popped
/// menu is open AppKit gives the chord to that menu, which runs the row once; the card does
/// not see it, and the menu is gone once it closes, so nothing runs twice.
pub fn menu_key(id: &CommandId) -> Option<(&'static str, bool)> {
    match id {
        CommandId::ChooseFile => Some(("o", false)),
        CommandId::Settings => Some((",", false)),
        CommandId::ToggleStrip => Some(("y", false)),
        // Undo / Redo in Text ▾ and Image ▾ (Table ▾ has no undo rows).
        CommandId::TableUndo => Some(("z", false)),
        CommandId::TableRedo => Some(("z", true)),
        _ => None,
    }
}

/// The grid cell under UTF-16 offset `at` of the table window's drawn body (rows on lines,
/// cells after tabs; a numeric first column starts each line with a tab): row and column.
/// `None` past the last line.
pub fn grid_cell_at(body: &str, at: usize, first_numeric: bool) -> Option<(usize, usize)> {
    let (mut row, mut tabs, mut offset) = (0usize, 0usize, 0usize);
    for ch in body.chars() {
        if offset >= at {
            break;
        }
        offset += ch.len_utf16();
        match ch {
            '\n' => {
                row += 1;
                tabs = 0;
            }
            '\t' => tabs += 1,
            _ => {}
        }
    }
    let lines = body.lines().count();
    if row >= lines {
        return None;
    }
    Some((row, tabs.saturating_sub(usize::from(first_numeric))))
}

/// The table window grid's right-click (⌃-click) menu on cell `row`, `col`: Copy Cell, Copy
/// Row, Copy Column. Copy in the toolbar keeps copying the whole table.
pub fn table_cell_menu(row: usize, col: usize) -> Vec<Command> {
    vec![
        command(
            CommandId::TableCopyCell(row, col),
            crate::locale::t("copy_cell"),
            "",
            "",
        ),
        command(
            CommandId::TableCopyRow(row),
            crate::locale::t("copy_row"),
            "",
            "",
        ),
        command(
            CommandId::TableCopyColumn(col),
            crate::locale::t("copy_column"),
            "",
            "",
        ),
    ]
}
