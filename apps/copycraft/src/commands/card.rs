//! Building the work card: masking, content keys and per-subject cards.

use super::*;

/// History row. The type mark and size stand in for the copied text.
pub fn history_label(mark: &str, byte_len: usize) -> String {
    format!("{mark}  {}", format_bytes(byte_len))
}

/// The eye button's choice per history item (its [`content_key`]): `true` hidden, `false`
/// shown. An item is masked by the privacy filter only until a choice is made; then the choice
/// holds when the card opens again, when browsing away and back, and when the app loses focus
/// (the Preview window). In memory only, for the last [`BlurChoices::KEEP`] items.
#[derive(Debug, Default, Clone)]
pub struct BlurChoices {
    items: Vec<(u64, bool)>,
}

impl BlurChoices {
    pub const KEEP: usize = 64;

    pub fn remember(&mut self, key: u64, hidden: bool) {
        self.items.retain(|(seen, _)| *seen != key);
        self.items.push((key, hidden));
        if self.items.len() > Self::KEEP {
            self.items.remove(0);
        }
    }

    pub fn choice(&self, key: u64) -> Option<bool> {
        self.items
            .iter()
            .find(|(seen, _)| *seen == key)
            .map(|(_, hidden)| *hidden)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// `(hidden, revealed)` for item `key`: its choice, else what the card had (`hidden`,
    /// `revealed`).
    pub fn apply(&self, key: u64, hidden: bool, revealed: bool) -> (bool, bool) {
        match self.choice(key) {
            Some(hidden) => (hidden, !hidden),
            None => (hidden, revealed),
        }
    }
}

/// Whether a revealed card stays revealed when it is stored again: while it shows the same
/// history entry (`shown_key` and `next_key` the same [`content_key`]), in all its views, chips,
/// table versions and steps, and when a rebuilt card or its sensitivity labels arrive. Views of
/// one entry can show different labels (a conversion, a version's CSV); a step only removes
/// data, so none of that masks it again. Another entry does; focus loss, Wipe, lock and
/// retention mask it on their own.
pub fn stays_revealed(revealed: bool, shown_key: u64, next_key: u64) -> bool {
    revealed && shown_key == next_key
}

/// The well has copied content, so it stays blurred until it is clicked.
/// Image info is dimensions and a data URL, so the Info chip stays sharp.
pub fn masks_content(card: &WorkCard, view: CardView) -> bool {
    if view == CardView::Info {
        return false;
    }
    // A diff is copied text even while it is still being built (the excerpt is empty then).
    if view == CardView::Diff {
        return true;
    }
    card.shows_image || card.link_page.is_some() || !card.excerpt.is_empty()
}

/// Whether content is masked now. The privacy filter setting is the master switch: with it off
/// nothing is masked, whatever was revealed before. With it on, content that [`masks_content`]
/// stays masked until it is revealed. The card's well and the table window both decide here.
pub fn effective_mask(privacy_filter: bool, content_masks: bool, revealed: bool) -> bool {
    privacy_filter && content_masks && !revealed
}

/// Whether the well is masked, including the card's hide button: `hidden` hides the current
/// entry whatever the privacy filter or blur settings say.
pub fn well_masked(
    privacy_filter: bool,
    content_masks: bool,
    revealed: bool,
    hidden: bool,
) -> bool {
    hidden || effective_mask(privacy_filter, content_masks, revealed)
}

/// The hide/show button was clicked on a well that is `masked`: the new (hidden, revealed).
pub fn toggle_hidden(masked: bool) -> (bool, bool) {
    if masked { (false, true) } else { (true, false) }
}

/// SF Symbol and label key for the hide/show button: eye.slash (Hide) while the content shows,
/// eye (Show) while it is masked.
pub fn hide_button_look(masked: bool) -> (&'static str, &'static str) {
    if masked {
        ("eye", "show_content")
    } else {
        ("eye.slash", "hide_content")
    }
}

/// How a masked well hides the copy. A blur that cannot be installed covers
/// the well and drops the excerpt. The transparent click target is not a mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WellMask {
    pub blur: bool,
    pub shade: bool,
    pub show_body: bool,
}

pub fn well_mask(masked: bool, blur_ready: bool) -> WellMask {
    if !masked {
        WellMask {
            blur: false,
            shade: false,
            show_body: true,
        }
    } else if blur_ready {
        WellMask {
            blur: true,
            shade: false,
            show_body: true,
        }
    } else {
        WellMask {
            blur: false,
            shade: true,
            show_body: false,
        }
    }
}

/// Identity of the copied item. Chips on that item share one reveal.
pub fn content_key(data: &LaunchData) -> u64 {
    use std::hash::{Hash, Hasher};
    // The diff is its own item: revealing the copy on screen does not reveal it, and a swap
    // or a finished result masks it again.
    if data.view == CardView::Diff
        && let Some(diff) = &data.diff
    {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        diff.token.hash(&mut hasher);
        return hasher.finish();
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.subject_kind.hash(&mut hasher);
    data.subject_text.hash(&mut hasher);
    data.source_name.hash(&mut hasher);
    if let Some(edit) = &data.image_edit {
        // Each version of a picture is the same entry, which stays revealed.
        edit.entry.hash(&mut hasher);
    } else {
        if let Some(image) = &data.image {
            image.format.hash(&mut hasher);
            image.width.hash(&mut hasher);
            image.height.hash(&mut hasher);
            image.byte_len.hash(&mut hasher);
        }
        // Two dropped pictures alike in name, format and size are still different items. (A text
        // that decodes to a picture is the text's item, with or without its picture yet.)
        if data.subject_kind == SubjectKind::Image
            && let Some(picture) = &data.picture
        {
            picture.allocation_id().hash(&mut hasher);
        }
    }
    // Not the table's version, view, reading or labels: those show the same entry, which stays
    // revealed ([`stays_revealed`]).
    hasher.finish()
}

/// The start of the clipboard text, kept on its own lines.
pub fn payload_excerpt(text: &str) -> String {
    let mut out = Vec::new();
    let mut extra = false;
    for (index, line) in text.lines().enumerate() {
        if index >= EXCERPT_LINES {
            extra = true;
            break;
        }
        out.push(clip_chars(line, EXCERPT_LINE_CHARS));
    }
    if extra
        && let Some(last) = out.last_mut()
        && !last.ends_with('…')
    {
        last.push('…');
    }
    out.join("\n")
}

/// The card shown while a slow card is built on its thread: no copied text, only "Working…".
pub fn building_card() -> WorkCard {
    let working = crate::locale::t("card_building").to_string();
    WorkCard {
        title: working.clone(),
        meta: String::new(),
        excerpt: String::new(),
        placeholder: working,
        shows_image: false,
        highlight: None,
        selectable: false,
        link_page: None,
        preview_note: None,
        error_line: None,
    }
}

/// Whether `card` is the [`building_card`] placeholder.
pub fn is_building_card(card: &WorkCard) -> bool {
    card.excerpt.is_empty() && card.title == crate::locale::t("card_building")
}

/// Window height for a card: the placeholder keeps the last real card's height (when there
/// is one), so the window does not jump twice while a slow card is built.
pub fn stable_height(placeholder: bool, wanted: f64, last: Option<f64>) -> f64 {
    match (placeholder, last) {
        (true, Some(last)) => last,
        _ => wanted,
    }
}

/// The card when its worker thread could not start: the short reason, no copied text.
pub fn failed_card(reason: &str) -> WorkCard {
    let mut card = building_card();
    card.title = reason.to_string();
    card.placeholder = reason.to_string();
    card
}

pub fn work_card(data: &LaunchData) -> WorkCard {
    if data.view == CardView::Diff {
        return diff_card(data);
    }
    if let Some(note) = &data.source_note {
        return WorkCard {
            title: data
                .source_name
                .clone()
                .unwrap_or_else(|| crate::locale::t("title_file").to_string()),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: note.clone(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        };
    }
    let mut card = compose_card(data);
    if let Some(name) = &data.source_name {
        card.meta = if card.meta.is_empty() {
            name.clone()
        } else {
            format!("{name}  ·  {}", card.meta)
        };
    }
    if let Some(mark) = data.shown_mark {
        card.meta = if card.meta.is_empty() {
            mark.to_string()
        } else {
            format!("{mark}  ·  {}", card.meta)
        };
    }
    if let Some(notice) = &data.notice {
        card.meta = if card.meta.is_empty() {
            notice.clone()
        } else {
            format!("{}  ·  {notice}", card.meta)
        };
    }
    card
}

/// Whether the card built for `data` may be kept with its history entry and shown again
/// later. A card with a notice in its meta line (a Lines… count, a step note, an error) may
/// not: the kept card would hide a newer notice, or show an old one again. Without this,
/// *Count lines, words and characters* on a plain text card got the kept card back, built
/// before the count, so nothing changed.
pub fn card_may_be_kept(data: &LaunchData) -> bool {
    data.notice.is_none()
}

/// The diff view. The well shows A and B side by side. Copy and Save take the `+` / `-` lines.
pub(crate) fn diff_card(data: &LaunchData) -> WorkCard {
    let Some(diff) = &data.diff else {
        return WorkCard {
            title: crate::locale::t("chip_diff").to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: crate::locale::t("diff_working").to_string(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        };
    };
    let (excerpt, placeholder, meta, selectable) = match &diff.phase {
        crate::diff::Phase::Working => (
            String::new(),
            crate::locale::t("diff_working").to_string(),
            String::new(),
            false,
        ),
        crate::diff::Phase::TooLarge => {
            let message = crate::locale::t("diff_too_large").to_string();
            (message.clone(), String::new(), message, true)
        }
        crate::diff::Phase::Same(pane) => (
            pane.as_str().to_string(),
            String::new(),
            crate::locale::t("diff_none").to_string(),
            true,
        ),
        crate::diff::Phase::Lines(lines) => {
            let added = lines.added();
            let removed = lines.removed();
            (
                lines.view().to_string(),
                String::new(),
                crate::locale::tf("diff_counts", &[&added, &removed]),
                true,
            )
        }
    };
    let mut card = WorkCard {
        title: crate::locale::t("chip_diff").to_string(),
        meta,
        excerpt,
        placeholder,
        shows_image: false,
        highlight: None,
        selectable,
        link_page: None,
        preview_note: None,
        error_line: None,
    };
    with_diff_labels(&mut card, &diff.labels);
    card
}

/// Sensitivity labels of both sides, after the summary. "Checking…" stays at the end so the
/// card cannot be revealed until the check is done.
pub(crate) fn with_diff_labels(card: &mut WorkCard, labels: &crate::sensitivity::Labeling) {
    let line = match labels {
        crate::sensitivity::Labeling::Checking => crate::sensitivity::checking().to_string(),
        crate::sensitivity::Labeling::Known(found) if !found.labels.is_empty() => {
            crate::sensitivity::label_line(&found.labels)
        }
        crate::sensitivity::Labeling::Known(_) => return,
    };
    if card.meta.is_empty() {
        card.meta = format!("  ·  {line}");
    } else {
        card.meta.push_str("  ·  ");
        card.meta.push_str(&line);
    }
}

pub(crate) fn compose_card(data: &LaunchData) -> WorkCard {
    match data.subject_kind {
        SubjectKind::Image => image_card(data),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            // A link's decoder view (URL) is a text view; the Link card is its Original.
            if data.view != CardView::Decode {
                if let Some(card) = youtube_card(text) {
                    return card;
                }
                if let Some(card) = page_card(text) {
                    return card;
                }
            }
            let (excerpt, preview_note) = excerpt_for(text, data.full);
            let mut card = WorkCard {
                title: text_title(text),
                meta: text_meta(text),
                excerpt,
                placeholder: String::new(),
                shows_image: false,
                highlight: None,
                selectable: false,
                link_page: None,
                preview_note,
                error_line: None,
            };
            let mut view = presented_view(text, data.view);
            let svg = format::detect(text) == FormatKind::Svg;
            if svg && view == CardView::Original {
                if data.picture.is_some() {
                    return svg_picture_card(text);
                }
                // Not drawable (or it points outside itself): the highlighted code.
                view = CardView::Format;
            }
            apply_text_view(&mut card, text, view, data.full, data.table.as_ref());
            // Plain text gets line numbers, so a Lines… count can be checked by eye (unless the
            // numbers button turned them off).
            if line_numbers_on()
                && view == CardView::Original
                && card.highlight.is_none()
                && is_plain_kind(format::detect(text))
            {
                card.excerpt = numbered_lines(&card.excerpt);
            }
            if svg && view == CardView::Format {
                card.title = FormatKind::Svg.source_heading().to_string();
                card.meta = svg_meta(text);
            }
            if view == CardView::Decode && data.picture.is_some() && decode::picture_candidate(text)
            {
                show_decoded_picture(&mut card, text);
            }
            // JSON with a mistake stays as copied; the meta line says where, the well marks it.
            if view == CardView::Format
                && let Some(problem) = crate::validate::broken_json(text)
            {
                add_meta_note(&mut card, &problem.note());
                card.error_line = Some(problem.line);
            }
            card
        }
        SubjectKind::Empty => WorkCard {
            title: crate::locale::t("title_clipboard").to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: crate::locale::tf("empty_history", &[&crate::settings::hotkey_label()]),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Hidden => WorkCard {
            title: crate::clipboard::hidden_content().to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: crate::locale::t("private_copy_note").to_string(),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Denied => WorkCard {
            title: crate::locale::t("title_clipboard").to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: format!(
                "{}\n{}\n{}",
                crate::paste_access::denied_note(),
                crate::locale::t("paste_denied_detail"),
                drop_hint()
            ),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::Pending | SubjectKind::PasteAsk => WorkCard {
            title: crate::locale::t("title_clipboard").to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: if data.subject_kind == SubjectKind::PasteAsk {
                crate::paste_access::ask_note().to_string()
            } else {
                format!("{}\n{}", crate::paste_access::pending_note(), drop_hint())
            },
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
        SubjectKind::NoText => WorkCard {
            title: crate::locale::t("title_clipboard").to_string(),
            meta: String::new(),
            excerpt: String::new(),
            placeholder: format!(
                "{}\n{}",
                crate::locale::t("no_text_on_clipboard"),
                drop_hint()
            ),
            shows_image: false,
            highlight: None,
            selectable: false,
            link_page: None,
            preview_note: None,
            error_line: None,
        },
    }
}

pub(crate) fn image_card(data: &LaunchData) -> WorkCard {
    let mut facts = data.image.as_ref().map(image_meta).unwrap_or_default();
    if let Some(edit) = &data.image_edit {
        let mut parts: Vec<&str> = Vec::new();
        if !facts.is_empty() {
            parts.push(&facts);
        }
        if edit.version > 0
            && let Some(label) = edit.labels.get(edit.version)
        {
            parts.push(label);
        }
        if edit.working {
            parts.push(crate::locale::t("working"));
        } else if let Some(note) = &edit.note {
            parts.push(note);
        }
        facts = parts.join("  ·  ");
    }
    let text = match data.view {
        CardView::Info => Some(image_info_text(data)),
        CardView::Ocr | CardView::Qr => image_view_text(data.image_scan.as_ref(), data.view),
        _ => None,
    };
    if let Some(text) = text {
        let meta = if matches!(data.view, CardView::Ocr | CardView::Qr) {
            text_meta(&text)
        } else {
            facts
        };
        return WorkCard {
            title: match data.view {
                CardView::Ocr => crate::locale::t("title_text").to_string(),
                CardView::Qr => crate::locale::t("title_qr").to_string(),
                _ => crate::locale::t("title_image_info").to_string(),
            },
            meta,
            excerpt: shown_body(&text),
            placeholder: String::new(),
            shows_image: false,
            highlight: None,
            selectable: true,
            link_page: None,
            preview_note: None,
            error_line: None,
        };
    }
    WorkCard {
        title: crate::locale::t("title_image").to_string(),
        meta: facts,
        excerpt: String::new(),
        placeholder: String::new(),
        shows_image: true,
        highlight: None,
        selectable: false,
        link_page: None,
        preview_note: None,
        error_line: None,
    }
}

/// Text a non-picture image view copies or saves.
/// OCR and QR use that body.
pub fn image_view_text(scan: Option<&ImageScan>, view: CardView) -> Option<String> {
    let scan = scan?;
    match view {
        CardView::Info => Some(scan.info.clone()),
        CardView::Ocr => scan.ocr.clone(),
        CardView::Qr => scan.qr.clone(),
        _ => None,
    }
}

/// `full` renders the whole text; otherwise the card shows a preview (see [`excerpt_for`]).
pub(crate) fn apply_text_view(
    card: &mut WorkCard,
    source: &str,
    view: CardView,
    full: bool,
    table: Option<&TableShown>,
) {
    if view == CardView::Original {
        show_copied_table(card, source, full);
        return;
    }
    if view == CardView::Dataframe {
        match table {
            Some(table)
                if table.version > 0 || table.options != dataframe::ReadOptions::default() =>
            {
                show_table_version(card, table, full)
            }
            _ => show_dataframe(card, source, full, table.and_then(|table| table.overview)),
        }
        if let Some(error) = table.and_then(|table| table.error.as_deref()) {
            add_meta_note(card, error);
        }
        return;
    }
    let Some(body) = transformed_text(source, view) else {
        return;
    };
    let kind = format::detect(source);
    card.title = if view == CardView::Format && matches!(kind, FormatKind::Json | FormatKind::Xml) {
        text_title(source)
    } else if let Some(flagged) = (view == CardView::Format)
        .then(|| crate::validate::code_title(source))
        .flatten()
    {
        // Formatting still runs; the title says the brackets do not balance.
        flagged
    } else if view == CardView::Format && body == source {
        kind.source_heading().to_string()
    } else {
        text_view_title(source, view, &body)
    };
    card.meta = if view == CardView::Decode {
        // The decoded text keeps the copy's labels: a JWT stays a credential once decoded.
        text_meta_joined(&body, source)
    } else {
        text_meta(&body)
    };
    if matches!(view, CardView::Schema | CardView::Sample) {
        set_excerpt(card, &body, full);
        card.highlight = Some(if view == CardView::Sample || kind == FormatKind::Xml {
            FormatKind::Xml
        } else {
            FormatKind::Json
        });
        card.selectable = true;
    } else if view == CardView::Format && opens_formatted(source) {
        set_excerpt(card, &body, full);
        card.highlight = Some(kind);
    } else if view == CardView::Decode && decode::detect(source) == Some(decode::DecodeKind::Jwt) {
        // A decoded JWT: its Header and Payload JSON in color, the notes above as text.
        set_excerpt(card, &body, full);
        card.highlight = Some(FormatKind::Jwt);
    } else if matches!(view, CardView::Convert | CardView::Decode)
        && format::detect(&body) == FormatKind::Json
    {
        // JSON that a step produced (To JSON / Convert, a decoded Base64 JSON) is colored like
        // the JSON card; the theme and light/dark handling are the highlighter's.
        set_excerpt(card, &body, full);
        card.highlight = Some(FormatKind::Json);
    } else {
        set_excerpt(card, &body, full);
    }
}

/// The Decode view of a Base64 picture whose bytes the card holds ([`LaunchData::picture`]): the
/// well draws the picture (blurred like any picture until clicked) and the meta line keeps the
/// description (type, pixels, bytes), then the labels of the copy.
pub(crate) fn show_decoded_picture(card: &mut WorkCard, source: &str) {
    let Some(body) = transformed_text(source, CardView::Decode) else {
        return;
    };
    card.shows_image = true;
    card.excerpt.zeroize();
    card.preview_note = None;
    card.highlight = None;
    card.selectable = false;
    card.error_line = None;
    let description: Vec<&str> = body.lines().collect();
    let mut meta = description.join("  ·  ");
    if let Some(labels) = joined_label_line(&body, source) {
        meta.push_str("  ·  ");
        meta.push_str(&labels);
    }
    card.meta = meta;
}

/// An SVG drawn in the well: title SVG, its size (width × height, else the viewBox) and the
/// usual line, byte and label facts.
fn svg_picture_card(text: &str) -> WorkCard {
    WorkCard {
        title: FormatKind::Svg.source_heading().to_string(),
        meta: svg_meta(text),
        excerpt: String::new(),
        placeholder: String::new(),
        shows_image: true,
        highlight: None,
        selectable: false,
        link_page: None,
        preview_note: None,
        error_line: None,
    }
}

pub(crate) fn svg_meta(text: &str) -> String {
    let facts = text_meta(text);
    match format::svg::dimensions(text) {
        Some(size) => format!("{size} · {facts}"),
        None => facts,
    }
}

/// Line numbers on plain text cards: one choice for all cards, in memory (like the eye
/// button's choices), off at launch.
static LINE_NUMBERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn line_numbers_on() -> bool {
    LINE_NUMBERS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Flip the line numbers; returns the new state.
pub fn toggle_line_numbers() -> bool {
    !LINE_NUMBERS.fetch_xor(true, std::sync::atomic::Ordering::Relaxed)
}

/// The card for `data` is plain text in its Original view: it can show line numbers, so the
/// numbers button shows.
pub fn offers_line_numbers(data: &LaunchData) -> bool {
    data.subject_kind == SubjectKind::Text
        && data.subject_text.as_deref().is_some_and(|text| {
            presented_view(text, data.view) == CardView::Original
                && is_plain_kind(format::detect(text))
                && crate::youtube::video_id(text).is_none()
                && crate::page_preview::page_url(text).is_none()
        })
}

/// Plain text: multi-line text (`Text`) and a single sentence or other prose (`Plain`). Both
/// show as "Inhoud" / "Content"; line numbers apply to both.
fn is_plain_kind(kind: FormatKind) -> bool {
    matches!(kind, FormatKind::Text | FormatKind::Plain)
}

/// SF Symbol and tooltip (NL/EN) for the numbers button.
pub fn line_numbers_button_look(on: bool) -> (&'static str, &'static str) {
    if on {
        ("list.number", crate::locale::t("hide_line_numbers"))
    } else {
        ("list.number", crate::locale::t("show_line_numbers"))
    }
}

/// `excerpt` with a right-aligned line number in front of every line (`  1  text`). A
/// preview's closing `…` line is no line of the text and stays unnumbered.
pub fn numbered_lines(excerpt: &str) -> String {
    let (body, cut) = match excerpt.strip_suffix("\n…") {
        Some(body) => (body, true),
        None => (excerpt, false),
    };
    let count = body.lines().count().max(1);
    let width = count.to_string().len();
    let mut out = String::with_capacity(body.len() + count * (width + 2));
    for (index, line) in body.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!("{:>width$}  {line}", index + 1));
    }
    if body.is_empty() {
        out.push_str(&format!("{:>width$}  ", 1));
    }
    if cut {
        out.push_str("\n…");
    }
    out
}
