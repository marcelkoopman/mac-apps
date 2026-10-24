//! The NL/EN catalog: `(key, Dutch, English)`. Placeholders are `{0}`, `{1}`, … (see `locale::tf`).
//! Keep terminology consistent: a term has one word everywhere. Glossary (Dutch / English):
//!
//! - klembord / clipboard: never "pasteboard" in a visible string (`NSPasteboard` is code).
//! - geschiedenis / history: never "historie" or "historiestrook".
//! - afbeelding / image: never "beeld" on its own, "plaatje" or "foto", never "picture" or
//!   "photo". Compounds of their own stay: *beeldverhouding* (aspect ratio), *voorbeeld*.
//! - idee voorstellen / suggest a feature: never "feature" in Dutch.
//! - tabel / table, kolommen / columns, kaart / card, regels / lines.
//! - A command starts with its verb: *Meet complexiteit* / *Measure Complexity*, *Toon
//!   koppenoverzicht* / *Show Heading Outline*, *Bereken SHA-256* / *Calculate SHA-256*,
//!   *Wissel A en B* / *Swap A and B*; never a bare noun such as "Complexiteit".
//!
//! `locale::tests::visible_strings_follow_the_glossary` rejects the forbidden variants.

mod ai;
mod card;
mod commands;
mod files;
mod history;
mod image;
mod settings;
mod table;
mod text_menu;
mod welcome;

/// The catalog's parts, one per screen or section. A key is in one of them.
const PARTS: &[&[(&str, &str, &str)]] = &[
    welcome::STRINGS,
    settings::STRINGS,
    card::STRINGS,
    files::STRINGS,
    history::STRINGS,
    commands::STRINGS,
    table::STRINGS,
    image::STRINGS,
    text_menu::STRINGS,
    ai::STRINGS,
];

/// Every entry of the catalog, part after part.
pub(super) fn all() -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    PARTS.iter().flat_map(|part| part.iter())
}
