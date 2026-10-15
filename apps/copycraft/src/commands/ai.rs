//! Lines… › Apple Intelligence: the local AI actions on a text or code card ([`mac_ai`]). Menu
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
        Unavailable::NoBridge | Unavailable::Other => "ai_off_other",
    })
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

/// The five actions as menu items, all dimmed (with the reason as their tooltip, and as a last
/// dimmed line) when `availability` says no.
pub fn ai_menu_items(availability: mac_ai::Availability) -> Vec<Command> {
    let enabled = availability.is_ok();
    let detail = match availability {
        Ok(()) => ai_group(),
        Err(reason) => ai_reason(reason),
    };
    let mut items: Vec<Command> = Action::ALL
        .iter()
        .map(|&action| {
            command_enabled(
                CommandId::Ai(action),
                ai_title(action),
                detail,
                "ai apple intelligence summarize summary shorten formal explain translate dutch english samenvatten korter formeel uitleggen vertalen",
                enabled,
            )
        })
        .collect();
    items.push(command_enabled(
        CommandId::AiCustom,
        crate::locale::t("ai_custom"),
        detail,
        "ai apple intelligence custom prompt free instruction opdracht eigen",
        enabled,
    ));
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

/// Title of the preview and label of the version a free prompt makes.
pub fn ai_custom_label() -> &'static str {
    crate::locale::t("ai_label_custom")
}

/// The typed instruction for a free prompt, trimmed and cut to [`mac_ai::MAX_INSTRUCTION_CHARS`];
/// `None` when it is empty. The `field` is cleared either way: no copy of the prompt is kept
/// beyond the run (the result is zeroized when dropped).
pub fn take_instruction(field: &mut String) -> Option<zeroize::Zeroizing<String>> {
    use zeroize::Zeroize;
    let (cut, _) = mac_ai::truncate_instruction(field.trim());
    let taken = (!cut.is_empty()).then(|| zeroize::Zeroizing::new(cut.to_string()));
    field.zeroize();
    taken
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
        let mut want: Vec<_> = Action::ALL.iter().map(|&a| CommandId::Ai(a)).collect();
        want.push(CommandId::AiCustom);
        assert_eq!(ids, want);
        for item in &items {
            assert!(item.enabled);
            assert_eq!(symbol_for(&item.id), Some("sparkles"));
            assert_eq!(menu_group(&item.id), Some(ai_group()));
        }
    }

    #[test]
    fn unavailable_dims_the_actions_and_says_why() {
        for reason in REASONS {
            let items = ai_menu_items(Err(reason));
            let (actions, note) = items.split_at(Action::ALL.len() + 1);
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
            for key in [
                "ai_working",
                "ai_apply",
                "ai_apply_a11y",
                "ai_truncated",
                "ai_custom",
                "ai_custom_title",
                "ai_custom_message",
                "ai_label_custom",
            ] {
                assert_ne!(crate::locale::t(key), "?");
            }
        }
        crate::locale::set_test_language(Language::En);
    }

    #[test]
    fn the_prompt_field_is_cleared_when_taken() {
        let mut field = String::from("  zet dit om naar een tabel  ");
        let taken = take_instruction(&mut field);
        assert_eq!(
            taken.as_deref().map(String::as_str),
            Some("zet dit om naar een tabel")
        );
        assert!(field.is_empty());
        let mut blank = String::from("   ");
        assert!(take_instruction(&mut blank).is_none());
        assert!(blank.is_empty());
        let mut long = "a".repeat(mac_ai::MAX_INSTRUCTION_CHARS + 50);
        let taken = take_instruction(&mut long).expect("some");
        assert_eq!(taken.chars().count(), mac_ai::MAX_INSTRUCTION_CHARS);
        assert!(long.is_empty());
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
    fn apply_is_dutch_in_dutch() {
        crate::locale::set_test_language(Language::Nl);
        assert_eq!(crate::locale::t("ai_apply"), "Toepassen op klembord");
        crate::locale::set_test_language(Language::En);
        assert_eq!(crate::locale::t("ai_apply"), "Apply to clipboard");
    }

    #[test]
    fn lines_menu_has_the_actions_on_text_and_code_cards() {
        for code in [false, true] {
            let edit = TextShown {
                labels: vec!["Original".into()],
                version: 0,
                note: None,
                code,
                agent: false,
            };
            let ids: Vec<_> = text_menu_items(&edit)
                .into_iter()
                .map(|(c, _)| c.id)
                .collect();
            for action in Action::ALL {
                assert!(ids.contains(&CommandId::Ai(action)));
            }
        }
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
        // Only a pick in the menu (or search) runs one: no chip carries an AI action.
        let card = super::super::tests::data(SubjectKind::Text, Some("some text to look at"));
        assert!(
            chips(&card)
                .iter()
                .all(|c| !matches!(c.id, CommandId::Ai(_)))
        );
    }
}
