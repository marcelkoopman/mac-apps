//! Table and image menu items and titles.

use super::*;

/// Title of the chip that shows a YAML entry as JSON.
pub fn to_json_title() -> &'static str {
    crate::locale::t("chip_to_json")
}

/// Title of the chip that opens the table's menu.
#[allow(dead_code)] // Window toolbar still says Table ▾ via locale directly where needed.
pub fn table_menu_title() -> &'static str {
    crate::locale::t("table_menu")
}
/// English title of the window's Table ▾ control.
#[allow(dead_code)]
pub const TABLE_MENU_TITLE: &str = "Table ▾";

/// Card chip that opens the table window ([`CommandId::TableOpenWindow`]).
pub fn open_table_title() -> &'static str {
    crate::locale::t("open_table")
}
/// English title kept for tests that pin the default language.
#[cfg_attr(not(test), allow(dead_code))]
pub const OPEN_TABLE_TITLE: &str = "Open table";

/// The steps a table can take, for the "Table ▾" menu and the search.
pub fn table_steps() -> Vec<Command> {
    crate::table::TableOp::ONE_CLICK
        .iter()
        .map(table_step_command)
        .collect()
}

pub(crate) fn table_step_command(op: &crate::table::TableOp) -> Command {
    command(
        CommandId::TableStep(op.clone()),
        &op.title(),
        op.group().unwrap_or(crate::locale::t("table_view")),
        op.keywords(),
    )
}

/// The submenu a table menu item sits in ([`crate::table::TableOp::group`]).
pub fn menu_group(id: &CommandId) -> Option<&'static str> {
    match id {
        CommandId::TableStep(op) => op.group(),
        CommandId::ImageStep(op) => op.group(),
        CommandId::ImageResizeCustom => Some(crate::image_edit::resize_group()),
        _ => None,
    }
}

/// The date question: when a date column fits both orders, a command to read it the other
/// way ("Dates are mm/dd/yyyy").
pub(crate) fn date_order_command(notes: &dataframe::ReadNotes) -> Option<Command> {
    if !notes.ambiguous_dates {
        return None;
    }
    let (title, month_first) = if notes.month_first {
        (crate::locale::t("dates_are_dmy"), false)
    } else {
        (crate::locale::t("dates_are_mdy"), true)
    };
    Some(command(
        CommandId::TableDateOrder(month_first),
        title,
        crate::locale::t("dates_fit_both"),
        "dates order day month us european ambiguous",
    ))
}

/// The "Image ▾" chip.
pub fn image_menu_title() -> &'static str {
    crate::locale::t("image_menu")
}
/// The "Text…" chip.
pub fn text_menu_title() -> &'static str {
    crate::locale::t("text_menu")
}

/// The "Text…" menu: count, first and last lines, reverse, column, then undo and redo when
/// there is a version to go to. No check marks.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn text_menu_items(edit: &TextShown) -> Vec<(Command, bool)> {
    let mut commands = vec![
        command(
            CommandId::TextCount,
            crate::locale::t("text_count"),
            crate::locale::t("text_menu"),
            "count wc lines words characters text",
        ),
        command(
            CommandId::TextHead,
            crate::locale::t("text_head"),
            crate::locale::t("text_menu"),
            "head first lines top text",
        ),
        command(
            CommandId::TextTail,
            crate::locale::t("text_tail"),
            crate::locale::t("text_menu"),
            "tail last lines bottom text",
        ),
        command(
            CommandId::TextReverse,
            crate::locale::t("text_reverse"),
            crate::locale::t("text_menu"),
            "tac reverse lines order text",
        ),
        command(
            CommandId::TextColumn,
            crate::locale::t("text_column"),
            crate::locale::t("text_menu"),
            "awk column field cut separator text",
        ),
        command(
            CommandId::TextFilter,
            crate::locale::t("text_filter"),
            crate::locale::t("text_menu"),
            "grep filter regex regular expression match lines text",
        ),
    ];
    if edit.can_undo() {
        commands.push(command(
            CommandId::TableUndo,
            crate::locale::t("undo_text_step"),
            &edit.labels[edit.version],
            "undo text version back",
        ));
    }
    if edit.can_redo() {
        commands.push(command(
            CommandId::TableRedo,
            crate::locale::t("redo_text_step"),
            &edit.labels[edit.version + 1],
            "redo text version forward",
        ));
    }
    commands
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

/// Resize › Custom….
pub fn custom_resize_title() -> &'static str {
    crate::locale::t("custom_resize")
}

/// The "Image ▾" menu: Resize (a submenu), the other steps, then undo and redo when there is
/// a version to go to. No check marks.
pub fn image_menu_items(edit: &ImageShown) -> Vec<(Command, bool)> {
    use crate::image_edit::ImageOp;
    let step = |op: &ImageOp| {
        let detail = op.group().unwrap_or(crate::locale::t("title_image"));
        command(
            CommandId::ImageStep(*op),
            &op.title(),
            detail,
            image_keywords(op),
        )
    };
    let (resizes, others): (Vec<ImageOp>, Vec<ImageOp>) =
        ImageOp::MENU.iter().partition(|op| op.group().is_some());
    let mut commands: Vec<Command> = resizes.iter().map(step).collect();
    commands.push(command(
        CommandId::ImageResizeCustom,
        custom_resize_title(),
        crate::image_edit::resize_group(),
        "resize custom width height size image",
    ));
    commands.extend(others.iter().map(step));
    if edit.can_undo() {
        commands.push(command(
            CommandId::TableUndo,
            crate::locale::t("undo_image_step"),
            &edit.labels[edit.version],
            "undo image version back",
        ));
    }
    if edit.can_redo() {
        commands.push(command(
            CommandId::TableRedo,
            crate::locale::t("redo_image_step"),
            &edit.labels[edit.version + 1],
            "redo image version forward",
        ));
    }
    commands
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

pub(crate) fn image_keywords(op: &crate::image_edit::ImageOp) -> &'static str {
    use crate::image_edit::ImageOp;
    match op {
        ImageOp::Resize(_) => "resize smaller scale size image",
        ImageOp::RotateLeft | ImageOp::RotateRight => "rotate turn image",
        ImageOp::FlipHorizontal | ImageOp::FlipVertical => "flip mirror image",
        ImageOp::RemoveMetadata => "remove metadata exif gps strip image",
        ImageOp::Grayscale => "grayscale black white gray image",
    }
}

/// Undo and redo of a table step, for the search when there is a version to go to (⌘Z, ⇧⌘Z
/// and the version capsule have them too; the Table ▾ menu does not).
pub(crate) fn table_undo_redo(table: &TableShown) -> Vec<Command> {
    let mut commands = Vec::new();
    if table.can_undo() {
        commands.push(command(
            CommandId::TableUndo,
            crate::locale::t("undo_table_step"),
            &table.labels[table.version],
            "undo table version back",
        ));
    }
    if table.can_redo() {
        commands.push(command(
            CommandId::TableRedo,
            crate::locale::t("redo_table_step"),
            &table.labels[table.version + 1],
            "redo table version forward",
        ));
    }
    commands
}

/// The "Table ▾" menu with check marks ([`table_menu`]; none is checked).
pub fn table_menu_items(table: Option<&TableShown>) -> Vec<(Command, bool)> {
    table_menu(table)
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

/// The table steps menu (window Table ▾ and command search): Remove duplicate rows, Remove
/// empty rows and columns, then, once the table is read, Choose columns… (more than one
/// column), Sort ascending › and Sort descending ›. Open table is the card chip. Undo and
/// redo are ⌘Z and ⇧⌘Z and the version capsule.
pub fn table_menu(table: Option<&TableShown>) -> Vec<Command> {
    let mut commands = table_steps();
    let Some(frame) = table.and_then(|table| table.frame.as_ref()) else {
        return commands;
    };
    if frame.width() > 1 {
        commands.push(command(
            CommandId::TableChooseColumns,
            choose_columns_title(),
            crate::locale::t("choose_columns_detail"),
            "choose pick select keep remove columns table",
        ));
    }
    let columns: Vec<String> = frame
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    commands.extend(
        crate::table::TableOp::column_steps(&columns)
            .iter()
            .map(table_step_command),
    );
    // Open table is the card chip ([`open_table_title`]), not a menu item.
    commands
}

/// The Table ▾ item that opens the column picker.
pub fn choose_columns_title() -> &'static str {
    crate::locale::t("choose_columns_title")
}

/// The Table ▾ item that opens the table window.
#[allow(dead_code)] // Was the Table ▾ item; Open table is the card chip now.
pub fn open_window_title() -> &'static str {
    crate::locale::t("open_in_window")
}
