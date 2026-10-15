//! The AI chip (left of Lines…): the local AI actions on a text or code card ([`mac_ai`]). Menu
//! items, titles and the reasons they are off. The actions run on a click only.

use mac_ai::{Action, Unavailable};

use super::*;

/// The submenu the AI actions sit in.
pub fn ai_group() -> &'static str {
    crate::locale::t("ai_group")
}

/// Menu title of `action`.
pub fn ai_title(action: Action) -> &'static str {
    crate::locale::t(match action {
        Action::Summarize => "ai_summarize",
        Action::Shorter => "ai_shorter",
        Action::MoreFormal => "ai_formal",
        Action::Explain => "ai_explain",
        Action::Translate => "ai_translate",
    })
}

/// Why the actions are off: one short line.
pub fn ai_reason(reason: Unavailable) -> &'static str {
    crate::locale::t(match reason {
        Unavailable::UnsupportedSystem => "ai_off_system",
        Unavailable::NotAppleSilicon => "ai_off_silicon",
        Unavailable::DeviceNotEligible => "ai_off_device",
        Unavailable::AppleIntelligenceOff => "ai_off_disabled",
        Unavailable::ModelNotReady => "ai_off_not_ready",
        Unavailable::NoBridge => "ai_off_bridge",
        Unavailable::Other => "ai_off_other",
    })
}

/// What to do about `reason`: one or two short sentences, for the warning and the check.
pub fn ai_fix(reason: Unavailable) -> &'static str {
    crate::locale::t(match reason {
        Unavailable::UnsupportedSystem => "ai_fix_system",
        Unavailable::NotAppleSilicon => "ai_fix_silicon",
        Unavailable::DeviceNotEligible => "ai_fix_device",
        Unavailable::AppleIntelligenceOff => "ai_fix_disabled",
        Unavailable::ModelNotReady => "ai_fix_not_ready",
        Unavailable::NoBridge => "ai_fix_bridge",
        Unavailable::Other => "ai_fix_other",
    })
}

/// Title and message of the Apple Intelligence check: that it works, or the reason and what to
/// do. No content is involved, only the status.
pub fn ai_health(availability: mac_ai::Availability) -> (&'static str, String) {
    let title = crate::locale::t("ai_check_title");
    match availability {
        Ok(()) => (title, crate::locale::t("ai_check_ok").to_string()),
        Err(reason) => (
            title,
            format!("{}\n\n{}", ai_reason(reason), ai_fix(reason)),
        ),
    }
}

/// What a failed run says on the preview. Cancelling says nothing.
pub fn ai_error_text(error: mac_ai::Error) -> &'static str {
    crate::locale::t(match error {
        mac_ai::Error::Unavailable => "ai_off_other",
        mac_ai::Error::Guardrail => "ai_err_guardrail",
        mac_ai::Error::TooLong => "ai_err_too_long",
        mac_ai::Error::Language => "ai_err_language",
        mac_ai::Error::Cancelled | mac_ai::Error::Failed => "ai_err_failed",
    })
}

/// The AI chip's menu on text and code: the five actions; all dimmed (with the reason
/// as their tooltip, and as a last dimmed line) when `availability` says no.
pub fn ai_menu_items(availability: mac_ai::Availability) -> Vec<Command> {
    let enabled = availability.is_ok();
    let detail = match availability {
        Ok(()) => ai_group(),
        Err(reason) => ai_reason(reason),
    };
    let mut items = Vec::new();
    items.extend(Action::ALL.iter().map(|&action| {
        command_enabled(
            CommandId::Ai(action),
            ai_title(action),
            detail,
            "ai apple intelligence summarize summary shorten formal explain translate dutch english samenvatten korter formeel uitleggen vertalen",
            enabled,
        )
    }));
    if let Err(reason) = availability {
        items.push(command_enabled(
            CommandId::AiUnavailable,
            ai_reason(reason),
            ai_group(),
            "ai apple intelligence unavailable",
            false,
        ));
    }
    items
}

/// The "AI" chip left of Lines…: dimmed, with the reason as its tooltip, when the model is not
/// available.
pub fn ai_button(availability: mac_ai::Availability) -> Command {
    let (enabled, detail) = match availability {
        Ok(()) => (true, ai_group()),
        Err(reason) => (false, ai_reason(reason)),
    };
    command_enabled(
        CommandId::AiMenu,
        crate::locale::t("ai_button"),
        detail,
        "ai apple intelligence",
        enabled,
    )
}

/// The AI chip on a scanned PDF: dimmed with a short explanation until text recognition has
/// found text (or when it found none), else like [`ai_button`].
pub fn pdf_scan_ai_button(scan: Option<&ImageScan>) -> Command {
    let mut button = ai_button(mac_ai::availability());
    let waiting = match scan {
        None => Some(crate::locale::t("ai_pdf_ocr_pending")),
        Some(scan) if scan.ocr.is_none() => Some(crate::locale::t("ai_pdf_no_text")),
        Some(_) => None,
    };
    if let Some(reason) = waiting {
        button.enabled = false;
        button.detail = reason.to_string();
    }
    button
}

/// The chips the card shows: [`chips`], with the AI chip directly left of Lines….
pub fn card_chips(data: &LaunchData) -> Vec<Command> {
    let mut chips = chips(data);
    if let Some(at) = chips.iter().position(|c| c.id == CommandId::TextMenu) {
        chips.insert(at, ai_button(mac_ai::availability()));
    } else if data.subject_kind == SubjectKind::Image && data.pdf.is_some() {
        // A scanned PDF: the AI chip works on the recognized text, so it waits for it.
        let at = chips
            .iter()
            .position(|c| c.id == CommandId::ImageMenu)
            .unwrap_or(chips.len());
        chips.insert(at, pdf_scan_ai_button(data.image_scan.as_ref()));
    }
    chips
}

/// The label of the version an applied `action` makes (the version capsule and its menu).
pub fn ai_label(action: Action) -> &'static str {
    crate::locale::t(match action {
        Action::Summarize => "ai_label_summarize",
        Action::Shorter => "ai_label_shorter",
        Action::MoreFormal => "ai_label_formal",
        Action::Explain => "ai_label_explain",
        Action::Translate => "ai_label_translate",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Language;

    const REASONS: [Unavailable; 7] = [
        Unavailable::UnsupportedSystem,
        Unavailable::NotAppleSilicon,
        Unavailable::DeviceNotEligible,
        Unavailable::AppleIntelligenceOff,
        Unavailable::ModelNotReady,
        Unavailable::NoBridge,
        Unavailable::Other,
    ];

    #[test]
    fn available_gives_five_enabled_actions_with_sparkles() {
        let items = ai_menu_items(Ok(()));
        let ids: Vec<_> = items.iter().map(|c| c.id.clone()).collect();
        let want: Vec<_> = (Action::ALL.iter().map(|&a| CommandId::Ai(a))).collect();
        assert_eq!(ids, want);
        for item in &items {
            assert!(item.enabled);
            assert_eq!(symbol_for(&item.id), Some("sparkles"));
            // A flat menu under the chip, no submenu.
            assert_eq!(menu_group(&item.id), None);
        }
    }

    #[test]
    fn unavailable_dims_the_actions_and_says_why() {
        for reason in REASONS {
            let items = ai_menu_items(Err(reason));
            let (actions, note) = items.split_at(Action::ALL.len());
            assert!(
                actions
                    .iter()
                    .all(|c| !c.enabled && c.detail == ai_reason(reason))
            );
            assert_eq!(note.len(), 1);
            assert_eq!(note[0].id, CommandId::AiUnavailable);
            assert!(!note[0].enabled);
            assert_eq!(note[0].title, ai_reason(reason));
        }
    }

    #[test]
    fn every_reason_error_and_title_exists_in_both_languages() {
        for lang in [Language::Nl, Language::En] {
            crate::locale::set_test_language(lang);
            for reason in REASONS {
                assert_ne!(ai_reason(reason), "?");
            }
            for error in [
                mac_ai::Error::Unavailable,
                mac_ai::Error::Guardrail,
                mac_ai::Error::TooLong,
                mac_ai::Error::Language,
                mac_ai::Error::Failed,
            ] {
                assert_ne!(ai_error_text(error), "?");
            }
            for action in Action::ALL {
                assert_ne!(ai_title(action), "?");
                assert_ne!(ai_label(action), "?");
            }
            for key in ["ai_working", "ai_apply", "ai_apply_a11y", "ai_truncated"] {
                assert_ne!(crate::locale::t(key), "?");
            }
        }
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn refusal_and_too_long_say_so_in_both_languages() {
        for lang in [Language::Nl, Language::En] {
            crate::locale::set_test_language(lang);
            let refused = ai_error_text(mac_ai::Error::Guardrail);
            let long = ai_error_text(mac_ai::Error::TooLong);
            assert_ne!(refused, long);
            assert_ne!(refused, ai_error_text(mac_ai::Error::Failed));
            assert_ne!(long, ai_error_text(mac_ai::Error::Failed));
        }
        crate::locale::set_test_language(Language::En);
        // Only the kind of the error reaches the log.
        let line = crate::diag::LogEvent::AiFailed(mac_ai::Error::Guardrail).line();
        assert!(line.ends_with("guardrail"), "{line}");
    }

    #[test]
    fn every_reason_has_a_reason_and_a_fix_in_both_languages() {
        let reasons = [
            Unavailable::UnsupportedSystem,
            Unavailable::NotAppleSilicon,
            Unavailable::DeviceNotEligible,
            Unavailable::AppleIntelligenceOff,
            Unavailable::ModelNotReady,
            Unavailable::NoBridge,
            Unavailable::Other,
        ];
        for lang in [Language::Nl, Language::En] {
            crate::locale::set_test_language(lang);
            let mut seen = Vec::new();
            for reason in reasons {
                let (_, message) = ai_health(Err(reason));
                assert!(message.contains(ai_reason(reason)));
                assert!(message.contains(ai_fix(reason)));
                assert!(!ai_fix(reason).is_empty());
                assert!(!seen.contains(&message), "{reason:?} repeats a message");
                seen.push(message);
            }
        }
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn the_check_names_the_concrete_dutch_advice() {
        crate::locale::set_test_language(Language::Nl);
        let (title, ok) = ai_health(Ok(()));
        assert_eq!(title, "Apple Intelligence");
        assert!(ok.contains("werkt"));
        let (_, off) = ai_health(Err(Unavailable::AppleIntelligenceOff));
        assert!(off.contains("Zet Apple Intelligence aan in Systeeminstellingen"));
        let (_, loading) = ai_health(Err(Unavailable::ModelNotReady));
        assert!(loading.contains("gedownload"));
        let (_, bridge) = ai_health(Err(Unavailable::NoBridge));
        assert!(bridge.contains("swiftc"));
        // The chip's tooltip uses the same reason.
        let chip = ai_button(Err(Unavailable::NoBridge));
        assert_eq!(chip.detail, ai_reason(Unavailable::NoBridge));
        assert!(!chip.enabled);
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn apply_is_dutch_in_dutch() {
        crate::locale::set_test_language(Language::Nl);
        assert_eq!(crate::locale::t("ai_apply"), "Toepassen op klembord");
        crate::locale::set_test_language(Language::En);
        assert_eq!(crate::locale::t("ai_apply"), "Apply to clipboard");
    }

    #[test]
    fn text_and_code_cards_have_the_ai_chip_left_of_lines() {
        for text in ["some prose to look at", "fn main() {\n    let x = 1;\n}\n"] {
            let card = super::super::tests::data(SubjectKind::Text, Some(text));
            let ids: Vec<_> = card_chips(&card).into_iter().map(|c| c.id).collect();
            let lines = ids
                .iter()
                .position(|id| *id == CommandId::TextMenu)
                .expect("Lines…");
            assert_eq!(ids[lines - 1], CommandId::AiMenu);
            // Lines… no longer holds the actions; search still finds them.
            let edit = TextShown {
                labels: vec!["Original".into()],
                version: 0,
                note: None,
                code: false,
                agent: false,
            };
            assert!(
                text_menu_items(&edit)
                    .iter()
                    .all(|(c, _)| !matches!(c.id, CommandId::Ai(_)))
            );
            assert!(
                search_pool(&card)
                    .iter()
                    .any(|c| c.id == CommandId::Ai(Action::Explain))
            );
        }
        // Not on a link or an empty card.
        let none = super::super::tests::data(SubjectKind::Empty, None);
        assert!(card_chips(&none).iter().all(|c| c.id != CommandId::AiMenu));
    }

    #[test]
    fn a_scanned_pdf_has_the_ai_chip_dimmed_until_text_is_recognized() {
        crate::locale::set_test_language(Language::Nl);
        let mut card = super::super::tests::data(SubjectKind::Image, None);
        card.pdf = Some(crate::pdf::PdfFacts {
            pages: 3,
            read: 3,
            scanned: true,
        });
        let chip = |card: &LaunchData| {
            card_chips(card)
                .into_iter()
                .find(|c| c.id == CommandId::AiMenu)
                .expect("AI chip")
        };
        // Recognition not done yet.
        let waiting = chip(&card);
        assert!(!waiting.enabled);
        assert_eq!(waiting.detail, crate::locale::t("ai_pdf_ocr_pending"));
        // Done, nothing found.
        let scan = ImageScan {
            info: String::new(),
            data_url: None,
            ocr: None,
            qr: None,
        };
        card.image_scan = Some(scan.clone());
        let empty = chip(&card);
        assert!(!empty.enabled);
        assert_eq!(empty.detail, crate::locale::t("ai_pdf_no_text"));
        // A picture that is not a PDF has no AI chip.
        card.pdf = None;
        assert!(card_chips(&card).iter().all(|c| c.id != CommandId::AiMenu));
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn the_pdf_footer_starts_with_the_pages() {
        crate::locale::set_test_language(Language::Nl);
        let mut card = super::super::tests::data(SubjectKind::Text, Some("alpha\nbeta"));
        card.pdf = Some(crate::pdf::PdfFacts {
            pages: 340,
            read: 100,
            scanned: false,
        });
        let meta = work_card(&card).meta;
        assert!(
            meta.starts_with("Eerste 100 van 340 pagina's  ·  "),
            "{meta}"
        );
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn the_ai_chip_is_dimmed_with_the_reason_when_unavailable() {
        let on = ai_button(Ok(()));
        assert!(on.enabled);
        assert_eq!(symbol_for(&on.id), Some("sparkles"));
        for reason in REASONS {
            let off = ai_button(Err(reason));
            assert!(!off.enabled);
            assert_eq!(off.detail, ai_reason(reason));
        }
        assert!(keeps_card_open(&CommandId::AiMenu));
        let (shown, _) = split_chips(vec![on.clone(), command(CommandId::TextMenu, "L", "", "")]);
        assert!(shown.iter().any(|c| c.id == CommandId::AiMenu));
    }

    #[test]
    fn the_actions_keep_the_card_open_and_are_not_run_by_a_chip() {
        for id in [
            CommandId::Ai(Action::Summarize),
            CommandId::AiApply,
            CommandId::AiCancel,
        ] {
            assert!(keeps_card_open(&id));
        }
        // Only a pick in the menu (or search) runs one: no chip carries an action.
        let card = super::super::tests::data(SubjectKind::Text, Some("some text to look at"));
        assert!(
            chips(&card)
                .iter()
                .all(|c| !matches!(c.id, CommandId::Ai(_)))
        );
    }
}
