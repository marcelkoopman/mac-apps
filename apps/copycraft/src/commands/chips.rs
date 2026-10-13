//! Command chips offered for the current subject.

use super::*;

/// Actions for the thing on the clipboard. Housekeeping stays in [`overflow`].
pub fn chips(data: &LaunchData) -> Vec<Command> {
    let mut chips = match data.subject_kind {
        SubjectKind::Image => image_chips(data.image_scan.as_ref(), data.image_edit.is_some()),
        SubjectKind::Text => {
            let mut chips = copied_text_chips(data.subject_text.as_deref().unwrap_or(""));
            // The date question, in the Dataframe view while dates fit both orders.
            if data.view == CardView::Dataframe
                && let Some(question) = data
                    .table
                    .as_ref()
                    .and_then(|table| table.notes)
                    .and_then(|notes| date_order_command(&notes))
            {
                chips.push(question);
            }
            // Lines… on text and code cards, also in a formatted view; tables and links have
            // their own chips.
            if data.text_edit.is_some()
                && data.subject_text.as_deref().is_some_and(offers_text_menu)
            {
                // Up front (after Original), before the less used chips.
                let at = chips
                    .iter()
                    .position(|cmd| cmd.id == CommandId::Original)
                    .map_or(0, |index| index + 1);
                chips.insert(
                    at,
                    command(
                        CommandId::TextMenu,
                        text_menu_title(),
                        crate::locale::t("detail_text_steps"),
                        "text lines count head tail reverse column filter undo redo",
                    ),
                );
            }
            chips
        }
        SubjectKind::Empty
        | SubjectKind::NoText
        | SubjectKind::Hidden
        | SubjectKind::Denied
        | SubjectKind::Pending
        | SubjectKind::PasteAsk => Vec::new(),
    };
    // Not part of the text memo: the chip follows the two chosen copies, not the text.
    if data.diff.is_some() {
        insert_diff_chip(&mut chips);
    }
    // A card in another view (a table version from search) always has the way back.
    if data.subject_kind == SubjectKind::Text
        && !matches!(data.view, CardView::Original | CardView::Diff)
        && !chips.iter().any(|cmd| cmd.id == CommandId::Original)
    {
        chips.insert(0, original_command());
    }
    chips
}

/// "Diff" next to Original. A card without Original (plain text, or a link) gets both.
pub(crate) fn insert_diff_chip(chips: &mut Vec<Command>) {
    let diff = command(
        CommandId::Diff,
        crate::locale::t("chip_diff"),
        crate::locale::t("detail_diff"),
        "diff compare",
    );
    if let Some(at) = chips.iter().position(|cmd| cmd.id == CommandId::Original) {
        chips.insert(at + 1, diff);
        return;
    }
    let at = chips
        .iter()
        .position(|cmd| cmd.id == CommandId::Visit)
        .map_or(0, |index| index + 1);
    chips.insert(at, original_command());
    chips.insert(at + 1, diff);
}

/// The chips of a copied text, which follow from the text alone. Working them out tries the
/// formatter, the converters and the table parser, and every card update (each step through
/// history, twice through [`search_pool`]) asks again, so they are remembered for the last
/// copies, whatever their length. Only the text's hash is kept with them; the chips are fixed
/// labels. [`forget_chips`] drops them (Wipe).
pub(crate) static TEXT_CHIPS: crate::memo::Memo<Vec<Command>> =
    crate::memo::Memo::with_min_len(crate::clipboard::MAX_HISTORY + 4, 0);

pub(crate) fn copied_text_chips(text: &str) -> Vec<Command> {
    TEXT_CHIPS.get_or_compute(text, |text| {
        if crate::youtube::video_id(text).is_some() || crate::page_preview::page_url(text).is_some()
        {
            link_chips(text)
        } else {
            text_chips(text)
        }
    })
}

/// Whether the Lines… menu belongs on a copied text: prose and every code card (Rust, Python,
/// JSON, XML, Markdown, HTML…), also when code happens to parse as a table (commas in a
/// function call); not a real table (CSV, TSV, or plain text that reads as one) and not a link.
/// Remembered per text like the chips.
pub fn offers_text_menu(text: &str) -> bool {
    TEXT_MENU.get_or_compute(text, |text| {
        if crate::youtube::video_id(text).is_some() || crate::page_preview::page_url(text).is_some()
        {
            return false;
        }
        let kind = crate::format::detect(text);
        match kind {
            FormatKind::Csv | FormatKind::Tsv | FormatKind::Dataframe => false,
            FormatKind::Plain | FormatKind::Text | FormatKind::Url => {
                !crate::toolbar_visibility::shows_dataframe_button(kind, text)
            }
            _ => true,
        }
    })
}

static TEXT_MENU: crate::memo::Memo<bool> =
    crate::memo::Memo::with_min_len(crate::clipboard::MAX_HISTORY + 4, 0);

/// Work out (and remember) the chips of a copied text ahead of the card, off the main thread.
pub fn warm_chips(text: &str) {
    copied_text_chips(text);
}

/// Drop the remembered chips (Wipe).
pub fn forget_chips() {
    TEXT_CHIPS.clear();
    TEXT_MENU.clear();
}

/// Chips, earlier copies, and appearance. Quit stays out.
pub fn search_pool(data: &LaunchData) -> Vec<Command> {
    let mut commands = chips(data);
    let offers_table = commands.iter().any(|c| c.id == CommandId::TableOpenWindow);
    if offers_table || data.table.is_some() {
        commands.extend(table_menu(data.table.as_ref()));
        commands.extend(data.table.as_ref().map(table_undo_redo).unwrap_or_default());
    }
    if let Some(edit) = &data.image_edit {
        commands.extend(
            image_menu_items(edit)
                .into_iter()
                .map(|(command, _)| command),
        );
    }
    if let Some(edit) = &data.text_edit
        && commands.iter().any(|c| c.id == CommandId::TextMenu)
    {
        commands.extend(
            text_menu_items(edit)
                .into_iter()
                .map(|(command, _)| command),
        );
    }
    if content_actions(data).preview {
        commands.push(preview_command());
    }
    for item in &data.history {
        let keywords = history_keywords(item);
        commands.push(command(
            CommandId::History(item.index),
            &history_chip_title(item, data.settings.privacy_filter),
            &item.mark,
            &keywords,
        ));
    }
    commands.extend(entry_commands(data));
    commands.extend(capture_commands(data));
    if let Some(shown) = shown_history(data) {
        commands.push(diff_action(
            CommandId::DiffShown,
            &data.diff_picks,
            shown.id,
            shown.text,
        ));
    }
    if data.diff_picks.len() == 2 {
        commands.push(diff_swap_command());
    }
    for theme in [Theme::System, Theme::Light, Theme::Dark] {
        let name = theme_name(theme);
        let detail = if data.theme == theme {
            crate::locale::t("appearance_current")
        } else {
            crate::locale::t("appearance")
        };
        let keywords = match theme {
            Theme::System => "appearance theme system",
            Theme::Light => "appearance theme light",
            Theme::Dark => "appearance theme dark",
        };
        commands.push(command(
            CommandId::Appearance(theme),
            name,
            detail,
            keywords,
        ));
    }
    commands
}

/// Byte range of 1-based line `line` in `text` (without its line end), for the well's mark.
/// `None` past the last line.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn line_range(text: &str, line: usize) -> Option<std::ops::Range<usize>> {
    let mut start = 0;
    for (index, part) in text.split('\n').enumerate() {
        if index + 1 == line {
            return Some(start..start + part.trim_end_matches('\r').len());
        }
        start += part.len() + 1;
    }
    None
}

pub(crate) fn source_commands(data: &LaunchData) -> Vec<Command> {
    let mut commands = vec![command(
        CommandId::ChooseFile,
        crate::locale::t("menu_choose_file"),
        crate::locale::t("detail_choose_file"),
        "choose file open",
    )];
    if data.source_name.is_some() {
        commands.push(command(
            CommandId::UseClipboard,
            crate::locale::t("title_clipboard"),
            crate::locale::t("detail_show_clipboard"),
            "clipboard pasteboard",
        ));
    }
    commands
}

pub(crate) fn link_chips(text: &str) -> Vec<Command> {
    let mut commands = vec![command(
        CommandId::Visit,
        crate::locale::t("chip_visit"),
        crate::locale::t("detail_visit"),
        "visit open browser",
    )];
    if toolbar_visibility::shows_format(text) {
        commands.push(command(
            CommandId::Format,
            crate::locale::t("chip_format"),
            crate::locale::t("detail_uri"),
            "format url uri",
        ));
    }
    // A page link with a query or escapes: Original and the URL decoder after Visit (which
    // stays first, the default action). Not on a YouTube video, whose query is its id.
    if crate::youtube::video_id(text).is_none()
        && let Some(found) = decode::detect(text)
    {
        commands.push(original_command());
        commands.push(decoder_command(found));
    }
    commands
}

pub(crate) fn decoder_command(found: decode::DecodeKind) -> Command {
    command(
        CommandId::Decode,
        found.label(),
        crate::locale::t("detail_decoded"),
        found.keywords(),
    )
}

pub(crate) fn text_chips(text: &str) -> Vec<Command> {
    let kind = format::detect(text);
    let mut commands = Vec::new();
    // The cheap check first: kinds that open formatted never get the chip, so they skip the
    // formatter (slow on a large copy).
    if !opens_formatted(text) && toolbar_visibility::shows_format(text) {
        commands.push(command(
            CommandId::Format,
            crate::locale::t("chip_format"),
            kind.source_heading(),
            "format pretty print",
        ));
    }
    if kind == FormatKind::Json && crate::validate::broken_json(text).is_none() {
        commands.push(command(
            CommandId::Schema,
            crate::locale::t("chip_schema"),
            crate::locale::t("avro_schema"),
            "schema avro",
        ));
    } else if kind == FormatKind::Xml && crate::xsd_schema::is_xsd(text) {
        if crate::xsd_schema::try_sample(text).is_some() {
            commands.push(command(
                CommandId::Sample,
                crate::locale::t("chip_sample"),
                crate::locale::t("sample_xml"),
                "sample xml example",
            ));
        }
    } else if kind == FormatKind::Xml && crate::xsd_schema::try_schema(text).is_some() {
        commands.push(command(
            CommandId::Schema,
            crate::locale::t("chip_schema"),
            crate::locale::t("xsd_schema"),
            "schema xsd xml",
        ));
    }
    // YAML as JSON: a view of the entry (Copy and Save take it, as `.json`). Never on JSON,
    // which is YAML too but detected as JSON first.
    if kind == FormatKind::Yaml && crate::transform::yaml_to_json(text).is_some() {
        commands.push(command(
            CommandId::Convert,
            to_json_title(),
            crate::locale::t("detail_yaml_as_json"),
            "to json convert yaml",
        ));
    }
    if toolbar_visibility::shows_convert(text) {
        commands.push(command(
            CommandId::Convert,
            crate::locale::t("chip_convert"),
            crate::locale::t("detail_convert"),
            "convert json key value",
        ));
    }
    // One decoder chip, titled with what the copy decodes as (JWT, Base64, URL, …).
    if toolbar_visibility::tries_decode(kind, text)
        && let Some(found) = decode::detect(text)
    {
        commands.push(decoder_command(found));
    }
    if toolbar_visibility::shows_dataframe_button(kind, text) {
        // Opens the table window (no in-card grid). Steps / Choose columns live there.
        commands.push(command(
            CommandId::TableOpenWindow,
            open_table_title(),
            crate::locale::t("detail_table_window"),
            "open window table larger resize sidebar",
        ));
    }
    finish_modes(commands)
}

pub(crate) fn image_chips(scan: Option<&ImageScan>, edits: bool) -> Vec<Command> {
    let mut modes = vec![command(
        CommandId::Info,
        crate::locale::t("chip_info"),
        crate::locale::t("detail_info"),
        "info data url size",
    )];
    if let Some(scan) = scan {
        if scan.ocr.is_some() {
            modes.push(command(
                CommandId::Ocr,
                crate::locale::t("title_text"),
                crate::locale::t("detail_ocr"),
                "text ocr recognize",
            ));
        }
        if scan.qr.is_some() {
            modes.push(command(
                CommandId::Qr,
                crate::locale::t("title_qr"),
                crate::locale::t("detail_qr"),
                "qr barcode",
            ));
        }
    }
    if edits {
        modes.push(command(
            CommandId::ImageMenu,
            image_menu_title(),
            crate::locale::t("detail_image_steps"),
            "image picture resize rotate flip grayscale metadata undo redo",
        ));
    }
    let mut commands = vec![command(
        CommandId::Original,
        crate::locale::t("original"),
        crate::locale::t("detail_original_image"),
        "original source picture",
    )];
    commands.extend(modes);
    commands
}

pub(crate) fn finish_modes(modes: Vec<Command>) -> Vec<Command> {
    let mut commands = Vec::new();
    // Raw only when there is another view on the card to come back from: Open table opens a
    // window, so a CSV card has none.
    if has_other_view(&modes) {
        commands.push(original_command());
    }
    commands.extend(modes);
    commands
}

/// Some chip switches the card to another view of the same text (not Raw itself).
pub(crate) fn has_other_view(chips: &[Command]) -> bool {
    chips
        .iter()
        .any(|cmd| cmd.id != CommandId::Original && CardView::from_command(&cmd.id).is_some())
}

pub(crate) fn original_command() -> Command {
    command(
        CommandId::Original,
        crate::locale::t("chip_raw"),
        crate::locale::t("detail_original"),
        "original source",
    )
}
