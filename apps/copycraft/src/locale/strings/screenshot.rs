//! Screenshots: the command, its hotkey, the Screen Recording permission and the setting that
//! lets CopyCraft's own windows in.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    (
        "take_screenshot",
        "Maak schermafbeelding…",
        "Take Screenshot…",
    ),
    (
        "detail_screenshot",
        "Een gebied, een venster of het hele scherm, naar het klembord",
        "An area, a window or the whole screen, to the clipboard",
    ),
    (
        "screenshot_unavailable",
        "Schermafbeeldingen werken niet op deze Mac",
        "Screenshots don't work on this Mac",
    ),
    ("screenshot_hotkey", "Schermafbeelding", "Screenshot"),
    (
        "screenshot_secret_notice",
        "Screenshot bevat mogelijk een geheim",
        "Screenshot may contain a secret",
    ),
    (
        "screenshot_secret_notice_detail",
        "Op het klembord gemarkeerd als privé en niet in de geschiedenis bewaard",
        "Marked private on the clipboard and not kept in history",
    ),
    (
        "screenshot_thumbnail_a11y",
        "Schermafbeelding — klik om te openen, sleep om te delen",
        "Screenshot — click to open, drag to share",
    ),
    (
        "screenshot_hotkey_note",
        "Sleep een gebied, klik voor het hele scherm, Spatie kiest een venster.",
        "Drag an area, click for the whole screen, Space picks a window.",
    ),
    (
        "change_screenshot_hotkey_a11y",
        "Sneltoets voor schermafbeelding wijzigen",
        "Change screenshot hotkey",
    ),
    (
        "hotkey_in_use",
        "Al in gebruik voor de andere sneltoets — opnieuw",
        "Already used by the other hotkey — try again",
    ),
    (
        "screenshot_includes_own",
        "Toon CopyCraft in schermafbeeldingen",
        "Show CopyCraft in screenshots",
    ),
    (
        "screenshot_includes_own_note",
        "Anders blijven de kaart en het tabelvenster erbuiten.",
        "Otherwise the card and the table window are left out.",
    ),
    (
        "screenshot_ask_title",
        "Schermafbeeldingen maken?",
        "Take screenshots?",
    ),
    (
        "screenshot_ask_message",
        "Voor een schermafbeelding heeft CopyCraft toestemming voor schermopname nodig. De afbeelding gaat alleen naar het klembord en de geschiedenis op je Mac, nooit naar het internet, en nooit naar een bestand, behalve als je hem zelf ergens heen sleept. Na Doorgaan vraagt macOS om toestemming.",
        "For a screenshot CopyCraft needs Screen Recording permission. The image only goes to the clipboard and the history on your Mac: never to the internet, and never to a file unless you drag it somewhere yourself. After Continue, macOS asks for permission.",
    ),
    (
        "screenshot_denied_title",
        "Schermopname staat uit voor CopyCraft",
        "Screen Recording is off for CopyCraft",
    ),
    (
        "screenshot_denied_message",
        "Zet CopyCraft aan in Systeeminstellingen › Privacy en beveiliging › Schermopname en systeemaudio. Misschien moet je CopyCraft daarna opnieuw openen.",
        "Turn on CopyCraft in System Settings › Privacy & Security › Screen & System Audio Recording. You may need to open CopyCraft again after that.",
    ),
    // Run outside an .app (`cargo run`): macOS gives the permission to the app that started it.
    (
        "screenshot_denied_unbundled_title",
        "Schermopname staat uit voor de app die CopyCraft startte",
        "Screen Recording is off for the app that started CopyCraft",
    ),
    (
        "screenshot_denied_unbundled_message",
        "Gestart via terminal of editor. Zet die app (bijv. Zed of Terminal) aan in Systeeminstellingen › Privacy en beveiliging › Schermopname en systeemaudio, en herstart hem. Of open de gebundelde CopyCraft.app.",
        "Started from a terminal or editor. Turn on that app (such as Zed or Terminal) in System Settings › Privacy & Security › Screen & System Audio Recording, and restart it. Or open the bundled CopyCraft.app.",
    ),
    (
        "screenshot_open_privacy",
        "Open Privacy en beveiliging…",
        "Open Privacy & Security…",
    ),
];
