//! User-facing strings follow Settings › Language: System (the macOS preferred language),
//! Nederlands or English. Every string the user can see goes through [`t`] / [`tf`] with a key
//! from `locale/strings.rs`, which holds the Dutch and the English text side by side.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

mod strings;

pub use mac_ui::lang::Lang;

/// The macOS preferred language (English when that is not Dutch). English off macOS.
pub fn system_lang() -> Lang {
    mac_ui::lang::system_lang()
}

/// The language for labels: the stored choice, or [`system_lang`] when that choice is System.
pub fn lang() -> Lang {
    match chosen_language() {
        crate::settings::Language::Nl => Lang::Nl,
        crate::settings::Language::En => Lang::En,
        crate::settings::Language::System => system_lang(),
    }
}

#[cfg(not(test))]
fn chosen_language() -> crate::settings::Language {
    crate::settings::language()
}

// Tests run in parallel on Macs set to any language: each test thread gets English
// unless it picks a language itself, and never touches the stored setting.
#[cfg(test)]
thread_local! {
    static TEST_LANGUAGE: std::cell::Cell<crate::settings::Language> =
        const { std::cell::Cell::new(crate::settings::Language::En) };
}

#[cfg(test)]
fn chosen_language() -> crate::settings::Language {
    TEST_LANGUAGE.with(|c| c.get())
}

#[cfg(test)]
pub(crate) fn set_test_language(language: crate::settings::Language) {
    TEST_LANGUAGE.with(|c| c.set(language));
}

/// Point shared panels ([`mac_ui::lang`]) at the same language as [`lang`].
pub fn apply() {
    mac_ui::lang::set_lang(lang());
}

/// Look up a catalog entry by key.
fn entry(key: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    static INDEX: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        strings::STRINGS
            .iter()
            .enumerate()
            .map(|(at, (key, _, _))| (*key, at))
            .collect()
    });
    index.get(key).map(|at| &strings::STRINGS[*at])
}

/// Translate a UI key into `lang`. An unknown key is a typo: it is logged and shows as `?`
/// (a unit test checks that every key used in the sources is in the catalog).
pub fn t_in(lang: Lang, key: &str) -> &'static str {
    match entry(key) {
        Some((_, nl, en)) => match lang {
            Lang::Nl => nl,
            Lang::En => en,
        },
        None => {
            crate::diag::log(crate::diag::LogEvent::MissingLocaleKey);
            "?"
        }
    }
}

/// Translate a UI key into the chosen language.
pub fn t(key: &str) -> &'static str {
    t_in(lang(), key)
}

/// Fill `{0}`, `{1}`, … of `template` with `args`.
fn fill(template: &str, args: &[&dyn Display]) -> String {
    // One pass, so an argument that looks like `{1}` is never filled in again.
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let filled = rest
            .strip_prefix('{')
            .and_then(|tail| tail.split_once('}'))
            .and_then(|(index, tail)| Some((index.parse::<usize>().ok()?, tail)))
            .and_then(|(index, tail)| Some((args.get(index)?, tail)));
        match filled {
            Some((arg, tail)) => {
                out.push_str(&arg.to_string());
                rest = tail;
            }
            None => {
                out.push('{');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Translate a UI key into `lang` and fill its `{0}`, `{1}`, … placeholders.
pub fn tf_in(lang: Lang, key: &str, args: &[&dyn Display]) -> String {
    fill(t_in(lang, key), args)
}

/// Translate a UI key into the chosen language and fill its `{0}`, `{1}`, … placeholders.
pub fn tf(key: &str, args: &[&dyn Display]) -> String {
    tf_in(lang(), key, args)
}

/// "Gekopieerd: €" / "Copied: €". The character is not a catalog key, so it is not in [`t`].
pub fn copied_symbol(lang: Lang, symbol: &str) -> String {
    match lang {
        Lang::Nl => format!("Gekopieerd: {symbol}"),
        Lang::En => format!("Copied: {symbol}"),
    }
}

/// Tooltip on a character button: "Kopieer €" / "Copy €".
pub fn copy_symbol(lang: Lang, symbol: &str) -> String {
    match lang {
        Lang::Nl => format!("Kopieer {symbol}"),
        Lang::En => format!("Copy {symbol}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, lang, set_test_language, strings::STRINGS, system_lang, t, t_in, tf_in};
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn system_follows_the_platform_language_and_a_choice_overrides_it() {
        let _lock = crate::settings::language_test_lock();
        set_test_language(crate::settings::Language::System);
        assert_eq!(lang(), system_lang());
        assert_eq!(t("settings"), t_in(system_lang(), "settings"));
        set_test_language(crate::settings::Language::Nl);
        assert_eq!(lang(), Lang::Nl);
        assert_eq!(t("settings"), "Instellingen…");
        assert_eq!(t("about"), "Over CopyCraft");
        set_test_language(crate::settings::Language::En);
        assert_eq!(lang(), Lang::En);
        assert_eq!(t("settings"), "Settings…");
        assert_eq!(t("quit"), "Quit");
    }

    #[test]
    fn every_key_has_dutch_and_english() {
        let mut seen = HashSet::new();
        for (key, nl, en) in STRINGS {
            assert!(seen.insert(*key), "duplicate key {key}");
            assert!(!key.is_empty() && !nl.is_empty() && !en.is_empty(), "{key}");
            assert_eq!(t_in(Lang::Nl, key), *nl, "{key}");
            assert_eq!(t_in(Lang::En, key), *en, "{key}");
            // The same placeholders in both languages.
            for index in 0..6 {
                let tag = format!("{{{index}}}");
                assert_eq!(
                    nl.contains(&tag),
                    en.contains(&tag),
                    "{key}: placeholder {tag}"
                );
            }
            assert_eq!(nl.matches('{').count(), en.matches('{').count(), "{key}");
            // Filled in either language, no placeholder is left over.
            let args: [&dyn std::fmt::Display; 6] = [&"A", &"B", &"C", &"D", &"E", &"F"];
            for lang in [Lang::Nl, Lang::En] {
                let filled = tf_in(lang, key, &args);
                assert!(
                    !(0..6).any(|i| filled.contains(&format!("{{{i}}}"))),
                    "{key}"
                );
            }
        }
    }

    #[test]
    fn dutch_differs_from_english_unless_listed() {
        // Words and names that are the same in both languages.
        const SAME: &[&str] = &[
            // Case style names are written in the style itself.
            "case_camel",
            "case_pascal",
            "case_snake",
            "case_kebab",
            "case_constant",
            "agent_tokens",
            "window_compact",
            "label_pii",
            "title_link",
            "title_youtube",
            "title_qr",
            "detail_uri",
            "chip_schema",
            "chip_diff",
            "chip_info",
            "image_pixels_line",
            "overview_type",
            "language_nl",
            "language_en",
            "group_privacy",
            "group_feedback",
            // The same word in Dutch.
            "support_link",
            "sep_tab",
            "text_filter",
            "filter_label",
            // "zoom 125%": the same word.
            "pdf_zoom",
            // A colour name, the same word in Dutch.
            "card_theme_sepia",
            // Card theme brand names, the same in Dutch.
            "card_theme_nord",
            "card_theme_dracula",
            "card_theme_monokai",
            "card_theme_catppuccin_mocha",
            "card_theme_tokyo_night",
            // Brand name and a two-letter label.
            "ai_group",
            "ai_button",
            "ai_check_title",
        ];
        for (key, nl, en) in STRINGS {
            if nl == en {
                assert!(
                    SAME.contains(key),
                    "{key} is the same in Dutch and English: {nl}"
                );
            }
        }
    }

    #[test]
    fn paste_first_keys_are_in_both_languages() {
        for key in [
            "clipboard_history",
            "clipboard_history_note",
            "welcome_history_choice",
            "paste_here_hint",
            "paste_nothing",
            "pasted_text",
            "pasted_image",
        ] {
            let (_, nl, en) = STRINGS
                .iter()
                .find(|(k, _, _)| *k == key)
                .unwrap_or_else(|| panic!("{key} is missing"));
            assert!(!nl.is_empty() && !en.is_empty() && nl != en, "{key}");
        }
        assert_eq!(
            t_in(Lang::En, "clipboard_history"),
            "Keep clipboard history"
        );
        assert_eq!(
            t_in(Lang::Nl, "clipboard_history"),
            "Klembordgeschiedenis bijhouden"
        );
        assert_eq!(
            t_in(Lang::En, "paste_here_hint"),
            "Paste here (⌘V) or turn on history in Settings"
        );
        assert_eq!(
            t_in(Lang::Nl, "paste_here_hint"),
            "Plak hier (⌘V) of zet geschiedenis aan in Instellingen"
        );
    }

    #[test]
    fn welcome_keys_are_in_both_languages() {
        for key in [
            "welcome_menu",
            "welcome_title",
            "welcome_hotkey",
            "welcome_history",
            "welcome_local",
            "welcome_sensitive",
            "welcome_start",
            "open_at_login",
            "settings",
        ] {
            let (_, nl, en) = STRINGS
                .iter()
                .find(|(k, _, _)| *k == key)
                .unwrap_or_else(|| panic!("{key} is missing"));
            assert!(!nl.is_empty() && !en.is_empty(), "{key}");
        }
        assert_eq!(t_in(Lang::En, "welcome_title"), "Welcome to CopyCraft");
        assert_eq!(t_in(Lang::Nl, "welcome_title"), "Welkom bij CopyCraft");
        assert_eq!(
            tf_in(Lang::En, "welcome_hotkey", &[&"⌃⌘C"]),
            "Open CopyCraft from any app with ⌃⌘C."
        );
    }

    /// ⌘-click on a strip card compares it; a compared card says its side.
    #[test]
    fn strip_compare_is_in_both_languages() {
        assert_eq!(
            t_in(Lang::Nl, "strip_compare_hint"),
            "⌘-klik om te vergelijken met de getoonde kopie"
        );
        assert_eq!(
            t_in(Lang::En, "strip_compare_hint"),
            "⌘-click to compare with the copy on screen"
        );
        assert_eq!(
            tf_in(Lang::Nl, "strip_compare_side", &[&"A"]),
            "vergelijking, kant A"
        );
        assert_eq!(
            tf_in(Lang::En, "strip_compare_side", &[&"B"]),
            "compared, side B"
        );
        assert_eq!(
            t_in(Lang::Nl, "strip_compare_tip"),
            "⌘-klik op een tegel om te vergelijken"
        );
        assert_eq!(
            t_in(Lang::En, "strip_compare_tip"),
            "⌘-click a tile to compare"
        );
        for key in [
            "strip_compare_hint",
            "strip_compare_side",
            "strip_compare_tip",
        ] {
            assert_ne!(t_in(Lang::Nl, key), t_in(Lang::En, key), "{key}");
        }
    }

    /// The strip's ✕ and the card's Clear All button.
    #[test]
    fn history_remove_and_clear_all_are_in_both_languages() {
        assert_eq!(
            t_in(Lang::Nl, "history_remove_item"),
            "Verwijder uit geschiedenis"
        );
        assert_eq!(t_in(Lang::En, "history_remove_item"), "Remove from history");
        assert_eq!(t_in(Lang::Nl, "wipe"), "Alles wissen");
        assert_eq!(t_in(Lang::En, "wipe"), "Clear All");
        assert_eq!(t_in(Lang::Nl, "clear_confirm_title"), "Alles wissen?");
        assert_eq!(t_in(Lang::En, "clear_confirm_title"), "Clear All?");
        assert_eq!(t_in(Lang::Nl, "clear_confirm_action"), "Alles wissen");
        assert_eq!(t_in(Lang::En, "clear_confirm_action"), "Clear All");
        // The tooltip starts with the same words as the label.
        for lang in [Lang::Nl, Lang::En] {
            assert!(
                t_in(lang, "wipe_tip").starts_with(t_in(lang, "wipe")),
                "{lang:?}"
            );
        }
        for key in [
            "history_remove_item",
            "wipe",
            "clear_confirm_title",
            "confirm_clear_note",
        ] {
            assert_ne!(t_in(Lang::Nl, key), t_in(Lang::En, key), "{key}");
        }
    }

    /// The About and Settings links to the privacy policy and support page.
    #[test]
    fn page_links_are_in_both_languages() {
        assert_eq!(t_in(Lang::Nl, "privacy_policy"), "Privacybeleid");
        assert_eq!(t_in(Lang::En, "privacy_policy"), "Privacy Policy");
        assert_eq!(t_in(Lang::Nl, "support_link"), "Support");
        assert_eq!(t_in(Lang::En, "support_link"), "Support");
        assert_ne!(
            t_in(Lang::Nl, "link_open_failed"),
            t_in(Lang::En, "link_open_failed")
        );
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(tf_in(Lang::En, "item_of", &[&2, &5]), "Item 2 of 5");
        assert_eq!(tf_in(Lang::Nl, "item_of", &[&2, &5]), "Item 2 van 5");
    }

    /// The words of `text` that spell the app's name in another case than `CopyCraft`. Links
    /// and identifiers (a word with `.`, `/`, `:` or `_` inside, such as the bundle id
    /// `nl.marcelkoopman.copycraft` or a GitHub Pages URL) keep their lowercase spelling, and so
    /// do names that only start with it (`CopycraftAppearance`, a defaults key or class name).
    fn misspelled_app_names(text: &str) -> Vec<String> {
        text.split(|c: char| c.is_whitespace() || "\"'`()[]{}<>,;*!?|".contains(c))
            .map(|word| word.trim_end_matches(['.', ':']))
            .filter(|word| !word.contains(['.', '/', ':', '_']))
            .flat_map(|word| word.split('-'))
            .filter(|word| {
                word.to_lowercase()
                    .strip_prefix("copycraft")
                    .is_some_and(str::is_empty)
                    && !word.starts_with("CopyCraft")
            })
            .map(str::to_string)
            .collect()
    }

    /// The words of `text` (lowercase, split at anything but letters) that start with one of
    /// `stems` and are not one of `allowed`.
    fn forbidden_terms(text: &str, stems: &[&str], allowed: &[&str]) -> Vec<String> {
        text.split(|c: char| !c.is_alphabetic())
            .map(str::to_lowercase)
            .filter(|word| {
                stems.iter().any(|stem| word.starts_with(stem)) && !allowed.contains(&word.as_str())
            })
            .collect()
    }

    /// Dutch words the glossary in `strings.rs` rules out, as word starts.
    const NL_FORBIDDEN: &[&str] = &["historie", "beeld", "plaatje", "foto", "feature"];
    /// Dutch compounds that start like a forbidden word but are their own term.
    const NL_ALLOWED: &[&str] = &["beeldverhouding"];
    /// English words the glossary rules out.
    const EN_FORBIDDEN: &[&str] = &["pasteboard", "picture", "photo"];

    #[test]
    fn forbidden_terms_spots_word_starts_only() {
        assert_eq!(
            forbidden_terms("Historiestrook tonen", NL_FORBIDDEN, NL_ALLOWED),
            ["historiestrook"]
        );
        assert_eq!(
            forbidden_terms("Toon beeld", NL_FORBIDDEN, NL_ALLOWED),
            ["beeld"]
        );
        assert!(
            forbidden_terms("Voorbeeld, Beeldverhouding: 4:3", NL_FORBIDDEN, NL_ALLOWED).is_empty()
        );
        assert!(forbidden_terms("Toon afbeelding", NL_FORBIDDEN, NL_ALLOWED).is_empty());
        assert_eq!(
            forbidden_terms("Empty pasteboard", EN_FORBIDDEN, &[]),
            ["pasteboard"]
        );
        assert!(forbidden_terms("Show image", EN_FORBIDDEN, &[]).is_empty());
    }

    /// Every visible string follows the glossary at the head of `strings.rs`: Geschiedenis,
    /// afbeelding, Idee; history, image, clipboard.
    #[test]
    fn visible_strings_follow_the_glossary() {
        for (key, nl, en) in STRINGS {
            let wrong = forbidden_terms(nl, NL_FORBIDDEN, NL_ALLOWED);
            assert!(
                wrong.is_empty(),
                "{key} (nl): {wrong:?} is not in the glossary"
            );
            let wrong = forbidden_terms(en, EN_FORBIDDEN, &[]);
            assert!(
                wrong.is_empty(),
                "{key} (en): {wrong:?} is not in the glossary"
            );
        }
        assert_eq!(t_in(Lang::Nl, "strip_group"), "Geschiedenis");
        assert_eq!(t_in(Lang::En, "image_cannot_read"), "Can't read this image");
    }

    #[test]
    fn misspelled_app_names_spots_only_visible_words() {
        assert_eq!(misspelled_app_names("Over Copycraft"), ["Copycraft"]);
        assert_eq!(misspelled_app_names("Quit copycraft."), ["copycraft"]);
        assert_eq!(misspelled_app_names("Copycraft's card"), ["Copycraft"]);
        assert!(misspelled_app_names("Over CopyCraft, CopyCraft's card.").is_empty());
        assert!(misspelled_app_names("nl.marcelkoopman.copycraft").is_empty());
        assert!(
            misspelled_app_names("https://marcelkoopman.github.io/mac-apps/copycraft").is_empty()
        );
        assert!(misspelled_app_names("CopycraftAppearance").is_empty());
        assert_eq!(
            misspelled_app_names("Copycraft-instellingen"),
            ["Copycraft"]
        );
    }

    /// Every Dutch and English string spells the app name `CopyCraft`; only links and
    /// identifiers keep it lowercase.
    #[test]
    fn visible_strings_spell_the_app_name_copycraft() {
        for (key, nl, en) in STRINGS {
            for text in [nl, en] {
                let wrong = misspelled_app_names(text);
                assert!(wrong.is_empty(), "{key}: {wrong:?} should be CopyCraft");
            }
        }
        assert_eq!(t_in(Lang::En, "about"), "About CopyCraft");
        assert_eq!(t_in(Lang::Nl, "detail_quit"), "CopyCraft stoppen");
        assert_eq!(t_in(Lang::En, "detail_quit"), "Quit CopyCraft");
        assert_eq!(crate::notifications::TITLE, "CopyCraft");
    }

    /// The literals in the app's own code (menu bar tooltip, About, notification titles, the
    /// feedback mail) spell the name `CopyCraft` as well. Test code is skipped: its synthetic
    /// copies may say anything.
    #[test]
    fn source_literals_spell_the_app_name_copycraft() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![dir];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("read dir") {
                    files.push(entry.expect("entry").path());
                }
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let in_tests = path.components().any(|part| part.as_os_str() == "tests");
            if !name.ends_with(".rs")
                || in_tests
                || name == "tests.rs"
                || name.ends_with("_tests.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source");
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            let wrong = misspelled_app_names(code)
                .into_iter()
                // Lowercase `copycraft` in code is the crate, a file or thread name.
                .filter(|word| word.starts_with('C'))
                .collect::<Vec<_>>();
            assert!(
                wrong.is_empty(),
                "{}: {wrong:?} should be CopyCraft",
                path.display()
            );
        }
    }

    /// Every `t("key")` / `tf("key", …)` in the sources names a catalog key, and every catalog
    /// key is used by some call (so no translation is left behind unused).
    #[test]
    fn keys_in_sources_are_in_the_catalog() {
        let keys: HashSet<&str> = STRINGS.iter().map(|(key, _, _)| *key).collect();
        let mut used = HashSet::new();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![dir];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                for entry in std::fs::read_dir(&path).expect("read dir") {
                    files.push(entry.expect("entry").path());
                }
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs")
                || path.ends_with("strings.rs")
                || path.ends_with("locale.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source");
            // `t_in`/`tf_in` take the language first, under any name (`lang`, `*lang`, `Lang::En`).
            for call in ["t(", "tf(", "t_in(", "tf_in(", "_key:"] {
                let mut rest = text.as_str();
                while let Some(at) = rest.find(call) {
                    let before = rest[..at].chars().next_back();
                    rest = &rest[at + call.len()..];
                    // `format(\"`, `set(\"` and the like are not catalog calls.
                    if call != "_key:" && before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    if call.ends_with("_in(") {
                        // Skip the language argument: up to the first comma, if it comes
                        // before the call closes.
                        match (rest.find(','), rest.find(')')) {
                            (Some(comma), Some(close)) if comma < close => {
                                rest = &rest[comma + 1..];
                            }
                            _ => continue,
                        }
                    }
                    // The key may sit on the next line.
                    let Some(quoted) = rest.trim_start().strip_prefix('"') else {
                        continue;
                    };
                    rest = quoted;
                    let Some(end) = rest.find('"') else { break };
                    let key = &rest[..end];
                    if key
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    {
                        assert!(
                            keys.contains(key),
                            "{}: unknown locale key {key}",
                            path.display()
                        );
                        used.insert(key.to_string());
                    }
                }
            }
        }
        // Translated ahead of the table window's sidebar and table toggles.
        const RESERVED: &[&str] = &["show_columns", "show_table"];
        for key in keys {
            assert!(
                used.contains(key) || RESERVED.contains(&key),
                "locale key {key} is never used"
            );
        }
    }
}
