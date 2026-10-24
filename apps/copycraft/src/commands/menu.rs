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
pub const OPEN_TABLE_TITLE: &str = "Open Table";

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
        CommandId::Ai(action) => ai_submenu(*action),
        // The functions of one code action sit in a submenu named after it.
        CommandId::AiFn(action, _) => Some(ai_title(*action)),
        CommandId::TextCase(_) => Some(crate::locale::t("case_menu")),
        CommandId::TextEncode(_) => Some(crate::locale::t("encoding_menu")),
        CommandId::TextHash(_) => Some(crate::locale::t("hash_menu")),
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

/// VoiceOver name of a menu chip (*Text ▾*, *Image ▾*, *Table ▾*): its title without the
/// "▾", which the menu-button role already says. `None` for every other command.
pub fn menu_chip_label(cmd: &Command) -> Option<&str> {
    matches!(
        cmd.id,
        CommandId::TextMenu | CommandId::ImageMenu | CommandId::TableMenu
    )
    .then(|| cmd.title.trim_end_matches('▾').trim_end())
}

/// The "Image ▾" chip.
pub fn image_menu_title() -> &'static str {
    crate::locale::t("image_menu")
}
/// The "Text ▾" chip.
pub fn text_menu_title() -> &'static str {
    crate::locale::t("text_menu")
}

/// A section of the Text ▾ menu: a header on macOS 14+, a separator before that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSection {
    /// First and last lines, reverse, column, filter.
    Lines,
    /// Case style, Clean URL, Format JSON.
    Convert,
    /// Complexity and the AGENTS.md steps.
    Analyse,
}

impl TextSection {
    /// The header title, NL/EN.
    pub fn title(self) -> Option<&'static str> {
        match self {
            TextSection::Lines => Some(crate::locale::t("text_section_lines")),
            TextSection::Convert => Some(crate::locale::t("text_section_convert")),
            TextSection::Analyse => Some(crate::locale::t("text_section_analyse")),
        }
    }
}

/// The Text ▾ section of `id`; `None` for a command that is not in that menu.
pub fn text_section(id: &CommandId) -> Option<TextSection> {
    match id {
        CommandId::TextHead
        | CommandId::TextTail
        | CommandId::TextReverse
        | CommandId::TextColumn
        | CommandId::TextFilter => Some(TextSection::Lines),
        CommandId::TextCase(_)
        | CommandId::TextEncode(_)
        | CommandId::TextHash(_)
        | CommandId::TextCleanUrl
        | CommandId::TextJson => Some(TextSection::Convert),
        CommandId::AgentCommands
        | CommandId::AgentOutline
        | CommandId::AgentCheck
        | CommandId::TextComplexity => Some(TextSection::Analyse),
        _ => None,
    }
}

/// The sections `items` fall into, one per item, when they are the Text ▾ menu (it starts
/// with a line step); `None` for the table and image menus, which keep no headers.
pub fn text_sections(items: &[CommandId]) -> Option<Vec<Option<TextSection>>> {
    let first = items.first()?;
    (text_section(first) == Some(TextSection::Lines))
        .then(|| items.iter().map(text_section).collect())
}

/// The "Text ▾" menu, in [`TextSection`]s: first and last lines, reverse, column and filter;
/// case, Clean URL and Format JSON; Complexity and the AGENTS.md steps. Each step makes a new
/// copy; ⌘Z shows the copy the step was taken on. No check marks. Line count and size stay on the meta
/// line under the preview, so the menu does not count again.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn text_menu_items(edit: &TextShown) -> Vec<(Command, bool)> {
    let mut commands = vec![
        command(
            CommandId::TextHead,
            crate::locale::t("text_head"),
            crate::locale::t("text_section_lines"),
            "head first lines top text",
        ),
        command(
            CommandId::TextTail,
            crate::locale::t("text_tail"),
            crate::locale::t("text_section_lines"),
            "tail last lines bottom text",
        ),
        command(
            CommandId::TextReverse,
            crate::locale::t("text_reverse"),
            crate::locale::t("text_section_lines"),
            "tac reverse lines order text",
        ),
        command(
            CommandId::TextColumn,
            crate::locale::t("text_column"),
            crate::locale::t("text_section_lines"),
            "awk column field cut separator text",
        ),
        command(
            CommandId::TextFilter,
            crate::locale::t("text_filter"),
            crate::locale::t("text_section_lines"),
            "grep filter regex regular expression match lines text",
        ),
    ];
    // Case style › camelCase, …: a submenu ([`menu_group`]).
    commands.extend(
        crate::text_ops::case_style::CaseStyle::ALL
            .iter()
            .map(|style| {
                command(
                    CommandId::TextCase(*style),
                    style.title(),
                    crate::locale::t("detail_case"),
                    "case style camel pascal snake kebab constant upper lower identifier rename",
                )
            }),
    );
    // Encoding › the four encodes, then the four decodes (a separator between them,
    // [`separator_before`]): a submenu too.
    let (encodes, decodes): (Vec<_>, Vec<_>) = crate::text_ops::encoding::Codec::ALL
        .into_iter()
        .partition(|codec| codec.encodes());
    commands.extend(encodes.iter().chain(&decodes).map(|codec| {
        command(
            CommandId::TextEncode(*codec),
            codec.title(),
            crate::locale::t("detail_encoding"),
            "encode decode base64 base64url url percent escape hex bytes",
        )
    }));
    // Then SHA-256, SHA-1 and MD5 in a Hash submenu of their own, also under Convert.
    commands.extend(crate::text_ops::hash::HashKind::ALL.iter().map(|kind| {
        command(
            CommandId::TextHash(*kind),
            kind.title(),
            kind.detail(),
            "hash digest checksum sha sha256 sha-256 sha1 sha-1 md5 hex",
        )
    }));
    if edit.tracking {
        commands.push(clean_url_command());
    }
    if edit.json {
        commands.push(command(
            CommandId::TextJson,
            crate::locale::t("text_json"),
            crate::locale::t("detail_json_style"),
            "json format pretty print compress minify one line sort keys indent",
        ));
    }
    if edit.agent {
        commands.push(command(
            CommandId::AgentCommands,
            crate::locale::t("agent_commands"),
            crate::locale::t("detail_agent_commands"),
            "agent commands shell bash cargo npm make build test",
        ));
        commands.push(command(
            CommandId::AgentOutline,
            crate::locale::t("agent_outline"),
            crate::locale::t("detail_agent_outline"),
            "agent outline headings table of contents",
        ));
        commands.push(command(
            CommandId::AgentCheck,
            crate::locale::t("agent_check"),
            crate::locale::t("detail_agent_check"),
            "agent check lint warnings secrets links",
        ));
    }
    if edit.code {
        commands.push(command(
            CommandId::TextComplexity,
            crate::locale::t("text_complexity"),
            crate::locale::t("detail_complexity"),
            "complexity cognitive sonar functions methods score code",
        ));
    }
    commands
        .into_iter()
        .map(|command| (command, false))
        .collect()
}

/// Clean URL…: in Text ▾ and as a chip on a link card, when a URL holds a tracking parameter.
pub(crate) fn clean_url_command() -> Command {
    command(
        CommandId::TextCleanUrl,
        crate::locale::t("text_clean_url"),
        crate::locale::t("detail_url_clean"),
        "clean url link tracking utm fbclid gclid parameters query strip",
    )
}

/// The command a pick in a popped menu runs: the item at `tag`, but only when its title is
/// the one the menu showed. A list replaced while the menu was open (a refresh, another menu)
/// must not run a neighbour's step, e.g. Flip Horizontal for Rotate left: then the title
/// decides, and `None` when no item has it.
pub fn popped_pick(popped: &[Command], tag: isize, title: &str) -> Option<Command> {
    let at = usize::try_from(tag).ok();
    at.and_then(|at| popped.get(at))
        .filter(|cmd| cmd.title == title)
        .or_else(|| popped.iter().find(|cmd| cmd.title == title))
        .cloned()
}

/// Resize › Custom….
pub fn custom_resize_title() -> &'static str {
    crate::locale::t("custom_resize")
}

/// The "Image ▾" menu: Resize (a submenu), then the other steps. Each makes a new copy; ⌘Z
/// shows the copy the step was taken on. No check marks.
pub fn image_menu_items() -> Vec<(Command, bool)> {
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
            CommandId::StepBack,
            crate::locale::t("undo_table_step"),
            &table.labels[table.version],
            "undo table version back",
        ));
    }
    if table.can_redo() {
        commands.push(command(
            CommandId::StepForward,
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

/// The table steps menu (window Table ▾ and command search): Remove Duplicate Rows, Remove
/// empty rows and columns, then, once the table is read, Choose Columns… (more than one
/// column), Sort Ascending › and Sort Descending ›. Open table is the card chip. Undo and
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

/// Rows of the Complexity menu shown at most; the rest are summed up in one line.
pub const COMPLEXITY_ROWS: usize = 300;

/// The Complexity menu's rows, highest first: "18  parse  ·  r. 12–40  ·  hoog", each going to
/// its first line, with its band for the colour. The band is in the text too, so colour is
/// not the only signal.
pub fn complexity_rows(
    scores: &[crate::complexity::FnScore],
) -> Vec<(Command, crate::complexity::Band)> {
    use crate::complexity::Band;
    scores
        .iter()
        .take(COMPLEXITY_ROWS)
        .map(|f| {
            let band = Band::of(f.score);
            let lines = crate::locale::tf("complexity_lines", &[&f.first_line, &f.last_line]);
            let mut title = format!("{}  {}  ·  {lines}", f.score, f.name);
            match band {
                Band::Normal => {}
                Band::High => {
                    title.push_str("  ·  ");
                    title.push_str(crate::locale::t("complexity_high"));
                }
                Band::VeryHigh => {
                    title.push_str("  ·  ");
                    title.push_str(crate::locale::t("complexity_very_high"));
                }
            }
            (
                command(
                    CommandId::GotoLine(f.first_line),
                    &title,
                    crate::locale::t("detail_complexity_row"),
                    "complexity function",
                ),
                band,
            )
        })
        .collect()
}

/// The lines under the rows: how many more there are, then total and average; or that there
/// are no functions.
pub fn complexity_footer(scores: &[crate::complexity::FnScore]) -> Vec<String> {
    if scores.is_empty() {
        return vec![crate::locale::t("complexity_none").to_string()];
    }
    let mut lines = Vec::new();
    if scores.len() > COMPLEXITY_ROWS {
        lines.push(crate::locale::tf(
            "complexity_more",
            &[&(scores.len() - COMPLEXITY_ROWS)],
        ));
    }
    let (sum, average) = crate::complexity::total(scores);
    let average = format!("{average:.1}");
    let average = if crate::locale::lang() == crate::locale::Lang::Nl {
        average.replace('.', ",")
    } else {
        average
    };
    lines.push(crate::locale::tf(
        "complexity_total",
        &[&sum, &average, &scores.len()],
    ));
    lines
}

/// The Outline menu: each heading, indented by level, going to its line.
pub fn agent_outline_rows(
    headings: &[crate::agent::Heading],
) -> Vec<(Command, crate::complexity::Band)> {
    headings
        .iter()
        .take(COMPLEXITY_ROWS)
        .map(|h| {
            let indent = "    ".repeat(h.level.saturating_sub(1));
            let lines = crate::locale::tf("agent_line", &[&h.line]);
            let title = format!("{indent}{}  ·  {lines}", h.text);
            (
                command(
                    CommandId::GotoLine(h.line),
                    &title,
                    crate::locale::t("detail_complexity_row"),
                    "outline heading",
                ),
                crate::complexity::Band::Normal,
            )
        })
        .collect()
}

/// The Check menu: each warning; one about a line goes there. A possible secret shows as
/// very high, the rest as high (the kind is in the words too).
pub fn agent_check_rows(
    warnings: &[crate::agent::Warning],
) -> Vec<(Command, crate::complexity::Band)> {
    use crate::agent::Warning;
    use crate::complexity::Band;
    warnings
        .iter()
        .take(COMPLEXITY_ROWS)
        .map(|w| {
            let band = match w {
                Warning::Secret { .. } => Band::VeryHigh,
                _ => Band::High,
            };
            let id = w.line().map_or(CommandId::TextMenu, CommandId::GotoLine);
            (
                command_enabled(
                    id,
                    &w.title(),
                    crate::locale::t("detail_complexity_row"),
                    "check warning",
                    w.line().is_some(),
                ),
                band,
            )
        })
        .collect()
}

/// What the Complexity menu says instead of scores.
pub fn complexity_refused(refused: crate::complexity::Refused) -> String {
    match refused {
        crate::complexity::Refused::TooLarge => crate::locale::t("complexity_too_large"),
        crate::complexity::Refused::Failed => crate::locale::t("complexity_failed"),
    }
    .to_string()
}
