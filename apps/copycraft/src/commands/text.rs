//! Text transforms, titles, link/page previews and save file names.

use super::*;

pub(crate) fn image_info_text(data: &LaunchData) -> String {
    let mut parts = Vec::new();
    if let Some(info) = data.image_scan.as_ref().map(|scan| scan.info.clone()) {
        parts.push(info);
    } else if let Some(facts) = &data.image {
        parts.push(format!(
            "{}\n{}\n{}",
            facts.format,
            crate::locale::tf("image_size_line", &[&facts.width, &facts.height]),
            format_bytes(facts.byte_len)
        ));
    } else {
        parts.push(crate::locale::t("title_image").to_string());
    }
    if let Some(url) = data
        .image_scan
        .as_ref()
        .and_then(|scan| scan.data_url.clone())
    {
        parts.push(url);
    }
    parts.join("\n\n")
}

pub(crate) fn shown_body(body: &str) -> String {
    let mut chars = body.chars();
    let shown: String = chars.by_ref().take(CODE_CAP).collect();
    if chars.next().is_some() {
        format!("{shown}\n…")
    } else {
        shown
    }
}

pub(crate) fn text_title(text: &str) -> String {
    crate::validate::title(text)
        .or_else(|| crate::validate::code_title(text))
        .unwrap_or_else(|| format::detect(text).source_heading().to_string())
}

pub(crate) fn text_view_title(source: &str, view: CardView, body: &str) -> String {
    match view {
        CardView::Format => format::detect(source).preview_heading().to_string(),
        CardView::Dataframe => crate::locale::t("table_view").to_string(),
        CardView::Schema => if format::detect(source) == FormatKind::Xml {
            crate::locale::t("xsd_schema")
        } else {
            crate::locale::t("avro_schema")
        }
        .to_string(),
        CardView::Sample => crate::locale::t("sample_xml").to_string(),
        CardView::Decode => match decode::detect(source) {
            Some(found) => found.label().to_string(),
            None => format::detect(body).source_heading().to_string(),
        },
        _ => format::detect(body).source_heading().to_string(),
    }
}

/// Full text for the current card view. Original returns the source.
pub fn transformed_text(source: &str, view: CardView) -> Option<String> {
    match view {
        CardView::Original => Some(source.to_string()),
        CardView::Format => Some(clipboard::formatted(source)),
        CardView::Convert => convert::try_convert(source).map(|body| clipboard::formatted(&body)),
        CardView::Decode => decode::decode(source).map(|decoded| decoded.body),
        // A table copies as the CSV it is; the drawn grid is for the screen only.
        // Read and written again: yyyy-mm-dd dates, the source's separator, proper quoting.
        CardView::Dataframe => Some(
            dataframe::parse_table(source)
                .and_then(|df| dataframe::frame_copy(&df, source))
                .unwrap_or_else(|| source.to_string()),
        ),
        CardView::Schema => {
            if format::detect(source) == FormatKind::Xml {
                crate::xsd_schema::try_schema(source)
            } else {
                crate::avro_schema::try_schema(source)
            }
        }
        CardView::Sample => crate::xsd_schema::try_sample(source),
        CardView::Info | CardView::Ocr | CardView::Qr | CardView::Diff => None,
    }
}

/// A page link: the URL as plain text under the kind, "Link". Nothing is fetched for it.
pub(crate) fn page_card(text: &str) -> Option<WorkCard> {
    let page = crate::page_preview::page_url(text)?;
    Some(WorkCard {
        title: crate::locale::t("title_link").to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        // The URL can be selected and copied.
        selectable: true,
        link_page: Some(page.to_string()),
        preview_note: None,
        error_line: None,
    })
}

pub(crate) fn youtube_card(text: &str) -> Option<WorkCard> {
    crate::youtube::video_id(text)?;
    Some(WorkCard {
        title: crate::locale::t("title_youtube").to_string(),
        meta: text_meta(text),
        excerpt: payload_excerpt(text),
        placeholder: String::new(),
        shows_image: false,
        highlight: None,
        selectable: true,
        link_page: Some(text.trim().to_string()),
        preview_note: None,
        error_line: None,
    })
}

/// File name and extension [`text_save_file`] gives `view`, when they follow from `source`
/// alone (Original, Format, Dataframe). The save panel can then open before the bytes are
/// built, and the conversion runs after a path is chosen. `None` for views whose name depends
/// on the converted text.
pub fn deferred_save_name(source: &str, view: CardView) -> Option<(String, &'static str)> {
    let kind = match view {
        CardView::Original | CardView::Format => format::detect(source),
        CardView::Dataframe => FormatKind::Dataframe,
        _ => return None,
    };
    Some((kind.suggested_filename(), kind.suggested_extension()))
}

pub fn text_save_file(source: &str, view: CardView) -> Option<SaveFile> {
    if view == CardView::Dataframe {
        // The table as the view reads it (dates typed), as CSV; the save panel can pick Parquet.
        let bytes = dataframe::TableFile::Csv.bytes(
            &dataframe::parse_table(source)?,
            dataframe::source_separator(source),
        )?;
        return Some(SaveFile {
            filename: FormatKind::Dataframe.suggested_filename(),
            extension: FormatKind::Dataframe.suggested_extension(),
            bytes,
        });
    }
    if view == CardView::Schema {
        let body = transformed_text(source, view)?;
        let (filename, extension) = if format::detect(source) == FormatKind::Xml {
            ("clipboard.xsd", "xsd")
        } else {
            ("clipboard.avsc", "avsc")
        };
        return Some(SaveFile {
            filename: filename.to_string(),
            extension,
            bytes: body.into_bytes(),
        });
    }
    let body = transformed_text(source, view)?;
    let kind = match view {
        CardView::Format => format::detect(source),
        _ => format::detect(&body),
    };
    Some(SaveFile {
        filename: kind.suggested_filename(),
        extension: kind.suggested_extension(),
        bytes: body.into_bytes(),
    })
}
