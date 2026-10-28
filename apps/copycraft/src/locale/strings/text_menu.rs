//! The Text ▾ menu: its sections, encodings, hashes, Clean URL, case styles and Format JSON. Part of the catalog in `strings.rs`, whose glossary holds here too.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    // Text ▾ menu (text_ops)
    ("text_menu", "Tekst ▾", "Text ▾"),
    ("text_section_lines", "Regels", "Lines"),
    ("text_section_convert", "Omzetten", "Convert"),
    // Text ▾ › Convert › Encoding (src/text_ops/encoding.rs)
    ("encoding_menu", "Codering", "Encoding"),
    // Text ▾ › Convert › Hash ›: SHA-256, SHA-1, MD5 ("hash" is Dutch too).
    ("hash_menu", "Hash", "Hash"),
    // Text ▾ › Convert › Code (src/text_ops/code_style.rs); "Code" is Dutch too.
    ("code_menu", "Code", "Code"),
    ("code_format_yaml", "YAML opmaken", "Format YAML"),
    ("code_minify_yaml", "YAML op één regel", "YAML on One Line"),
    ("code_yaml_to_json", "YAML naar JSON", "YAML to JSON"),
    ("code_json_to_yaml", "JSON naar YAML", "JSON to YAML"),
    ("code_format_xml", "XML opmaken", "Format XML"),
    ("code_minify_xml", "XML verkleinen", "Minify XML"),
    ("code_format_sql", "SQL opmaken", "Format SQL"),
    ("code_minify_sql", "SQL verkleinen", "Minify SQL"),
    // A curl command as code (src/text_ops/curl.rs): text only, nothing is run.
    (
        "code_curl_fetch",
        "Als JavaScript (fetch)",
        "As JavaScript (fetch)",
    ),
    (
        "code_curl_python",
        "Als Python (requests)",
        "As Python (requests)",
    ),
    ("code_curl_httpie", "Als HTTPie", "As HTTPie"),
    (
        "code_curl_reqwest",
        "Als Rust (reqwest)",
        "As Rust (reqwest)",
    ),
    (
        "code_curl_swift",
        "Als Swift (URLSession)",
        "As Swift (URLSession)",
    ),
    (
        "detail_code",
        "Opmaken, verkleinen of omzetten als nieuwe kopie; ⌘Z toont de vorige",
        "Format, minify or convert as a new copy; ⌘Z shows the previous one",
    ),
    // `⋯` submenus: the card themes, and help and feedback.
    ("theme_menu", "Thema", "Theme"),
    ("help_menu", "Help", "Help"),
    ("encode_base64", "Codeer als Base64", "Encode as Base64"),
    ("decode_base64", "Decodeer Base64", "Decode Base64"),
    (
        "encode_base64_url",
        "Codeer als Base64 (URL-veilig)",
        "Encode as Base64 (URL-Safe)",
    ),
    (
        "decode_base64_url",
        "Decodeer Base64 (URL-veilig)",
        "Decode Base64 (URL-Safe)",
    ),
    ("encode_url", "Codeer voor URL", "Encode for URL"),
    ("decode_url", "Decodeer URL", "Decode URL"),
    ("encode_hex", "Codeer als hex", "Encode as Hex"),
    ("decode_hex", "Decodeer hex", "Decode Hex"),
    (
        "detail_encoding",
        "Hele tekst coderen of decoderen als nieuwe kopie; ⌘Z toont de vorige",
        "Encode or decode the whole text as a new copy; ⌘Z shows the previous one",
    ),
    ("encoding_url_name", "URL-codering", "URL encoding"),
    ("encoding_hex_name", "hex", "hex"),
    (
        "encoding_invalid",
        "Dit is geen geldige {0}",
        "This isn't valid {0}",
    ),
    (
        "encoding_not_text",
        "Gedecodeerd is dit geen tekst",
        "The decoded bytes aren't text",
    ),
    // Text ▾ › Convert › Encoding › hashes (src/text_ops/hash.rs)
    ("hash_sha256", "Bereken SHA-256", "Calculate SHA-256"),
    (
        "hash_sha1",
        "Bereken SHA-1 (niet voor beveiliging)",
        "Calculate SHA-1 (Not for Security)",
    ),
    (
        "hash_md5",
        "Bereken MD5 (niet voor beveiliging)",
        "Calculate MD5 (Not for Security)",
    ),
    (
        "detail_hash",
        "Hash van de hele tekst als hex in kleine letters, als nieuwe kopie",
        "Hash of the whole text as lowercase hex, as a new copy",
    ),
    (
        "detail_hash_weak",
        "Hash als hex in kleine letters; niet voor beveiliging (MD5 en SHA-1 zijn gebroken)",
        "Hash as lowercase hex; not for security (MD5 and SHA-1 are broken)",
    ),
    (
        "hash_not_kept",
        "Hash gekopieerd, niet in de geschiedenis (de tekst is gevoelig)",
        "Hash copied, not in history (the text is sensitive)",
    ),
    ("text_section_analyse", "Analyse", "Analysis"),
    // Text ▾ › Clean URL… (text_ops::url_clean)
    ("text_clean_url", "Schoon URL op…", "Clean URL…"),
    ("url_clean_title", "URL opschonen", "Clean URL"),
    // Text ▾ › QR Code and the QR view's Copy (qr_code, macos_qr)
    ("text_qr", "QR-code", "QR Code"),
    (
        "detail_text_qr",
        "De tekst als QR-code, als nieuwe kopie (op het apparaat)",
        "The text as a QR code, as a new copy (on the device)",
    ),
    ("title_qr_made", "QR-code", "QR code"),
    ("title_qr_read", "Tekst uit QR-code", "Text from QR code"),
    (
        "url_clean_message",
        "Haalt trackingparameters (utm_…, fbclid, gclid, si, …) uit elke link. De rest van de link blijft zoals hij is; er wordt niets geopend of opgehaald. ⌘Z zet de tekst terug.",
        "Removes tracking parameters (utm_…, fbclid, gclid, si, …) from every link. The rest of each link stays as it is; nothing is opened or fetched. ⌘Z puts the text back.",
    ),
    (
        "detail_url_clean",
        "Trackingparameters uit links halen",
        "Remove tracking parameters from links",
    ),
    (
        "url_clean_removed",
        "Te verwijderen parameters: {0}",
        "Parameters to remove: {0}",
    ),
    (
        "url_clean_none",
        "Geen trackingparameters gevonden",
        "No tracking parameters found",
    ),
    ("text_apply", "Pas toe op klembord", "Apply to Clipboard"),
    // Text ▾ › Case style (text_ops::case_style)
    ("case_menu", "Hoofdletterstijl", "Case Style"),
    (
        "case_message",
        "Zet elke regel in de gekozen stijl. Woorden worden gesplitst bij _, -, spaties, hoofdletters (HTTPServer → http_server) en na cijfers; lege regels blijven staan. ⌘Z zet de tekst terug.",
        "Writes every line in the chosen style. Words split at _, -, spaces, capitals (HTTPServer → http_server) and after digits; empty lines stay. ⌘Z puts the text back.",
    ),
    (
        "detail_case",
        "Elke regel in een andere hoofdletterstijl",
        "Every line in another case style",
    ),
    ("case_style_label", "Stijl", "Style"),
    (
        "case_changed",
        "Regels die veranderen: {0}",
        "Lines that change: {0}",
    ),
    ("case_camel", "camelCase", "camelCase"),
    ("case_pascal", "PascalCase", "PascalCase"),
    ("case_snake", "snake_case", "snake_case"),
    ("case_kebab", "kebab-case", "kebab-case"),
    ("case_constant", "CONSTANT_CASE", "CONSTANT_CASE"),
    ("case_upper", "HOOFDLETTERS", "UPPERCASE"),
    ("case_lower", "kleine letters", "lowercase"),
    // Text ▾ › Format JSON… (text_ops::json_style)
    ("text_json", "Maak JSON op…", "Format JSON…"),
    ("json_style_title", "JSON opmaken", "Format JSON"),
    (
        "json_style_message",
        "Opmaken zet de JSON met 2 spaties inspringing neer, Comprimeren op één regel. Waarden blijven gelijk; ⌘Z zet de tekst terug.",
        "Format lays the JSON out with a 2-space indent, Compress puts it on one line. Values stay the same; ⌘Z puts the text back.",
    ),
    (
        "detail_json_style",
        "JSON opmaken of comprimeren, sleutels sorteren",
        "Format or compress JSON, sort keys",
    ),
    ("json_style_layout", "Vorm", "Layout"),
    ("json_format", "Opmaken", "Format"),
    ("json_compress", "Comprimeren", "Compress"),
    ("json_sort_keys", "Sleutels sorteren", "Sort keys"),
    (
        "json_style_same",
        "De JSON staat al zo; er verandert niets.",
        "The JSON is already laid out this way; nothing changes.",
    ),
    (
        "json_duplicate_keys",
        "Een sleutel staat twee keer in hetzelfde object; opmaken zou er één weggooien.",
        "A key appears twice in one object; formatting would drop one of them.",
    ),
    (
        "json_numbers_lossy",
        "Een getal is te groot of te precies om ongewijzigd terug te schrijven.",
        "A number is too large or too precise to be written back unchanged.",
    ),
    (
        "pager_status",
        "--Meer-- pagina {0} van {1} ({2}%)",
        "--More-- page {0} of {1} ({2}%)",
    ),
    (
        "page_long_text",
        "Toon lange tekst in pagina's",
        "Show long text in pages",
    ),
    (
        "page_long_text_note",
        "Zoals more: spatie, b, g en G bladeren; Esc of q stopt.",
        "Like more: Space, b, g and G page; Esc or q stops.",
    ),
    ("card_building", "Bezig…", "Working…"),
    ("copied_toast", "Gekopieerd", "Copied"),
    ("saved_toast", "Bewaard", "Saved"),
    ("save_failed_toast", "Bewaren mislukt", "Save failed"),
    ("hide_content", "Verberg", "Hide"),
    (
        "hide_line_numbers",
        "Verberg regelnummers",
        "Hide Line Numbers",
    ),
    (
        "show_line_numbers",
        "Toon regelnummers",
        "Show Line Numbers",
    ),
    ("show_content", "Toon", "Show"),
    ("clear_confirm_title", "Alles wissen?", "Clear All?"),
    (
        "clear_confirm_message",
        "De geschiedenis, vastgezette items en het klembord worden uit het geheugen gewist.",
        "History, pins and the clipboard are wiped from memory.",
    ),
    ("clear_confirm_action", "Alles wissen", "Clear All"),
    ("clear_dont_ask", "Niet meer vragen", "Don't ask again"),
    ("confirm_clear", "Vragen voor wissen", "Ask before clearing"),
    (
        "confirm_clear_note",
        "Alles wissen op de kaart vraagt eerst om bevestiging.",
        "Clear All on the card asks for confirmation first.",
    ),
    (
        "announce_confirmations",
        "Bevestigingen voorlezen",
        "Announce confirmations",
    ),
    (
        "announce_confirmations_note",
        "VoiceOver zegt bijvoorbeeld “Gekopieerd”; de melding zelf blijft.",
        "VoiceOver says, for example, “Copied”; the toast itself stays.",
    ),
    (
        "text_not_kept",
        "Gekopieerd, niet in de geschiedenis (opnemen gepauzeerd)",
        "Copied, not in history (recording paused)",
    ),
    (
        "text_not_kept_secret",
        "Gekopieerd, niet in de geschiedenis (bevat een token of sleutel)",
        "Copied, not in history (holds a token or key)",
    ),
    (
        "text_not_kept_blank",
        "Gekopieerd, niet in de geschiedenis (leeg)",
        "Copied, not in history (empty)",
    ),
    (
        "image_not_kept_large",
        "Gekopieerd, niet in de geschiedenis (te groot)",
        "Copied, not in history (too large)",
    ),
    (
        "work_failed",
        "Kon het werk niet starten. Probeer het opnieuw.",
        "Could not start the work. Try again.",
    ),
    ("sep_tab", "Tab", "Tab"),
    ("sep_comma", "Komma", "Comma"),
    ("sep_semicolon", "Puntkomma", "Semicolon"),
    ("sep_pipe", "Verticale streep (|)", "Pipe (|)"),
    ("sep_whitespace", "Spaties", "Whitespace"),
];
