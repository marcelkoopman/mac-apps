//! Feature voorstellen… and Probleem melden…: a small form (subject, two texts, a live preview of
//! the mail) that opens a draft in the default mail app through `mailto:`. The user sends it.
//!
//! Privacy: the body is only what the user typed plus the app and macOS versions
//! ([`crate::mailto`]). The typed text is not logged or stored; the form wipes its fields when it
//! closes. If the text is too long for a link, or no mail app handles `mailto:`, the whole mail
//! goes to the clipboard instead, when the user chooses so. Nothing uses the network: the link is
//! handed to the mail app, which does the sending.

use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2_foundation::NSProcessInfo;
use zeroize::Zeroizing;

use crate::macos_open;
use crate::mailto::{self, Kind, Mail};

/// Show the form for `kind`, then open the draft or offer the clipboard.
pub fn run(kind: Kind) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let (title, first_label, second_label) = match kind {
        Kind::Feature => (
            crate::locale::t("feedback_feature_title"),
            crate::locale::t("feedback_feature_what"),
            crate::locale::t("feedback_feature_why"),
        ),
        Kind::Problem => (
            crate::locale::t("feedback_problem_title"),
            crate::locale::t("feedback_problem_what"),
            crate::locale::t("feedback_problem_expected"),
        ),
    };
    let app_version = crate::macos_about::bundle_version();
    let macos_version = NSProcessInfo::processInfo()
        .operatingSystemVersionString()
        .to_string();
    let ask = mac_ui::dialog::Fields {
        title,
        message: crate::locale::t("feedback_message"),
        ok: crate::locale::t("feedback_make_mail"),
        fields: &[
            mac_ui::dialog::FieldAsk {
                label: crate::locale::t("feedback_subject"),
                lines: 1,
            },
            mac_ui::dialog::FieldAsk {
                label: first_label,
                lines: 3,
            },
            mac_ui::dialog::FieldAsk {
                label: second_label,
                lines: 3,
            },
        ],
        preview_label: crate::locale::t("feedback_preview"),
    };
    let to = crate::locale::t("feedback_to");
    let subject_line = crate::locale::t("feedback_subject_line");
    let too_long = crate::locale::t("feedback_too_long_note");
    let (app, macos) = (app_version.clone(), macos_version.clone());
    let answer = mac_ui::dialog::fields(
        mtm,
        &ask,
        Box::new(move |texts| {
            let mail = compose(kind, texts, &app, &macos);
            let mut shown = mailto::preview_text(&mail, to, subject_line);
            if mailto::mailto_url(&mail).is_none() {
                shown.push_str(too_long);
            }
            shown
        }),
    );
    let Some(texts) = answer else {
        return;
    };
    let texts: Vec<Zeroizing<String>> = texts.into_iter().map(Zeroizing::new).collect();
    if !mailto::has_content(&texts[0], &texts[1], &texts[2]) {
        return;
    }
    let mail = compose(kind, &texts, &app_version, &macos_version);
    // `Some(true)`: too long for a link; `Some(false)`: the link could not be opened.
    let too_long = match mailto::mailto_url(&mail) {
        None => Some(true),
        Some(url) => macos_open::open_mailto(&Zeroizing::new(url))
            .err()
            .map(|_| false),
    };
    if let Some(too_long) = too_long {
        offer_clipboard(mtm, &mail, too_long);
    }
}

fn compose<S: AsRef<str>>(kind: Kind, texts: &[S], app: &str, macos: &str) -> Mail {
    let text = |i: usize| texts.get(i).map_or("", |t| t.as_ref());
    mailto::compose(kind, text(0), text(1), text(2), app, macos)
}

/// The mail cannot be opened as a link: say why and offer to copy it, then tell where it is.
fn offer_clipboard(mtm: MainThreadMarker, mail: &Mail, too_long: bool) {
    let message = if too_long {
        crate::locale::tf("feedback_fallback_long", &[&mailto::SUPPORT_ADDRESS])
    } else {
        crate::locale::tf("feedback_fallback_no_app", &[&mailto::SUPPORT_ADDRESS])
    };
    let copy = crate::locale::t("feedback_copy");
    let cancel = crate::locale::t("cancel");
    let chosen = mac_ui::dialog::buttons(mtm, copy, &message, &[copy, cancel]);
    if chosen != Some(0) {
        return;
    }
    let text = Zeroizing::new(mailto::clipboard_text(mail));
    match crate::macos_pasteboard::write_text(&text, false) {
        Ok(()) => mac_ui::dialog::alert(
            mtm,
            copy,
            &crate::locale::tf("feedback_copied", &[&mailto::SUPPORT_ADDRESS]),
        ),
        Err(_) => mac_ui::dialog::alert(mtm, copy, crate::locale::t("feedback_copy_failed")),
    }
}
