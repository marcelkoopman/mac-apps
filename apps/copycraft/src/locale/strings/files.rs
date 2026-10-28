//! Opened and dropped files, the hidden main menu and ⋯ › New. Part of the catalog in `strings.rs`, whose glossary holds here too.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    // Opened and dropped files
    ("dropped_text", "Neergezette tekst", "Dropped text"),
    ("dropped_image", "Neergezette afbeelding", "Dropped image"),
    ("pasted_text", "Geplakte tekst", "Pasted text"),
    // The hidden main menu (src/macos_main_menu.rs): its key equivalents, as in macOS.
    ("menu_edit", "Wijzig", "Edit"),
    ("menu_undo", "Herstel", "Undo"),
    ("menu_redo", "Opnieuw", "Redo"),
    ("menu_cut", "Knip", "Cut"),
    ("menu_paste", "Plak", "Paste"),
    ("menu_select_all", "Selecteer alles", "Select All"),
    ("menu_window", "Venster", "Window"),
    ("menu_close", "Sluit venster", "Close Window"),
    ("menu_minimize", "Minimaliseer", "Minimize"),
    // ⋯ › New (src/generate.rs)
    ("new_menu", "Nieuw", "New"),
    ("generate_uuid_v4", "UUID v4", "UUID v4"),
    ("generate_uuid_v7", "UUID v7", "UUID v7"),
    (
        "generate_unix_seconds",
        "Unix-tijd (seconden)",
        "Unix Time (Seconds)",
    ),
    (
        "generate_unix_millis",
        "Unix-tijd (milliseconden)",
        "Unix Time (Milliseconds)",
    ),
    ("generate_iso_now", "ISO 8601 nu", "ISO 8601 Now"),
    // ⋯ › New › the colour sampler (src/macos_color_pick.rs); the card's title after the pick.
    (
        "generate_color_menu",
        "Kies kleur van scherm…",
        "Pick Color from Screen…",
    ),
    ("generate_color", "Kleur van scherm", "Color from Screen"),
    (
        "detail_generate_color",
        "Kies een kleur met de loep, als HEX op het klembord",
        "Pick a color with the loupe, as HEX on the clipboard",
    ),
    (
        "detail_generate",
        "Nieuwe kaart, ook op het klembord",
        "New card, also on the clipboard",
    ),
    ("pasted_image", "Geplakte afbeelding", "Pasted image"),
    (
        "file_not_text",
        "Dit bestand is geen tekst",
        "This file is not text",
    ),
    (
        "file_too_large",
        "Bestand is groter dan 8 MB",
        "File is larger than 8 MB",
    ),
    (
        "file_unreadable",
        "Kan dit bestand niet lezen",
        "Can't read this file",
    ),
    (
        "text_too_large",
        "Tekst is groter dan 8 MB",
        "Text is larger than 8 MB",
    ),
    ("pdf_one_page", "1 pagina", "1 page"),
    ("pdf_n_pages", "{0} pagina's", "{0} pages"),
    ("pdf_page_of", "pagina {0}/{1}", "page {0}/{1}"),
    ("pdf_zoom", "zoom {0}%", "zoom {0}%"),
    (
        "pdf_page_of_cut",
        "pagina {0}/{1} (van {2})",
        "page {0}/{1} (of {2})",
    ),
    (
        "pdf_first_pages",
        "Eerste {0} van {1} pagina's",
        "First {0} of {1} pages",
    ),
    (
        "pdf_locked",
        "Deze PDF is met een wachtwoord beveiligd",
        "This PDF is protected with a password",
    ),
    (
        "pdf_no_text",
        "Deze PDF bevat geen tekst",
        "This PDF has no text",
    ),
    (
        "pdf_too_large",
        "PDF is groter dan 32 MB",
        "PDF is larger than 32 MB",
    ),
    ("document_first_page", "Eerste pagina", "First page"),
    (
        "docx_too_large",
        "Word-bestand is groter dan 32 MB",
        "Word document is larger than 32 MB",
    ),
    (
        "docx_locked",
        "Dit Word-bestand is met een wachtwoord beveiligd",
        "This Word document is protected with a password",
    ),
    (
        "docx_bomb",
        "Dit Word-bestand is uitgepakt te groot om te lezen",
        "This Word document is too large to read once unpacked",
    ),
    (
        "docx_no_text",
        "Dit Word-bestand bevat geen tekst",
        "This Word document has no text",
    ),
    (
        "ai_pdf_ocr_pending",
        "Wacht tot de tekst in de PDF is herkend",
        "Wait until the text in the PDF is recognized",
    ),
    (
        "ai_pdf_no_text",
        "In deze PDF is geen tekst herkend",
        "No text was recognized in this PDF",
    ),
    (
        "dropped_text_empty",
        "De neergezette tekst is leeg",
        "The dropped text is empty",
    ),
    (
        "image_too_large",
        "Afbeelding is groter dan 32 MB",
        "Image is larger than 32 MB",
    ),
    (
        "image_unreadable",
        "Kan deze afbeelding niet lezen",
        "Can't read this image",
    ),
];
