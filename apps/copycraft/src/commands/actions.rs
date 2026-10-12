//! Content actions and button tips.

use super::*;

/// Copy, Save and Preview sit on the well, on the view that is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentActions {
    pub copy: bool,
    pub save: bool,
    /// Open the picture in Preview. The picture itself, or a Base64 picture in the Decode view.
    pub preview: bool,
}

pub fn content_actions(data: &LaunchData) -> ContentActions {
    if data.view == CardView::Diff {
        let ready = data.diff.as_ref().is_some_and(|diff| diff.phase.copyable());
        return ContentActions {
            copy: ready,
            save: ready,
            preview: false,
        };
    }
    match data.subject_kind {
        SubjectKind::Empty
        | SubjectKind::NoText
        | SubjectKind::Hidden
        | SubjectKind::Denied
        | SubjectKind::Pending
        | SubjectKind::PasteAsk => ContentActions {
            copy: false,
            save: false,
            preview: false,
        },
        SubjectKind::Image => image_content_actions(data),
        SubjectKind::Text => {
            let text = data.subject_text.as_deref().unwrap_or("");
            // A Base64 picture drawn in the Decode view. The description text is not a picture.
            let preview = data.view == CardView::Decode && data.picture.is_some();
            if crate::youtube::video_id(text).is_some()
                || crate::page_preview::page_url(text).is_some()
            {
                // Copy the URL. Nothing to save.
                ContentActions {
                    copy: true,
                    save: false,
                    preview,
                }
            } else {
                ContentActions {
                    copy: true,
                    save: true,
                    preview,
                }
            }
        }
    }
}

pub(crate) fn image_content_actions(data: &LaunchData) -> ContentActions {
    let scan = data.image_scan.as_ref();
    let copy = match data.view {
        CardView::Info => scan.is_some(),
        CardView::Ocr | CardView::Qr => image_view_text(scan, data.view).is_some(),
        _ => false,
    };
    // Info, Text and QR are the scan's text. Preview opens the picture itself.
    ContentActions {
        copy,
        save: true,
        preview: data.view == CardView::Original,
    }
}

pub(crate) fn preview_command() -> Command {
    command(
        CommandId::OpenPreview,
        crate::locale::t("open_preview"),
        crate::locale::t("detail_open_preview"),
        "preview voorvertoning quick look open image picture",
    )
}

pub fn wipe_commands(commands: &mut [Command]) {
    for command in commands {
        command.wipe();
    }
}

pub fn copy_tip(title: &str) -> String {
    crate::locale::tf("copy_tip", &[&title])
}

pub fn save_tip(title: &str) -> String {
    crate::locale::tf("save_tip", &[&title])
}

pub fn preview_tip(title: &str) -> String {
    crate::locale::tf("preview_tip", &[&title])
}
