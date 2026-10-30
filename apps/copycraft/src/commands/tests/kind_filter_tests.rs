use super::*;
use crate::commands::{
    KindFilter, history_kinds, kind_chips, kind_query, search_shown, split_kind,
};
use crate::format::FormatKind;

fn copy(index: usize, kind: FormatKind, text: &str, pinned: bool) -> Hist {
    let mut item = history_item(index, "fake", kind != FormatKind::Image);
    item.kind = kind;
    item.search = Zeroizing::new(text.to_string());
    item.pinned = pinned;
    item
}

/// A history of fake copies, one of each kind.
fn history() -> LaunchData {
    let mut input = data(SubjectKind::Text, Some("hallo"));
    input.history = vec![
        copy(0, FormatKind::Url, "https://example.com/docs", false),
        copy(1, FormatKind::Json, "{\"naam\": \"Voorbeeld\"}", false),
        copy(2, FormatKind::Csv, "a,b\n1,2", false),
        copy(3, FormatKind::Image, "", false),
        copy(4, FormatKind::Plain, "#336699", false),
        copy(5, FormatKind::Plain, "boodschappen voor morgen", true),
    ];
    input
}

#[test]
fn a_typed_kind_works_in_dutch_and_english() {
    assert_eq!(
        split_kind("soort:link docs"),
        (Some(KindFilter::Link), "docs")
    );
    assert_eq!(
        split_kind("type:link docs"),
        (Some(KindFilter::Link), "docs")
    );
    assert_eq!(split_kind("  Type:IMAGE"), (Some(KindFilter::Image), ""));
    assert_eq!(
        split_kind("soort:afbeelding"),
        (Some(KindFilter::Image), "")
    );
    assert_eq!(split_kind("type:colour x"), (Some(KindFilter::Colour), "x"));
    assert_eq!(split_kind("soort:kleur"), (Some(KindFilter::Colour), ""));
    assert_eq!(split_kind("type:json"), (Some(KindFilter::Code), ""));
    assert_eq!(split_kind("soort:tabel"), (Some(KindFilter::Table), ""));
    assert_eq!(split_kind("type:pinned"), (Some(KindFilter::Pinned), ""));
    assert_eq!(
        split_kind("soort:vastgepind"),
        (Some(KindFilter::Pinned), "")
    );
    assert_eq!(split_kind("soort:tekst"), (Some(KindFilter::Text), ""));
    // Not a kind: all of it is search text.
    assert_eq!(split_kind("soort:onbekend"), (None, "soort:onbekend"));
    assert_eq!(split_kind("link"), (None, "link"));
    assert_eq!(split_kind("mijn type:link"), (None, "mijn type:link"));
    // A chip writes the kind back, in the language shown (English in tests).
    let written = kind_query(Some(KindFilter::Link), "docs");
    assert_eq!(written, "type:link docs");
    assert_eq!(split_kind(&written), (Some(KindFilter::Link), "docs"));
    assert_eq!(kind_query(None, "docs"), "docs");
}

#[test]
fn each_copy_has_the_kinds_history_detected() {
    let input = history();
    let has = |index: usize, kind| history_kinds(&input.history[index]).contains(kind);
    assert!(has(0, KindFilter::Link) && !has(0, KindFilter::Text));
    assert!(has(1, KindFilter::Code));
    assert!(has(2, KindFilter::Table));
    assert!(has(3, KindFilter::Image));
    assert!(has(4, KindFilter::Colour) && has(4, KindFilter::Text));
    assert!(has(5, KindFilter::Pinned) && has(5, KindFilter::Text));
    assert!(!has(5, KindFilter::Colour));
}

#[test]
fn a_kind_shows_only_its_copies_and_combines_with_the_text() {
    let input = history();
    let pool = search_pool(&input);
    let chips = kind_chips(&input);
    // Empty: the chips, Alles first, every kind held so none dimmed.
    let shown = search_shown(&pool, &chips, "");
    assert_eq!(shown[0].id, CommandId::SearchKind(None));
    assert_eq!(shown.len(), 8);
    assert!(shown.iter().all(|chip| chip.enabled));
    // A kind alone: Alles and every copy of it, no actions.
    let links = ids(&search_shown(&pool, &chips, "type:link"));
    assert_eq!(
        links,
        vec![CommandId::SearchKind(None), CommandId::History(0)]
    );
    let pinned = ids(&search_shown(&pool, &chips, "soort:vastgepind"));
    assert_eq!(
        pinned,
        vec![CommandId::SearchKind(None), CommandId::History(5)]
    );
    // With text: both must match.
    let text = ids(&search_shown(&pool, &chips, "type:text morgen"));
    assert_eq!(
        text,
        vec![CommandId::SearchKind(None), CommandId::History(5)]
    );
    assert_eq!(
        ids(&search_shown(&pool, &chips, "type:code morgen")),
        vec![CommandId::SearchKind(None)]
    );
    // Without a kind the search is as before.
    assert_eq!(
        ids(&search_shown(&pool, &chips, "morgen")),
        ids(&crate::commands::matching(&pool, "morgen"))
    );
}

#[test]
fn kinds_not_in_history_are_dimmed_and_no_history_means_no_chips() {
    let mut input = data(SubjectKind::Text, Some("hallo"));
    assert!(kind_chips(&input).is_empty());
    input.history = vec![copy(0, FormatKind::Url, "https://example.com", false)];
    let chips = kind_chips(&input);
    let enabled: Vec<CommandId> = chips
        .iter()
        .filter(|chip| chip.enabled)
        .map(|chip| chip.id.clone())
        .collect();
    assert_eq!(
        enabled,
        vec![
            CommandId::SearchKind(None),
            CommandId::SearchKind(Some(KindFilter::Link))
        ]
    );
    let titles = titles(&chips);
    assert_eq!(
        titles,
        [
            "All", "Text", "Link", "Code", "Table", "Image", "Colour", "Pinned"
        ]
    );
}

/// The kind is remembered in memory only: Esc and Wipe forget it, nothing stores it.
#[test]
fn the_last_kind_lives_for_the_session_only() {
    let events = include_str!("../../macos_launcher/events.rs");
    let close = &events[events.find("pub(crate) fn close_search()").unwrap()..];
    assert!(close[..close.find("\n}\n").unwrap()].contains("LAST_KIND.set(None)"));
    let wipe = &events[events.find("pub fn wipe_shown()").unwrap()..];
    assert!(wipe[..wipe.find("\n}\n").unwrap()].contains("LAST_KIND.set(None)"));
    for source in [
        include_str!("../../settings.rs"),
        include_str!("../../macos_launcher/api.rs"),
    ] {
        assert!(!source.contains("LAST_KIND") && !source.contains("KindFilter"));
    }
    let window = include_str!("../../macos_launcher/window.rs");
    assert!(window.contains("commands::search_shown(&pool.borrow(), &chips.borrow()"));
}
