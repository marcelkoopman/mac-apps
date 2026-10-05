//! User-facing strings follow the system language (English or Dutch). No language picker.

/// Supported UI languages. Anything other than Dutch uses English.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Nl,
}

/// The language for labels this launch (from the preferred language list).
pub fn lang() -> Lang {
    #[cfg(target_os = "macos")]
    {
        use mac_ui::objc2_foundation::{NSLocale, NSString};
        let preferred = NSLocale::preferredLanguages();
        if preferred
            .firstObject()
            .and_then(|value| value.downcast_ref::<NSString>().map(|s| s.to_string()))
            .is_some_and(|code| code.to_ascii_lowercase().starts_with("nl"))
        {
            return Lang::Nl;
        }
    }
    Lang::En
}

/// Translate a UI key into the system language.
pub fn t(key: &str) -> &'static str {
    match (lang(), key) {
        // Tray / overflow
        (Lang::Nl, "settings") => "Instellingen…",
        (Lang::En, "settings") => "Settings…",
        (Lang::Nl, "about") => "Over Copycraft",
        (Lang::En, "about") => "About Copycraft",
        (Lang::Nl, "quit") => "Stop",
        (Lang::En, "quit") => "Quit",
        (Lang::Nl, "history") => "Geschiedenis",
        (Lang::En, "history") => "History",
        (Lang::Nl, "allow_screenshots") => "Schermafbeeldingen toestaan",
        (Lang::En, "allow_screenshots") => "Allow screenshots",
        (Lang::Nl, "empty_pasteboard") => "Klembord legen",
        (Lang::En, "empty_pasteboard") => "Empty pasteboard",
        (Lang::Nl, "clear_history") => "Geschiedenis wissen",
        (Lang::En, "clear_history") => "Clear history",
        (Lang::Nl, "clear_sensitive") => "Gevoelige kopieën na 60 s wissen",
        (Lang::En, "clear_sensitive") => "Clear sensitive copies after 60 s",
        (Lang::Nl, "appearance") => "Weergave",
        (Lang::En, "appearance") => "Appearance",
        (Lang::Nl, "theme_system") => "Systeem",
        (Lang::En, "theme_system") => "System",
        (Lang::Nl, "theme_light") => "Licht",
        (Lang::En, "theme_light") => "Light",
        (Lang::Nl, "theme_dark") => "Donker",
        (Lang::En, "theme_dark") => "Dark",

        // Settings window
        (Lang::Nl, "settings_title") => "Copycraft-instellingen",
        (Lang::En, "settings_title") => "Copycraft Settings",
        (Lang::Nl, "hotkey") => "Sneltoets",
        (Lang::En, "hotkey") => "Hotkey",
        (Lang::Nl, "change_hotkey") => "Wijzigen…",
        (Lang::En, "change_hotkey") => "Change…",
        (Lang::Nl, "change_hotkey_a11y") => "Sneltoets wijzigen",
        (Lang::En, "change_hotkey_a11y") => "Change hotkey",
        (Lang::Nl, "press_shortcut") => "Druk een sneltoets… (Esc annuleert)",
        (Lang::En, "press_shortcut") => "Press a shortcut… (Esc cancels)",
        (Lang::Nl, "hotkey_rejected") => "⌃ of ⌘ nodig (niet alleen Option) — opnieuw",
        (Lang::En, "hotkey_rejected") => "Need ⌃ or ⌘ (not Option alone) — try again",
        (Lang::Nl, "date_order") => "Datumvolgorde",
        (Lang::En, "date_order") => "Date order",
        (Lang::Nl, "blur_masked") => "Gemaskeerde inhoud vervagen",
        (Lang::En, "blur_masked") => "Blur masked content",
        (Lang::Nl, "open_at_login") => "Openen bij inloggen",
        (Lang::En, "open_at_login") => "Open at Login",
        (Lang::Nl, "login_note") => "Start Copycraft wanneer je inlogt (standaard uit)",
        (Lang::En, "login_note") => "Starts Copycraft when you log in (off by default)",
        (Lang::Nl, "login_approval") => "Wacht op goedkeuring in Systeeminstellingen › Inloggen",
        (Lang::En, "login_approval") => "Waiting for approval in System Settings › Login Items",
        (Lang::Nl, "login_unavailable") => "Openen bij inloggen vereist macOS 13 of nieuwer",
        (Lang::En, "login_unavailable") => "Open at Login needs macOS 13 or later",
        (Lang::Nl, "login_not_found") => "Openen bij inloggen is niet beschikbaar voor deze build",
        (Lang::En, "login_not_found") => "Open at Login is not available for this build",

        // About
        (Lang::Nl, "about_title") => "Over Copycraft",
        (Lang::En, "about_title") => "About Copycraft",
        (Lang::Nl, "version") => "Versie",
        (Lang::En, "version") => "Version",
        (Lang::Nl, "offline_promise") => {
            "Werkt offline. Gekopieerde tekst en afbeeldingen blijven op deze Mac — Copycraft opent nooit een netwerkverbinding."
        }
        (Lang::En, "offline_promise") => {
            "Works offline. Copied text and pictures stay on this Mac — Copycraft never opens a network connection."
        }

        // Card chips (common)
        (Lang::Nl, "copy") => "Kopieer",
        (Lang::En, "copy") => "Copy",
        (Lang::Nl, "save") => "Bewaar",
        (Lang::En, "save") => "Save",
        (Lang::Nl, "original") => "Origineel",
        (Lang::En, "original") => "Original",
        (Lang::Nl, "table_menu") => "Tabel ▾",
        (Lang::En, "table_menu") => "Table ▾",
        (Lang::Nl, "table_view") => "Tabel",
        (Lang::En, "table_view") => "Table",
        (Lang::Nl, "image_menu") => "Beeld ▾",
        (Lang::En, "image_menu") => "Image ▾",
        (Lang::Nl, "show_columns") => "Toon kolommen",
        (Lang::En, "show_columns") => "Show columns",
        (Lang::Nl, "show_table") => "Toon tabel",
        (Lang::En, "show_table") => "Show table",

        // Fallback: English (every key above has an En arm; this is for typos).
        (_, _) => {
            eprintln!("copycraft: missing locale key {key}");
            key_en(key)
        }
    }
}

fn key_en(key: &str) -> &'static str {
    // Known keys only — never return the borrowed `key`.
    match key {
        "settings" => "Settings…",
        "about" => "About Copycraft",
        "quit" => "Quit",
        "history" => "History",
        "allow_screenshots" => "Allow screenshots",
        "empty_pasteboard" => "Empty pasteboard",
        "clear_history" => "Clear history",
        "clear_sensitive" => "Clear sensitive copies after 60 s",
        "appearance" => "Appearance",
        "theme_system" => "System",
        "theme_light" => "Light",
        "theme_dark" => "Dark",
        "settings_title" => "Copycraft Settings",
        "hotkey" => "Hotkey",
        "change_hotkey" => "Change…",
        "change_hotkey_a11y" => "Change hotkey",
        "press_shortcut" => "Press a shortcut… (Esc cancels)",
        "hotkey_rejected" => "Need ⌃ or ⌘ (not Option alone) — try again",
        "date_order" => "Date order",
        "blur_masked" => "Blur masked content",
        "open_at_login" => "Open at Login",
        "login_note" => "Starts Copycraft when you log in (off by default)",
        "login_approval" => "Waiting for approval in System Settings › Login Items",
        "login_unavailable" => "Open at Login needs macOS 13 or later",
        "login_not_found" => "Open at Login is not available for this build",
        "about_title" => "About Copycraft",
        "version" => "Version",
        "offline_promise" => {
            "Works offline. Copied text and pictures stay on this Mac — Copycraft never opens a network connection."
        }
        "copy" => "Copy",
        "save" => "Save",
        "original" => "Original",
        "table_menu" => "Table ▾",
        "table_view" => "Table",
        "image_menu" => "Image ▾",
        "show_columns" => "Show columns",
        "show_table" => "Show table",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, lang, t};

    #[test]
    fn linux_and_unknown_prefer_english() {
        assert_eq!(lang(), Lang::En);
        assert_eq!(t("settings"), "Settings…");
        assert_eq!(t("about"), "About Copycraft");
        assert_eq!(t("quit"), "Quit");
    }
}
