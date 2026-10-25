//! The Settings window, About, and the save and open panels. Part of the catalog in `strings.rs`, whose glossary holds here too.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    // Settings window
    ("hotkey", "Sneltoets", "Hotkey"),
    (
        "hotkey_note",
        "Opent de kaart bij de aanwijzer.",
        "Opens the card at the pointer.",
    ),
    ("group_general", "Algemeen", "General"),
    ("group_privacy", "Privacy", "Privacy"),
    ("group_display", "Weergave", "Display"),
    // Settings › Display › Card theme
    ("card_theme", "Kaartthema", "Card theme"),
    ("card_theme_system", "Systeem", "System"),
    ("card_theme_light", "Licht", "Light"),
    ("card_theme_dark", "Donker", "Dark"),
    ("card_theme_night", "Nacht", "Night"),
    (
        "card_theme_solarized_light",
        "Solarized licht",
        "Solarized Light",
    ),
    (
        "card_theme_solarized_dark",
        "Solarized donker",
        "Solarized Dark",
    ),
    ("card_theme_high_contrast", "Hoog contrast", "High contrast"),
    ("card_theme_sepia", "Sepia", "Sepia"),
    ("card_theme_nord", "Nord", "Nord"),
    ("card_theme_gruvbox_dark", "Gruvbox donker", "Gruvbox Dark"),
    ("card_theme_dracula", "Dracula", "Dracula"),
    ("card_theme_monokai", "Monokai", "Monokai"),
    (
        "card_theme_catppuccin_mocha",
        "Catppuccin Mocha",
        "Catppuccin Mocha",
    ),
    ("card_theme_tokyo_night", "Tokyo Night", "Tokyo Night"),
    ("card_theme_ocean", "Oceaan", "Ocean"),
    ("card_theme_forest", "Bos", "Forest"),
    (
        "card_theme_preview",
        "Voorbeeld van het kaartthema",
        "Card theme preview",
    ),
    ("change_hotkey", "Wijzig…", "Change…"),
    ("change_hotkey_a11y", "Sneltoets wijzigen", "Change hotkey"),
    (
        "press_shortcut",
        "Druk een sneltoets… (Esc annuleert)",
        "Press a shortcut… (Esc cancels)",
    ),
    (
        "hotkey_rejected",
        "⌃ of ⌘ nodig (alleen Option mag met § of `) — opnieuw",
        "Need ⌃ or ⌘ (Option alone only with § or `) — try again",
    ),
    ("date_order", "Datumvolgorde", "Date order"),
    (
        "date_order_note",
        "Hoe een datum in een tabel wordt gelezen.",
        "How a date in a table is read.",
    ),
    ("language", "Taal", "Language"),
    (
        "language_note",
        "Taal van de kaart en de menu's.",
        "Language of the card and the menus.",
    ),
    ("language_system", "Systeem", "System"),
    ("language_nl", "Nederlands", "Nederlands"),
    ("language_en", "English", "English"),
    (
        "privacy_filter",
        "Gebruik privacyfilter",
        "Use privacy filter",
    ),
    (
        "privacy_note",
        "Verbergt gekopieerde inhoud tot je klikt.",
        "Hides copied content until you click.",
    ),
    (
        "blur_masked",
        "Gemaskeerde inhoud vervagen",
        "Blur masked content",
    ),
    (
        "blur_note",
        "Maakt het masker wazig in plaats van vlak.",
        "Makes the mask blurred instead of flat.",
    ),
    ("open_at_login", "Openen bij inloggen", "Open at Login"),
    (
        "login_note",
        "Start CopyCraft wanneer je inlogt (standaard uit)",
        "Starts CopyCraft when you log in (off by default)",
    ),
    (
        "login_approval",
        "Wacht op goedkeuring in Systeeminstellingen › Inloggen",
        "Waiting for approval in System Settings › Login Items",
    ),
    (
        "login_unavailable",
        "Open at Login niet beschikbaar",
        "Open at Login is not available",
    ),
    (
        "login_not_found",
        "Openen bij inloggen is niet beschikbaar voor deze build",
        "Open at Login is not available for this build",
    ),
    // About
    ("about_title", "Over CopyCraft", "About CopyCraft"),
    ("version", "Versie", "Version"),
    ("privacy_policy", "Privacybeleid", "Privacy Policy"),
    ("support_link", "Support", "Support"),
    (
        "link_open_failed",
        "Kon de pagina niet openen in je browser.",
        "Could not open the page in your browser.",
    ),
    (
        "offline_promise",
        "Werkt offline. Gekopieerde tekst en afbeeldingen blijven op deze Mac — CopyCraft opent nooit een netwerkverbinding.",
        "Works offline. Copied text and images stay on this Mac — CopyCraft never opens a network connection.",
    ),
    // Panels, Settings
    ("save_panel_clipboard", "Bewaar klembord", "Save clipboard"),
    ("save_panel_picture", "Bewaar afbeelding", "Save image"),
    ("save_panel_table", "Bewaar tabel", "Save table"),
    (
        "login_change_failed",
        "Kan Openen bij inloggen niet wijzigen",
        "Could not change Open at Login",
    ),
    ("date_order_dmy", "dd/mm/jjjj", "dd/mm/yyyy"),
    ("date_order_mdy", "mm/dd/jjjj", "mm/dd/yyyy"),
];
