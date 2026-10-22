//! Command chips offered for the current subject.

use super::*;

/// Actions for the thing on the clipboard. Housekeeping stays in [`overflow`].
pub fn chips(data: &LaunchData) -> Vec<Command> {
    let mut chips = match data.subject_kind {
        SubjectKind::Image => image_chips(
            data.image_scan.as_ref(),
            data.image_edit.is_some(),
            data.view == CardView::Ocr,
        ),
        SubjectKind::Text => {
            let mut chips = copied_text_chips(data.subject_text.as_deref().unwrap_or(""));
            // A drawn SVG: one toggle between its picture and its code, like OCR's.
            if data.picture.is_some()
                && data
                    .subject_text
                    .as_deref()
                    .is_some_and(|text| format::detect(text) == FormatKind::Svg)
            {
                chips.insert(0, svg_toggle(data.view == CardView::Format));
            }
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
            // Lines… on every text and code card, also formatted, and on an opened or dropped
            // file. It does not wait for versions to be attached: a Rust card has no other chip,
            // so gating it on `text_edit` left the row empty. Tables and links have their own chips.
            if data.subject_text.as_deref().is_some_and(offers_text_menu) {
                // Up front, before the less used chips.
                chips.insert(
                    0,
                    command(
                        CommandId::TextMenu,
                        text_menu_title(),
                        crate::locale::t("detail_text_steps"),
                        "text lines head tail reverse column filter undo redo",
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
    chips
}

/// "Diff" up front (after Visit on a link).
pub(crate) fn insert_diff_chip(chips: &mut Vec<Command>) {
    let diff = command(
        CommandId::Diff,
        crate::locale::t("chip_diff"),
        crate::locale::t("detail_diff"),
        "diff compare",
    );
    let at = chips
        .iter()
        .position(|cmd| cmd.id == CommandId::Visit)
        .map_or(0, |index| index + 1);
    chips.insert(at, diff);
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

/// The Lines… menu for `data`: the versions already attached, or a fresh one when the text
/// offers the menu and nothing has attached versions yet. `None` for a table, a link or a
/// picture, so the chip and the menu stay the same thing.
pub fn text_shown(data: &LaunchData) -> Option<TextShown> {
    if data.subject_kind != SubjectKind::Text {
        return None;
    }
    let text = data.subject_text.as_deref()?;
    if !offers_text_menu(text) {
        return None;
    }
    Some(data.text_edit.clone().unwrap_or_else(|| {
        let kind = crate::format::detect(text);
        TextShown {
            labels: vec![crate::locale::t("original").to_string()],
            version: 0,
            note: None,
            code: crate::complexity::Lang::of(kind).is_some(),
            agent: kind == crate::format::FormatKind::Agent,
            tracking: crate::text_ops::url_clean::has_tracking(text),
        }
    }))
}

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
    if let Some(edit) = text_shown(data)
        && commands.iter().any(|c| c.id == CommandId::TextMenu)
    {
        commands.extend(
            text_menu_items(&edit)
                .into_iter()
                .map(|(command, _)| command)
                .filter(|command| command.id != CommandId::AiUnavailable),
        );
    }
    if commands.iter().any(|c| c.id == CommandId::TextMenu) {
        // The AI chip's actions, found by search too (not the note on why they are off).
        commands.extend(
            ai_menu_items(mac_ai::availability(), ai_actions(data))
                .into_iter()
                .filter(|command| command.id != CommandId::AiUnavailable),
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
    // A page link with a query or escapes: the URL decoder after Visit (which
    // stays first, the default action). Not on a YouTube video, whose query is its id.
    if crate::youtube::video_id(text).is_none()
        && let Some(found) = decode::detect(text)
    {
        commands.push(decoder_command(found));
    }
    // Tracking parameters to remove: Clean URL… (a preview, then Apply to clipboard).
    if crate::text_ops::url_clean::has_tracking(text) {
        commands.push(clean_url_command());
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

pub(crate) fn image_chips(scan: Option<&ImageScan>, edits: bool, on_text: bool) -> Vec<Command> {
    let mut modes = vec![command(
        CommandId::Info,
        crate::locale::t("chip_info"),
        crate::locale::t("detail_info"),
        "info data url size",
    )];
    if let Some(scan) = scan {
        // After text recognition one toggle between the text and the picture, named after
        // where it goes (no Original chip, no clash with the Image ▾ menu).
        if scan.ocr.is_some() {
            modes.push(ocr_toggle(on_text));
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
        let mut menu = command(
            CommandId::ImageMenu,
            image_menu_title(),
            crate::locale::t("detail_image_steps"),
            "image picture resize rotate flip grayscale metadata undo redo",
        );
        // Picture steps only on the picture: on the recognized text Image ▾ is off, no switch.
        if on_text {
            menu.enabled = false;
            menu.detail = crate::locale::t("image_view_only").to_string();
        }
        modes.push(menu);
    }
    modes
}

/// The SVG toggle: on the code it goes to the picture (`Original`), on the picture to the
/// code (`Format`).
pub(crate) fn svg_toggle(on_code: bool) -> Command {
    if on_code {
        command(
            CommandId::Original,
            crate::locale::t("show_image"),
            crate::locale::t("detail_svg_picture"),
            "svg picture image drawing",
        )
    } else {
        command(
            CommandId::Format,
            crate::locale::t("show_code"),
            crate::locale::t("detail_svg_code"),
            "svg code xml source",
        )
    }
}

/// The OCR toggle: on the recognized text it goes to the picture (`Original`), on the picture
/// to the text (`Ocr`).
pub(crate) fn ocr_toggle(on_text: bool) -> Command {
    if on_text {
        command(
            CommandId::Original,
            crate::locale::t("show_image"),
            crate::locale::t("detail_original_image"),
            "original source picture image",
        )
    } else {
        command(
            CommandId::Ocr,
            crate::locale::t("show_text"),
            crate::locale::t("detail_ocr"),
            "text ocr recognize",
        )
    }
}

/// The view chips of a text card. There is no chip back to the copy as it came: the card always
/// shows the formatted view, and the active chip clicked again goes back to it.
pub(crate) fn finish_modes(modes: Vec<Command>) -> Vec<Command> {
    modes
}

/// Image ▾ steps run only on a picture view, never on the recognized text.
pub fn image_steps_allowed(view: CardView) -> bool {
    view != CardView::Ocr
}
