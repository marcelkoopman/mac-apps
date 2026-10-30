//! Search by kind: the chips *Alles*, *Tekst*, *Link*, … and a typed `soort:link` / `type:link`
//! in front of the search text. Only kinds history already knows of each copy.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindFilter {
    Text,
    Link,
    /// JSON, YAML, XML, HTML, SVG, TOML, JWT and source code.
    Code,
    /// CSV, TSV and tables.
    Table,
    Image,
    /// A text that is one colour (`#336699`, `rgb(…)`, [`crate::color::parse`]).
    Colour,
    Pinned,
}

/// The chips, in order (after *Alles*).
pub const KIND_FILTERS: [KindFilter; 7] = [
    KindFilter::Text,
    KindFilter::Link,
    KindFilter::Code,
    KindFilter::Table,
    KindFilter::Image,
    KindFilter::Colour,
    KindFilter::Pinned,
];

/// The kinds a history row has (none for an action).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindSet(u8);

impl KindSet {
    fn bit(kind: KindFilter) -> u8 {
        1 << kind as u8
    }

    pub fn with(self, kind: KindFilter) -> Self {
        Self(self.0 | Self::bit(kind))
    }

    pub fn contains(self, kind: KindFilter) -> bool {
        self.0 & Self::bit(kind) != 0
    }
}

/// What `item` is, from the type history detected when it was copied, its pin and whether
/// its text is a colour.
pub fn history_kinds(item: &Hist) -> KindSet {
    let base = match item.kind {
        FormatKind::Url => KindFilter::Link,
        FormatKind::Image => KindFilter::Image,
        FormatKind::Csv | FormatKind::Tsv | FormatKind::Dataframe => KindFilter::Table,
        FormatKind::Text | FormatKind::Plain | FormatKind::Markdown | FormatKind::Agent => {
            KindFilter::Text
        }
        _ => KindFilter::Code,
    };
    let mut kinds = KindSet::default().with(base);
    if item.text && crate::color::parse(item.search.as_str()).is_some() {
        kinds = kinds.with(KindFilter::Colour);
    }
    if item.pinned {
        kinds = kinds.with(KindFilter::Pinned);
    }
    kinds
}

fn token_key(kind: KindFilter) -> &'static str {
    match kind {
        KindFilter::Text => "kind_token_text",
        KindFilter::Link => "kind_token_link",
        KindFilter::Code => "kind_token_code",
        KindFilter::Table => "kind_token_table",
        KindFilter::Image => "kind_token_image",
        KindFilter::Colour => "kind_token_colour",
        KindFilter::Pinned => "kind_token_pinned",
    }
}

/// The word after `soort:` / `type:` for `kind`, in the language shown.
fn token(kind: KindFilter) -> &'static str {
    match kind {
        KindFilter::Text => crate::locale::t("kind_token_text"),
        KindFilter::Link => crate::locale::t("kind_token_link"),
        KindFilter::Code => crate::locale::t("kind_token_code"),
        KindFilter::Table => crate::locale::t("kind_token_table"),
        KindFilter::Image => crate::locale::t("kind_token_image"),
        KindFilter::Colour => crate::locale::t("kind_token_colour"),
        KindFilter::Pinned => crate::locale::t("kind_token_pinned"),
    }
}

/// Words that mean `kind` in either language (Dutch and English both work, whatever the
/// language shown), plus a few common ones.
fn kind_of_token(word: &str) -> Option<KindFilter> {
    use crate::locale::{Lang, t_in};
    const EXTRA: &[(&str, KindFilter)] = &[
        ("url", KindFilter::Link),
        ("json", KindFilter::Code),
        ("csv", KindFilter::Table),
        ("color", KindFilter::Colour),
        ("pin", KindFilter::Pinned),
    ];
    KIND_FILTERS
        .into_iter()
        .find(|kind| {
            [Lang::Nl, Lang::En]
                .into_iter()
                .any(|lang| word.eq_ignore_ascii_case(t_in(lang, token_key(*kind))))
        })
        .or_else(|| {
            EXTRA
                .iter()
                .find(|(extra, _)| word.eq_ignore_ascii_case(extra))
                .map(|(_, kind)| *kind)
        })
}

/// The kind a query starts with (`soort:link`, `type:image`) and the search text after it.
/// Without a known kind the whole query is search text.
pub fn split_kind(query: &str) -> (Option<KindFilter>, &str) {
    let start = query.trim_start();
    for prefix in ["soort:", "type:"] {
        let Some(head) = start.get(..prefix.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(prefix) {
            continue;
        }
        let rest = &start[prefix.len()..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if let Some(kind) = kind_of_token(&rest[..end]) {
            return (Some(kind), rest[end..].trim_start());
        }
    }
    (None, query)
}

/// The query for `kind` and the search text `rest`: `soort:link rest` (EN `type:link rest`);
/// `None` (*Alles*) leaves only `rest`.
pub fn kind_query(kind: Option<KindFilter>, rest: &str) -> String {
    match kind {
        Some(kind) => format!("{}{} {rest}", crate::locale::t("kind_prefix"), token(kind)),
        None => rest.to_string(),
    }
}

fn chip_title(kind: Option<KindFilter>) -> &'static str {
    match kind {
        None => crate::locale::t("kind_chip_all"),
        Some(KindFilter::Text) => crate::locale::t("kind_chip_text"),
        Some(KindFilter::Link) => crate::locale::t("kind_chip_link"),
        Some(KindFilter::Code) => crate::locale::t("kind_chip_code"),
        Some(KindFilter::Table) => crate::locale::t("kind_chip_table"),
        Some(KindFilter::Image) => crate::locale::t("kind_chip_image"),
        Some(KindFilter::Colour) => crate::locale::t("kind_chip_colour"),
        Some(KindFilter::Pinned) => crate::locale::t("kind_chip_pinned"),
    }
}

/// The chips of an open search: *Alles*, then each kind, dimmed when no copy in history is of
/// it. None without history: search then shows the card's chips as before.
pub fn kind_chips(data: &LaunchData) -> Vec<Command> {
    if data.history.is_empty() {
        return Vec::new();
    }
    let held: Vec<KindSet> = data.history.iter().map(history_kinds).collect();
    let mut chips = vec![command(
        CommandId::SearchKind(None),
        chip_title(None),
        crate::locale::t("kind_filter_all_detail"),
        "",
    )];
    for kind in KIND_FILTERS {
        chips.push(command_enabled(
            CommandId::SearchKind(Some(kind)),
            chip_title(Some(kind)),
            crate::locale::t("kind_filter_detail"),
            "",
            held.iter().any(|kinds| kinds.contains(kind)),
        ));
    }
    chips
}

/// What an open search shows for `query`: the kind chips while it is empty; with a kind
/// (`soort:link …`) *Alles* and the history rows of that kind that match the rest (all of
/// them while it is empty); else [`matching`]. At most [`MAX_VISIBLE`] rows.
pub fn search_shown(pool: &[Command], chips: &[Command], query: &str) -> Vec<Command> {
    let (kind, rest) = split_kind(query);
    let Some(kind) = kind else {
        if query.trim().is_empty() {
            return chips.to_vec();
        }
        return matching(pool, query);
    };
    let mut needle = rest.trim().to_lowercase();
    let mut shown: Vec<Command> = chips
        .iter()
        .filter(|chip| chip.id == CommandId::SearchKind(None))
        .cloned()
        .collect();
    shown.extend(
        pool.iter()
            .filter(|cmd| cmd.kinds.contains(kind))
            .filter(|cmd| needle.is_empty() || cmd.matches(&needle))
            .take(MAX_VISIBLE)
            .cloned(),
    );
    needle.zeroize();
    shown
}
