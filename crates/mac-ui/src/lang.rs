//! UI language for the shared panels (alerts, open and save).
//!
//! [`lang`] is the macOS preferred language until an app calls [`set_lang`]. Dutch is the only
//! preferred language that selects [`Lang::Nl`]; anything else is English. The choice can change
//! while the app runs ([`set_lang`]), so it is not frozen in a `OnceLock`.

use std::sync::atomic::{AtomicU8, Ordering};

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    En = 0,
    Nl = 1,
}

/// Not chosen yet: [`lang`] follows [`system_lang`].
const UNSET: u8 = 2;

static CURRENT: AtomicU8 = AtomicU8::new(UNSET);

/// Use `lang` for shared panel labels until the next [`set_lang`].
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Release);
}

/// The language shared panels show. The macOS preferred language when nothing was set.
pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Acquire) {
        0 => Lang::En,
        1 => Lang::Nl,
        _ => system_lang(),
    }
}

/// English, or Dutch when the macOS preferred language starts with `nl`. English off macOS.
pub fn system_lang() -> Lang {
    #[cfg(target_os = "macos")]
    {
        use crate::objc2_foundation::{NSLocale, NSString};
        let preferred = NSLocale::preferredLanguages();
        if preferred
            .firstObject()
            .and_then(|value| {
                value
                    .downcast_ref::<NSString>()
                    .map(|code| code.to_string())
            })
            .is_some_and(|code| code.to_ascii_lowercase().starts_with("nl"))
        {
            return Lang::Nl;
        }
    }
    Lang::En
}

/// `(key, Dutch, English)`. Both texts are non-empty; a test checks that.
const STRINGS: &[(&str, &str, &str)] = &[
    ("ok", "OK", "OK"),
    ("cancel", "Annuleer", "Cancel"),
    ("format", "Formaat:", "Format:"),
    // Default button of the open panel.
    ("open", "Open", "Open"),
    // Default button of the save panel.
    ("save", "Bewaar", "Save"),
    // VoiceOver name of a busy wheel before an app sets its own.
    ("loading", "Laden", "Loading"),
];

fn entry(key: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    STRINGS.iter().find(|(name, _, _)| *name == key)
}

/// `key` in `lang`. An unknown key is logged and shows as `?`.
pub fn t_in(lang: Lang, key: &str) -> &'static str {
    match entry(key) {
        Some((_, nl, en)) => match lang {
            Lang::Nl => nl,
            Lang::En => en,
        },
        None => {
            eprintln!("mac-ui: missing lang key {key}");
            "?"
        }
    }
}

/// `key` in [`lang`].
pub fn t(key: &str) -> &'static str {
    t_in(lang(), key)
}

/// Whether `label` is the cancel button in either language. Escape is tied to this, not to the
/// English title alone, so "Annuleer" still cancels when the running language is English.
pub fn is_cancel(label: &str) -> bool {
    entry("cancel").is_some_and(|(_, nl, en)| label == *nl || label == *en)
}

#[cfg(test)]
mod tests {
    use super::{Lang, STRINGS, is_cancel, set_lang, system_lang, t, t_in};

    #[test]
    fn every_key_has_dutch_and_english() {
        let mut seen = std::collections::HashSet::new();
        for (key, nl, en) in STRINGS {
            assert!(seen.insert(*key), "duplicate key {key}");
            assert!(!key.is_empty() && !nl.is_empty() && !en.is_empty(), "{key}");
            assert_eq!(t_in(Lang::Nl, key), *nl, "{key}");
            assert_eq!(t_in(Lang::En, key), *en, "{key}");
        }
    }

    #[test]
    fn cancel_matches_both_languages() {
        assert!(is_cancel("Cancel"));
        assert!(is_cancel("Annuleer"));
        assert!(!is_cancel("OK"));
        assert!(!is_cancel("Reset"));
        assert!(!is_cancel("Sluit"));
    }

    #[test]
    fn set_lang_switches_the_labels() {
        let previous = super::lang();
        set_lang(Lang::Nl);
        assert_eq!(t("cancel"), "Annuleer");
        assert_eq!(t("format"), "Formaat:");
        assert_eq!(t("save"), "Bewaar");
        set_lang(Lang::En);
        assert_eq!(t("cancel"), "Cancel");
        assert_eq!(t("open"), "Open");
        assert_eq!(t("loading"), "Loading");
        set_lang(previous);
    }

    #[test]
    fn system_language_is_english_or_dutch() {
        let lang = system_lang();
        assert!(matches!(lang, Lang::En | Lang::Nl));
        // Off macOS there is no preferred-language list, so the fallback is English.
        #[cfg(not(target_os = "macos"))]
        assert_eq!(lang, Lang::En);
    }
}
