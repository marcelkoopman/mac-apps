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
    match crate::settings::language() {
        crate::settings::Language::Nl => Lang::Nl,
        crate::settings::Language::En => Lang::En,
        crate::settings::Language::System => system_lang(),
    }
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
            eprintln!("copycraft: missing locale key {key}");
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
    use super::{Lang, lang, strings::STRINGS, system_lang, t, t_in, tf_in};
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn system_follows_the_platform_language_and_a_choice_overrides_it() {
        let _lock = crate::settings::language_test_lock();
        let previous = crate::settings::load().language;
        struct Restore(crate::settings::Language);
        impl Drop for Restore {
            fn drop(&mut self) {
                crate::settings::set_language(self.0);
                crate::settings::clear_language_choice();
                super::apply();
            }
        }
        let _restore = Restore(previous);
        crate::settings::clear_language_choice();
        assert_eq!(lang(), system_lang());
        assert_eq!(t("settings"), t_in(system_lang(), "settings"));
        crate::settings::set_language(crate::settings::Language::Nl);
        assert_eq!(lang(), Lang::Nl);
        assert_eq!(t("settings"), "Instellingen…");
        assert_eq!(t("about"), "Over Copycraft");
        crate::settings::set_language(crate::settings::Language::En);
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
    fn placeholders_are_filled() {
        assert_eq!(tf_in(Lang::En, "item_of", &[&2, &5]), "Item 2 of 5");
        assert_eq!(tf_in(Lang::Nl, "item_of", &[&2, &5]), "Item 2 van 5");
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
            for call in ["t(", "tf(", "t_in(lang,", "tf_in(lang,", "_key:"] {
                let mut rest = text.as_str();
                while let Some(at) = rest.find(call) {
                    let before = rest[..at].chars().next_back();
                    rest = &rest[at + call.len()..];
                    // `format(\"`, `set(\"` and the like are not catalog calls.
                    if call != "_key:" && before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
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
