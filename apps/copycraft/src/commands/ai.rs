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
    match action {
        Action::Summarize => crate::locale::t("ai_summarize"),
        Action::Shorter => crate::locale::t("ai_shorter"),
        Action::MoreFormal => crate::locale::t("ai_formal"),
        Action::Proofread => crate::locale::t("ai_proofread"),
        Action::Translate => crate::locale::t("ai_translate"),
        Action::ExplainCode => crate::locale::t("ai_explain_code"),
        Action::DocComment => crate::locale::t("ai_doc_comment"),
        Action::ExplainStructure => crate::locale::t("ai_explain_structure"),
        Action::SummarizeRules => crate::locale::t("ai_summarize_rules"),
        Action::FindConflicts => crate::locale::t("ai_find_conflicts"),
    }
}

/// The submenu a rewrite action sits in: prose gets four entries, one of them a small submenu.
pub fn ai_submenu(action: Action) -> Option<&'static str> {
    matches!(action, Action::Shorter | Action::MoreFormal).then(|| crate::locale::t("ai_rewrite"))
}

const PROSE: [Action; 5] = [
    Action::Summarize,
    Action::Shorter,
    Action::MoreFormal,
    Action::Proofread,
    Action::Translate,
];
const CODE: [Action; 2] = [Action::ExplainCode, Action::DocComment];
const STRUCTURE: [Action; 1] = [Action::ExplainStructure];
const AGENT: [Action; 2] = [Action::SummarizeRules, Action::FindConflicts];

/// The actions that fit a text of `kind`: prose gets Summarize, Rewrite ▸ (Shorter, More formal),
/// Proofread and Translate; code Explain and a doc comment; JSON and YAML an explanation of the
/// structure; agent instructions a summary and a check for conflicts. Everything else (links,
/// XML, TOML, tables, pictures) gets none, so no AI chip.
pub fn ai_actions_for_kind(kind: crate::format::FormatKind) -> &'static [Action] {
    use crate::format::FormatKind as K;
    match kind {
        K::Text | K::Plain | K::Markdown => &PROSE,
        K::Rust | K::Java | K::Python => &CODE,
        K::Json | K::Yaml => &STRUCTURE,
        K::Agent => &AGENT,
        _ => &[],
    }
}

/// The actions the card offers: by the kind of its text; a scanned PDF's recognized text is
/// prose (the chip itself waits for the recognition).
pub fn ai_actions(data: &LaunchData) -> &'static [Action] {
    if data.subject_kind == SubjectKind::Image && data.pdf.is_some() {
        return &PROSE;
    }
    if data.subject_kind != SubjectKind::Text {
        return &[];
    }
    match data.subject_text.as_deref() {
        Some(text) if offers_text_menu(text) => {
            let kind = crate::format::detect(text);
            // A copy with a decoder chip (JWT, Base64, URL, hash, UUID, Unix time) is data to
            // decode, not prose: no AI chip.
            if crate::toolbar_visibility::tries_decode(kind, text)
                && crate::decode::detect(text).is_some()
            {
                return &[];
            }
            ai_actions_for_kind(kind)
        }
        _ => &[],
    }
}

/// Why the actions are off: one short line.
pub fn ai_reason(reason: Unavailable) -> &'static str {
    match reason {
        Unavailable::UnsupportedSystem => crate::locale::t("ai_off_system"),
        Unavailable::NotAppleSilicon => crate::locale::t("ai_off_silicon"),
        Unavailable::DeviceNotEligible => crate::locale::t("ai_off_device"),
        Unavailable::AppleIntelligenceOff => crate::locale::t("ai_off_disabled"),
        Unavailable::ModelNotReady => crate::locale::t("ai_off_not_ready"),
        Unavailable::NoBridge => crate::locale::t("ai_off_bridge"),
        Unavailable::Other => crate::locale::t("ai_off_other"),
    }
}

/// What to do about `reason`: one or two short sentences, for the warning and the check.
pub fn ai_fix(reason: Unavailable) -> &'static str {
    match reason {
        Unavailable::UnsupportedSystem => crate::locale::t("ai_fix_system"),
        Unavailable::NotAppleSilicon => crate::locale::t("ai_fix_silicon"),
        Unavailable::DeviceNotEligible => crate::locale::t("ai_fix_device"),
        Unavailable::AppleIntelligenceOff => crate::locale::t("ai_fix_disabled"),
        Unavailable::ModelNotReady => crate::locale::t("ai_fix_not_ready"),
        Unavailable::NoBridge => crate::locale::t("ai_fix_bridge"),
        Unavailable::Other => crate::locale::t("ai_fix_other"),
    }
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
    match error {
        mac_ai::Error::Unavailable => crate::locale::t("ai_off_other"),
        mac_ai::Error::Guardrail => crate::locale::t("ai_err_guardrail"),
        mac_ai::Error::TooLong => crate::locale::t("ai_err_too_long"),
        mac_ai::Error::Language => crate::locale::t("ai_err_language"),
        mac_ai::Error::Refusal => crate::locale::t("ai_err_refusal"),
        mac_ai::Error::Cancelled | mac_ai::Error::Failed => crate::locale::t("ai_err_failed"),
    }
}

/// The longest code, in characters, that goes to the model in one piece: Apple's on-device
/// model has a small context window, and longer code fails ("too long"). A longer text gets a
/// menu of its functions instead ([`ai_functions`]).
pub const AI_CODE_MAX_CHARS: usize = 5_000;
/// The most functions the AI menu lists; the rest is mentioned, not listed.
pub const AI_FN_MENU_MAX: usize = 30;
/// [`CommandId::AiFn`] index of the whole text.
pub const AI_FN_WHOLE: u8 = u8::MAX;
/// [`CommandId::AiFn`] index of the dimmed line that says more functions exist.
pub const AI_FN_MORE: u8 = u8::MAX - 1;

/// The functions of a code card whose AI menu lists them: see [`ai_functions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiFns {
    /// The first [`AI_FN_MENU_MAX`] function names, in file order.
    pub names: Vec<String>,
    /// How many functions the text has.
    pub total: usize,
    /// The whole text is short enough to go to the model at once.
    pub whole_fits: bool,
}

/// The functions to list in the AI menu of a code card: when the text is longer than
/// [`AI_CODE_MAX_CHARS`] or has more than one function. `None` for other text, and for long code
/// without recognizable functions (its actions then say it is too long).
pub fn ai_functions(data: &LaunchData) -> Option<AiFns> {
    if data.subject_kind != SubjectKind::Text {
        return None;
    }
    ai_functions_of(data.subject_text.as_deref()?)
}

fn ai_functions_of(text: &str) -> Option<AiFns> {
    let kind = crate::format::detect(text);
    if !matches!(
        kind,
        crate::format::FormatKind::Rust
            | crate::format::FormatKind::Java
            | crate::format::FormatKind::Python
    ) {
        return None;
    }
    let whole_fits = text.chars().take(AI_CODE_MAX_CHARS + 1).count() <= AI_CODE_MAX_CHARS;
    let spans = crate::format::fn_spans::function_spans(text, kind);
    if spans.is_empty() || (whole_fits && spans.len() < 2) {
        return None;
    }
    Some(AiFns {
        names: spans
            .iter()
            .take(AI_FN_MENU_MAX)
            .map(|span| span.name.clone())
            .collect(),
        total: spans.len(),
        whole_fits,
    })
}

/// The text a run of a code action on function `index` works on: the function with its
/// signature, doc comment and body. `None` when the text has no such function.
pub fn ai_function_text(text: &str, index: u8) -> Option<&str> {
    let spans = crate::format::fn_spans::function_spans(text, crate::format::detect(text));
    let span = spans.get(usize::from(index))?;
    text.get(span.byte_range.clone())
}

/// Whether `text` is within [`AI_CODE_MAX_CHARS`].
pub fn ai_code_fits(text: &str) -> bool {
    text.chars().take(AI_CODE_MAX_CHARS + 1).count() <= AI_CODE_MAX_CHARS
}

/// What the preview says when code is too long for the model: with a chosen function (`whole`
/// false) or the whole text. Names the limit.
pub fn ai_code_too_long(whole: bool) -> String {
    if whole {
        crate::locale::tf("ai_err_code_too_long", &[&AI_CODE_MAX_CHARS])
    } else {
        crate::locale::tf("ai_err_fn_too_long", &[&AI_CODE_MAX_CHARS])
    }
}

/// The AI chip's menu: `actions` ([`ai_actions`]); all dimmed (with the reason as their
/// tooltip, and as a last dimmed line) when `availability` says no.
pub fn ai_menu_items(availability: mac_ai::Availability, actions: &[Action]) -> Vec<Command> {
    ai_menu_items_with(availability, actions, None)
}

/// [`ai_menu_items`] for a code card with `fns`: each code action becomes a submenu of the whole
/// text (when it fits) and the functions, so only the chosen one goes to the model.
pub fn ai_menu_items_with(
    availability: mac_ai::Availability,
    actions: &[Action],
    fns: Option<&AiFns>,
) -> Vec<Command> {
    let enabled = availability.is_ok();
    let detail = match availability {
        Ok(()) => ai_group(),
        Err(reason) => ai_reason(reason),
    };
    const KEYWORDS: &str = "ai apple intelligence summarize summary shorten formal proofread spelling grammar explain translate comment structure conflicts samenvatten korter formeel corrigeren uitleggen vertalen";
    let mut items: Vec<Command> = Vec::new();
    for &action in actions {
        match fns.filter(|_| enabled && CODE.contains(&action)) {
            Some(fns) => {
                if fns.whole_fits {
                    items.push(command_enabled(
                        CommandId::AiFn(action, AI_FN_WHOLE),
                        crate::locale::t("ai_whole_file"),
                        detail,
                        KEYWORDS,
                        true,
                    ));
                }
                for (index, name) in fns.names.iter().enumerate() {
                    items.push(command_enabled(
                        CommandId::AiFn(action, index as u8),
                        name,
                        detail,
                        KEYWORDS,
                        true,
                    ));
                }
                if fns.total > fns.names.len() {
                    items.push(command_enabled(
                        CommandId::AiFn(action, AI_FN_MORE),
                        &crate::locale::tf("ai_more_functions", &[&(fns.total - fns.names.len())]),
                        detail,
                        KEYWORDS,
                        false,
                    ));
                }
            }
            None => items.push(command_enabled(
                CommandId::Ai(action),
                ai_title(action),
                detail,
                KEYWORDS,
                enabled,
            )),
        }
    }
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

/// The chips the card shows: [`chips`], with the AI chip directly left of Lines… when an action
/// fits the text.
pub fn card_chips(data: &LaunchData) -> Vec<Command> {
    let mut chips = chips(data);
    if let Some(at) = chips.iter().position(|c| c.id == CommandId::TextMenu) {
        // Only when an action fits this kind of text.
        if !ai_actions(data).is_empty() {
            chips.insert(at, ai_button(mac_ai::availability()));
        }
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
    match action {
        Action::Summarize => crate::locale::t("ai_label_summarize"),
        Action::Shorter => crate::locale::t("ai_label_shorter"),
        Action::MoreFormal => crate::locale::t("ai_label_formal"),
        Action::Proofread => crate::locale::t("ai_label_proofread"),
        Action::Translate => crate::locale::t("ai_label_translate"),
        Action::ExplainCode => crate::locale::t("ai_label_explain"),
        Action::DocComment => crate::locale::t("ai_label_doc_comment"),
        Action::ExplainStructure => crate::locale::t("ai_label_structure"),
        Action::SummarizeRules => crate::locale::t("ai_label_summarize"),
        Action::FindConflicts => crate::locale::t("ai_label_conflicts"),
    }
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

    use crate::format::FormatKind as K;

    fn ids_of(items: &[Command]) -> Vec<CommandId> {
        items.iter().map(|c| c.id.clone()).collect()
    }

    #[test]
    fn each_kind_of_text_gets_its_few_actions() {
        let ai = |kind| ai_actions_for_kind(kind).to_vec();
        for kind in [K::Text, K::Plain, K::Markdown] {
            assert_eq!(
                ai(kind),
                vec![
                    Action::Summarize,
                    Action::Shorter,
                    Action::MoreFormal,
                    Action::Proofread,
                    Action::Translate
                ]
            );
        }
        for kind in [K::Rust, K::Java, K::Python] {
            assert_eq!(ai(kind), vec![Action::ExplainCode, Action::DocComment]);
        }
        for kind in [K::Json, K::Yaml] {
            assert_eq!(ai(kind), vec![Action::ExplainStructure]);
        }
        assert_eq!(
            ai(K::Agent),
            vec![Action::SummarizeRules, Action::FindConflicts]
        );
        // Not recognized as something the model helps with: no actions, so no chip.
        for kind in [
            K::Csv,
            K::Tsv,
            K::Dataframe,
            K::Url,
            K::Xml,
            K::Html,
            K::Svg,
            K::Toml,
            K::Image,
            K::Jwt,
        ] {
            assert!(ai(kind).is_empty(), "{kind:?}");
        }
        // Prose has four entries, the rewrites in a submenu; nothing else has more than two.
        let top = |actions: &[Action]| {
            actions.iter().filter(|&&a| ai_submenu(a).is_none()).count()
                + usize::from(actions.iter().any(|&a| ai_submenu(a).is_some()))
        };
        assert_eq!(top(ai_actions_for_kind(K::Text)), 4);
        for kind in [K::Rust, K::Json, K::Agent] {
            assert!(top(ai_actions_for_kind(kind)) <= 2);
        }
    }

    #[test]
    fn the_menu_lists_exactly_the_actions_of_the_card_with_sparkles() {
        let items = ai_menu_items(Ok(()), ai_actions_for_kind(K::Rust));
        assert_eq!(
            ids_of(&items),
            vec![
                CommandId::Ai(Action::ExplainCode),
                CommandId::Ai(Action::DocComment)
            ]
        );
        for item in &items {
            assert!(item.enabled);
            assert_eq!(symbol_for(&item.id), Some("sparkles"));
            assert_eq!(menu_group(&item.id), None);
        }
        let prose = ai_menu_items(Ok(()), ai_actions_for_kind(K::Text));
        let grouped: Vec<_> = prose
            .iter()
            .filter(|c| menu_group(&c.id).is_some())
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(
            grouped,
            vec![
                CommandId::Ai(Action::Shorter),
                CommandId::Ai(Action::MoreFormal)
            ]
        );
    }

    #[test]
    fn decodable_text_gets_no_ai_chip() {
        for text in [
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "SGVsbG8sIHdvcmxkIQ==",
            "hello%20world%21",
            "550e8400-e29b-41d4-a716-446655440000",
        ] {
            let card = super::super::tests::data(SubjectKind::Text, Some(text));
            assert!(ai_actions(&card).is_empty(), "{text}");
            assert!(card_chips(&card).iter().all(|c| c.id != CommandId::AiMenu));
            assert!(
                card_chips(&card).iter().any(|c| c.id == CommandId::Decode),
                "{text}"
            );
        }
        let prose = super::super::tests::data(SubjectKind::Text, Some("some prose to look at"));
        assert!(!ai_actions(&prose).is_empty());
    }

    #[test]
    fn cards_get_the_chip_only_when_an_action_fits() {
        let chip = |kind, text: &str| {
            let card = super::super::tests::data(kind, Some(text));
            (
                card_chips(&card).iter().any(|c| c.id == CommandId::AiMenu),
                ai_actions(&card).to_vec(),
            )
        };
        let (on, set) = chip(SubjectKind::Text, "some prose to look at here");
        assert!(on && set.contains(&Action::Proofread));
        let (on, set) = chip(SubjectKind::Text, "fn main() {\n    let x = 1;\n}\n");
        assert!(on && set == vec![Action::ExplainCode, Action::DocComment]);
        let (on, set) = chip(SubjectKind::Text, "{\"a\": {\"b\": [1, 2]}}");
        assert!(on && set == vec![Action::ExplainStructure]);
        // A table, a link, a picture and an empty card: no chip, no actions.
        let table = crate::dataframe::tests::ENERGY_FIXTURE;
        assert_eq!(chip(SubjectKind::Text, table), (false, vec![]));
        assert_eq!(
            chip(SubjectKind::Text, "https://example.com/a"),
            (false, vec![])
        );
        assert_eq!(chip(SubjectKind::Image, ""), (false, vec![]));
        assert_eq!(chip(SubjectKind::Empty, ""), (false, vec![]));
    }

    #[test]
    fn unavailable_dims_the_actions_and_says_why() {
        for reason in REASONS {
            let items = ai_menu_items(Err(reason), ai_actions_for_kind(K::Agent));
            let (actions, note) = items.split_at(2);
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
                mac_ai::Error::Refusal,
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
        for (text, found) in [
            ("some prose to look at", Action::Proofread),
            ("fn main() {\n    let x = 1;\n}\n", Action::ExplainCode),
        ] {
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
                    .any(|c| c.id == CommandId::Ai(found))
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

    fn rust_with(count: usize) -> String {
        (0..count)
            .map(|n| format!("fn f{n}() {{\n    let x = {n};\n}}\n"))
            .collect()
    }

    #[test]
    fn short_code_with_one_function_keeps_the_plain_menu() {
        let text = "fn main() {\n    let x = 1;\n}\n";
        assert_eq!(ai_functions_of(text), None);
    }

    #[test]
    fn code_with_several_functions_lists_them_per_action() {
        let text = rust_with(3);
        let fns = ai_functions_of(&text).expect("functions");
        assert_eq!(fns.names, ["f0", "f1", "f2"]);
        assert!(fns.whole_fits);
        let items = ai_menu_items_with(Ok(()), ai_actions_for_kind(K::Rust), Some(&fns));
        let ids: Vec<_> = items.iter().map(|c| c.id.clone()).collect();
        let explain = Action::ExplainCode;
        assert_eq!(
            &ids[..4],
            [
                CommandId::AiFn(explain, AI_FN_WHOLE),
                CommandId::AiFn(explain, 0),
                CommandId::AiFn(explain, 1),
                CommandId::AiFn(explain, 2),
            ]
        );
        assert_eq!(items.len(), 8);
        assert!(items.iter().all(|c| c.enabled));
        // One submenu per action, named after it.
        assert_eq!(
            menu_group(&CommandId::AiFn(explain, 1)),
            Some(ai_title(explain))
        );
        // A function goes to the model with its signature.
        assert_eq!(
            ai_function_text(&text, 1),
            Some("fn f1() {\n    let x = 1;\n}")
        );
        assert_eq!(ai_function_text(&text, 3), None);
    }

    #[test]
    fn long_code_has_no_whole_text_item_and_the_list_is_capped() {
        let text = rust_with(AI_FN_MENU_MAX + 5);
        let fns = ai_functions_of(&text).expect("functions");
        assert_eq!(fns.names.len(), AI_FN_MENU_MAX);
        assert_eq!(fns.total, AI_FN_MENU_MAX + 5);
        let items = ai_menu_items_with(Ok(()), &[Action::ExplainCode], Some(&fns));
        assert_eq!(
            items.len(),
            usize::from(fns.whole_fits) + AI_FN_MENU_MAX + 1
        );
        let last = items.last().expect("note");
        assert_eq!(last.id, CommandId::AiFn(Action::ExplainCode, AI_FN_MORE));
        assert!(!last.enabled);
        assert!(last.title.contains('5'));

        let long = format!(
            "fn big() {{\n{}}}\nfn small() {{}}\n",
            "    let a = 1;\n".repeat(500)
        );
        assert!(!ai_code_fits(&long));
        let fns = ai_functions_of(&long).expect("functions");
        assert!(!fns.whole_fits);
        let items = ai_menu_items_with(Ok(()), &[Action::DocComment], Some(&fns));
        assert!(
            items
                .iter()
                .all(|c| c.id != CommandId::AiFn(Action::DocComment, AI_FN_WHOLE))
        );
    }

    #[test]
    fn unavailable_model_keeps_the_flat_dimmed_menu() {
        let fns = ai_functions_of(&rust_with(3)).expect("functions");
        let items = ai_menu_items_with(
            Err(Unavailable::NoBridge),
            ai_actions_for_kind(K::Rust),
            Some(&fns),
        );
        assert!(items.iter().all(|c| !c.enabled));
        assert!(
            items
                .iter()
                .any(|c| c.id == CommandId::Ai(Action::ExplainCode))
        );
    }

    #[test]
    fn the_code_limit_is_named_in_both_languages() {
        assert_eq!(AI_CODE_MAX_CHARS, 5_000);
        for lang in [Language::Nl, Language::En] {
            crate::locale::set_test_language(lang);
            for whole in [true, false] {
                let message = ai_code_too_long(whole);
                assert!(message.contains("5000"), "{message}");
                assert!(!message.contains("{0}") && !message.contains('?'));
            }
            assert_ne!(ai_code_too_long(true), ai_code_too_long(false));
            for key in ["ai_whole_file", "ai_part_note", "ai_err_refusal"] {
                assert_ne!(crate::locale::t(key), "?");
            }
        }
        crate::locale::set_test_language(Language::En);
        assert!(ai_code_fits(&"a".repeat(AI_CODE_MAX_CHARS)));
        assert!(!ai_code_fits(&"a".repeat(AI_CODE_MAX_CHARS + 1)));
    }
}
