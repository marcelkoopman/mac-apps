//! History navigation and the card's status and counts. Part of the catalog in `strings.rs`, whose glossary holds here too.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    // History navigation
    ("strip_group", "Geschiedenis", "History"),
    (
        "strip_compare_hint",
        "⌘-klik om te vergelijken met de getoonde kopie",
        "⌘-click to compare with the copy on screen",
    ),
    ("strip_kind_text", "Tekst", "Text"),
    ("strip_age_now", "zojuist gekopieerd", "copied just now"),
    (
        "strip_age_minutes",
        "{0} min geleden gekopieerd",
        "copied {0} min ago",
    ),
    (
        "strip_age_hours",
        "{0} uur geleden gekopieerd",
        "copied {0} h ago",
    ),
    (
        "compare_text_only",
        "Vergelijken werkt alleen met tekst",
        "Compare only works with text",
    ),
    (
        "strip_action_compare",
        "Vergelijk met getoonde",
        "Compare with Shown",
    ),
    (
        "strip_compare_tip",
        "⌘-klik op een tegel om te vergelijken",
        "⌘-click a tile to compare",
    ),
    (
        "strip_compare_side",
        "vergelijking, kant {0}",
        "compared, side {0}",
    ),
    (
        "strip_hide",
        "Verberg geschiedenisstrook",
        "Hide History Strip",
    ),
    (
        "strip_show",
        "Toon geschiedenisstrook",
        "Show History Strip",
    ),
    // Card status, counts
    ("working", "Bezig…", "Working…"),
    (
        "working_on_table",
        "Bezig met de tabel…",
        "Working on the table…",
    ),
    ("table_empty", "De tabel is leeg", "The table is empty"),
    ("title_qr", "QR", "QR"),
    ("row", "rij", "row"),
    ("rows", "rijen", "rows"),
    ("column", "kolom", "column"),
    ("columns", "kolommen", "columns"),
    ("lines", "regels", "lines"),
    ("characters", "tekens", "characters"),
    (
        "rows_by_columns",
        "{0} rijen × {1} kolommen",
        "{0} rows × {1} columns",
    ),
    (
        "showing_note",
        "{0} van {1} {2} getoond",
        "Showing {0} of {1} {2}",
    ),
    ("image_size_line", "Afmetingen: {0}×{1}", "Size: {0}×{1}"),
    ("xsd_schema", "XSD-schema", "XSD schema"),
    ("avro_schema", "Avro-schema", "Avro schema"),
    ("sample_xml", "Voorbeeld-XML", "Sample XML"),
];
