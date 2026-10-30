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
    // A dropped or chosen zip
    ("chip_unzip", "Uitpakken", "Extract"),
    (
        "detail_unzip",
        "Kies een map; bestaat er al iets met dezelfde naam, dan komt de inhoud in een nieuwe map",
        "Choose a folder; if something with the same name is already there, the contents go into a new folder",
    ),
    ("zip_one_file", "1 bestand", "1 file"),
    ("zip_n_files", "{0} bestanden", "{0} files"),
    ("zip_one_folder", "1 map", "1 folder"),
    ("zip_n_folders", "{0} mappen", "{0} folders"),
    ("zip_unpacked", "{0} uitgepakt", "{0} unpacked"),
    ("zip_more", "… en nog {0}", "… and {0} more"),
    ("zip_meta_locked", "met wachtwoord", "password protected"),
    ("zip_meta_unsafe", "onveilige namen", "unsafe names"),
    (
        "zip_meta_work_folder_name",
        "naam van een werkmap",
        "name of a work folder",
    ),
    (
        "zip_meta_unsupported",
        "onbekende compressie",
        "unknown compression",
    ),
    ("zip_title", "Zip-archief", "Zip Archive"),
    (
        "zip_choose_folder",
        "Kies waar de bestanden komen",
        "Choose where the files go",
    ),
    (
        "unzipping",
        "Uitpakken… (⌘. stopt)",
        "Extracting… (⌘. to stop)",
    ),
    ("zip_extracted", "Uitgepakt in {0}", "Extracted to {0}"),
    (
        "zip_corrupt",
        "Dit zip-bestand is beschadigd",
        "This zip file is damaged",
    ),
    (
        "zip_locked",
        "Dit zip-bestand is met een wachtwoord beveiligd; uitpakken kan hier niet",
        "This zip file is protected with a password; it cannot be extracted here",
    ),
    (
        "zip_unsupported",
        "Dit zip-bestand gebruikt een compressie die hier niet uitgepakt kan worden",
        "This zip file uses a compression that cannot be extracted here",
    ),
    (
        "zip_unsafe",
        "Niet uitgepakt: een naam in dit zip-bestand wijst buiten de map, is een link of staat er twee keer in",
        "Not extracted: a name in this zip file points outside the folder, is a link, or is in it twice",
    ),
    (
        "zip_empty",
        "Dit zip-bestand is leeg",
        "This zip file is empty",
    ),
    (
        "zip_too_large",
        "Dit zip-bestand heeft te veel bestanden of is uitgepakt te groot",
        "This zip file has too many files or is too large once extracted",
    ),
    (
        "zip_write_failed",
        "Uitpakken mislukt; wat al was uitgepakt is weer weggehaald",
        "Extracting failed; what was already extracted has been removed",
    ),
    (
        "zip_no_space",
        "Niet uitgepakt: er is te weinig vrije ruimte op die schijf",
        "Not extracted: there is not enough free space on that disk",
    ),
    ("zip_incomplete", "{0} (onvolledig)", "{0} (incomplete)"),
    (
        "zip_work_folder_name",
        "Niet uitgepakt: een map in dit zip-bestand heeft de naam van de werkmap die CopyCraft bij het uitpakken maakt",
        "Not extracted: a folder in this zip file has the name of the work folder CopyCraft makes while extracting",
    ),
    (
        "zip_cancelled",
        "Uitpakken gestopt; er is niets achtergebleven",
        "Extracting stopped; nothing was left behind",
    ),
    // Checksums of a dropped or chosen file
    ("chip_checksums", "Controlegetallen", "Checksums"),
    (
        "detail_checksums",
        "SHA-256 en MD5 van dit bestand",
        "SHA-256 and MD5 of this file",
    ),
    (
        "chip_checksum_compare",
        "Controleer hash van klembord",
        "Check Hash from Clipboard",
    ),
    (
        "detail_checksum_compare",
        "Kopieer eerst de SHA-256 of MD5 van de downloadsite; dan zie je of dit bestand klopt",
        "Copy the SHA-256 or MD5 from the download site first to see whether this file is genuine",
    ),
    ("checksum_meta", "SHA-256 en MD5", "SHA-256 and MD5"),
    ("checksum_working", "Berekenen…", "Calculating…"),
    (
        "checksum_failed",
        "Kan dit bestand niet lezen",
        "Can't read this file",
    ),
    (
        "checksum_not_ready",
        "Controlegetallen worden nog berekend",
        "Checksums are still being calculated",
    ),
    (
        "checksum_match_sha256",
        "✓ Gelijk aan de SHA-256 op het klembord",
        "✓ Matches the SHA-256 on the clipboard",
    ),
    (
        "checksum_match_md5",
        "✓ Gelijk aan de MD5 op het klembord",
        "✓ Matches the MD5 on the clipboard",
    ),
    (
        "checksum_differs",
        "✗ Anders dan de hash op het klembord",
        "✗ Differs from the hash on the clipboard",
    ),
    (
        "checksum_no_hash",
        "Geen hash op het klembord",
        "No hash on the clipboard",
    ),
];
