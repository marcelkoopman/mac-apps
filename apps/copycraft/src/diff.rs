//! Line diff of two history copies. The result stays on the card until Wipe, lock or a
//! cleared history. It is not recorded and not kept with a history entry.
//!
//! [`compare`] does not read the pasteboard and does not touch the catalog, so it can run
//! off the main thread. Copy and Save call [`Outcome::copy_text`] on the main thread.

use std::hash::{Hash, Hasher};

use zeroize::{Zeroize, Zeroizing};

use similar::{ChangeTag, TextDiff};

/// A side larger than this (decimal megabyte, as the card's size line) is not compared.
pub const MAX_BYTES: usize = 1_000_000;
/// A side with more lines than this is not compared.
pub const MAX_LINES: usize = 20_000;
/// Unchanged lines kept before and after a change. A longer run collapses to [`FOLD`].
const CONTEXT: usize = 3;
/// Shown where unchanged lines were left out. Not a `+` or `-` line, so it stays gray.
const FOLD: &str = "···";
/// Between the A and B columns of the side-by-side view. Not a character of either copy.
pub const COLUMN_SEP: char = '\u{E000}';
/// How wide a column is padded, in characters. A longer line keeps its extra characters.
const COLUMN_CAP: usize = 48;

/// At most two history entries, oldest pick first. Index 0 is A (left, older pick) and
/// index 1 is B (right, newer pick). Not stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    picks: Vec<u64>,
}

/// What [`Selection::add`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddResult {
    /// The entry was already chosen.
    Held,
    /// One entry is chosen. There is no diff yet.
    One,
    /// The second entry was chosen. Start the diff.
    Start,
    /// A third entry replaced the oldest pick. The previous B is now A.
    Replace,
}

impl Selection {
    /// Choose `id`. A third pick drops the oldest (index 0) and appends, so the remaining
    /// pick becomes A and the new one is B.
    pub fn add(&mut self, id: u64) -> AddResult {
        if self.picks.contains(&id) {
            return AddResult::Held;
        }
        if self.picks.len() == 2 {
            self.picks.remove(0);
            self.picks.push(id);
            return AddResult::Replace;
        }
        self.picks.push(id);
        if self.picks.len() == 2 {
            AddResult::Start
        } else {
            AddResult::One
        }
    }

    /// Drop `id`. The one that remains is A. `false` when it was not chosen.
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.picks.len();
        self.picks.retain(|pick| *pick != id);
        self.picks.len() != before
    }

    /// Turn A and B around. `false` unless two are chosen.
    pub fn swap(&mut self) -> bool {
        if self.picks.len() == 2 {
            self.picks.swap(0, 1);
            true
        } else {
            false
        }
    }

    /// Wipe, lock and a cleared history drop the picks. Nothing is written to disk.
    pub fn clear(&mut self) {
        self.picks.clear();
    }

    /// Drop picks `keep` rejects. `true` when one was dropped.
    pub fn retain<F>(&mut self, mut keep: F) -> bool
    where
        F: FnMut(u64) -> bool,
    {
        let before = self.picks.len();
        self.picks.retain(|id| keep(*id));
        self.picks.len() != before
    }

    pub fn contains(&self, id: u64) -> bool {
        self.picks.contains(&id)
    }

    /// A and B, when both are chosen.
    pub fn pair(&self) -> Option<(u64, u64)> {
        match self.picks.as_slice() {
            [left, right] => Some((*left, *right)),
            _ => None,
        }
    }

    pub fn ids(&self) -> &[u64] {
        &self.picks
    }

    /// How many entries are chosen. At most two.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.picks.len()
    }

    /// No entry is chosen.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }

    /// "A" or "B" when `id` is chosen.
    pub fn label(&self, id: u64) -> Option<&'static str> {
        side_label(&self.picks, id)
    }

    /// The diff action for history entry `id`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn item_menu(&self, id: u64, is_text: bool) -> ItemMenu {
        menu_for(&self.picks, id, is_text)
    }
}

/// "A" or "B" for a chosen id. Index 0 is A.
pub fn side_label(picks: &[u64], id: u64) -> Option<&'static str> {
    match picks.iter().position(|pick| *pick == id) {
        Some(0) => Some("A"),
        Some(1) => Some("B"),
        _ => None,
    }
}

/// Which history-menu row a diff action is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Add,
    Compare,
    Remove,
}

/// Title kind and whether the menu item is enabled. A non-text copy stays dimmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemMenu {
    pub kind: MenuKind,
    pub enabled: bool,
}

/// The diff action for history entry `id`. One other pick says Compare; a chosen entry
/// says Remove; anything else says Add. `enabled` is false when the entry is not text,
/// except Remove, which is how a chosen entry is dropped.
pub fn menu_for(picks: &[u64], id: u64, is_text: bool) -> ItemMenu {
    if picks.contains(&id) {
        ItemMenu {
            kind: MenuKind::Remove,
            enabled: true,
        }
    } else if picks.len() == 1 {
        ItemMenu {
            kind: MenuKind::Compare,
            enabled: is_text,
        }
    } else {
        ItemMenu {
            kind: MenuKind::Add,
            enabled: is_text,
        }
    }
}

/// What the diff thread produced. [`Debug`] hides the lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    TooLarge,
    /// No inserted or deleted line. The pane is both copies, side by side.
    Same(Pane),
    Lines(LineDiff),
}

impl Outcome {
    /// No inserted or deleted line.
    #[cfg(test)]
    pub fn is_same(&self) -> bool {
        matches!(self, Self::Same(_))
    }

    /// The side-by-side well (A, then [`COLUMN_SEP`], then B). Empty when there is none.
    #[cfg(test)]
    pub fn view(&self) -> &str {
        match self {
            Self::Same(pane) => pane.as_str(),
            Self::Lines(lines) => lines.view(),
            Self::TooLarge => "",
        }
    }

    /// Text for Copy and Save. Main thread only: this reads the catalog.
    pub fn copy_text(&self) -> Zeroizing<String> {
        Zeroizing::new(match self {
            Self::Lines(lines) => lines.body.clone(),
            Self::Same(_) => crate::locale::t("diff_none").to_string(),
            Self::TooLarge => crate::locale::t("diff_too_large").to_string(),
        })
    }
}

/// One side-by-side well. [`Debug`] hides the text. Dropping it wipes the text.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Pane(String);

impl Pane {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// The card's diff: still running, or a finished [`Outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Working,
    TooLarge,
    /// Both copies, side by side. Nothing was inserted or deleted.
    Same(Pane),
    Lines(LineDiff),
}

impl Phase {
    pub fn from_outcome(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::TooLarge => Self::TooLarge,
            Outcome::Same(pane) => Self::Same(pane.clone()),
            Outcome::Lines(lines) => Self::Lines(lines.clone()),
        }
    }

    /// Copy and Save wait until the diff has finished.
    pub fn copyable(&self) -> bool {
        !matches!(self, Self::Working)
    }

    /// Mixed into [`token`] so a finished result is a different card than the spinner.
    pub fn tag(&self) -> u8 {
        match self {
            Self::Working => 0,
            Self::TooLarge => 1,
            Self::Same(_) => 2,
            Self::Lines(_) => 3,
        }
    }
}

/// Collapsed line diff. The body is `+` / `-` / space lines. Dropping it wipes the text.
#[derive(Clone, PartialEq, Eq)]
pub struct LineDiff {
    /// Unified `+` / `-` lines for Copy and Save.
    body: String,
    /// A on the left of [`COLUMN_SEP`], B on the right. What the well shows.
    view: String,
    added: usize,
    removed: usize,
}

impl LineDiff {
    pub fn added(&self) -> usize {
        self.added
    }

    pub fn removed(&self) -> usize {
        self.removed
    }

    /// Unified `+` / `-` lines.
    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.body
    }

    /// A left, B right, one row per line, separated by [`COLUMN_SEP`].
    pub fn view(&self) -> &str {
        &self.view
    }
}

impl Drop for LineDiff {
    fn drop(&mut self) {
        self.body.zeroize();
        self.view.zeroize();
    }
}

impl std::fmt::Debug for LineDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineDiff")
            .field("bytes", &self.body.len())
            .field("added", &self.added)
            .field("removed", &self.removed)
            .finish()
    }
}

/// Identity of an ordered pair and a phase. Swap and a finished result both change it,
/// so reveal does not carry from one side, or from the spinner, onto the lines.
pub fn token(left: u64, right: u64, phase: u8) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    left.hash(&mut hasher);
    right.hash(&mut hasher);
    phase.hash(&mut hasher);
    hasher.finish()
}

/// Labels of both sides. Sensitive when either side already has a label. While a side is
/// still being checked and neither is known-sensitive, the result is checking.
pub fn combine_labels(
    left: &crate::sensitivity::Labeling,
    right: &crate::sensitivity::Labeling,
) -> (bool, crate::sensitivity::Labeling) {
    use crate::sensitivity::{Found, Labeling};
    let sensitive = !known_labels(left).is_empty() || !known_labels(right).is_empty();
    if sensitive {
        let mut labels = Vec::new();
        labels.extend_from_slice(known_labels(left));
        labels.extend_from_slice(known_labels(right));
        labels.sort();
        labels.dedup();
        return (true, Labeling::Known(Found { labels }));
    }
    if matches!(left, Labeling::Checking) || matches!(right, Labeling::Checking) {
        return (false, Labeling::Checking);
    }
    (false, Labeling::Known(Found::default()))
}

fn known_labels(labeling: &crate::sensitivity::Labeling) -> &[crate::sensitivity::Label] {
    match labeling {
        crate::sensitivity::Labeling::Known(found) => found.labels.as_slice(),
        crate::sensitivity::Labeling::Checking => &[],
    }
}

/// Line diff of `left` (A, old) and `right` (B, new). JSON objects and arrays on both
/// sides are pretty-printed with sorted keys first, so key order is not a difference.
/// When that leaves the two sides equal, the well still shows each copy as it was.
/// Anything else, including CSV, is compared as it is. A side over [`MAX_BYTES`] or
/// [`MAX_LINES`] is [`Outcome::TooLarge`].
pub fn compare(left: &str, right: &str) -> Outcome {
    if too_large(left) || too_large(right) {
        return Outcome::TooLarge;
    }
    if let (Some(canon_left), Some(canon_right)) = (canonical_json(left), canonical_json(right)) {
        if canon_left.as_str() == canon_right.as_str() {
            return Outcome::Same(pane_of(&raw_rows(left, right)));
        }
        return line_diff(canon_left.as_str(), canon_right.as_str());
    }
    line_diff(left, right)
}

fn too_large(text: &str) -> bool {
    text.len() > MAX_BYTES || text.lines().count() > MAX_LINES
}

/// Pretty JSON with sorted keys, when `text` is a JSON object or array. `None` otherwise,
/// including JSON that does not parse: the caller then compares the raw text.
fn canonical_json(text: &str) -> Option<Zeroizing<String>> {
    if crate::format::detect(text) != crate::format::FormatKind::Json {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if !value.is_object() && !value.is_array() {
        return None;
    }
    let pretty = serde_json::to_string_pretty(&sort_json(value)).ok()?;
    Some(Zeroizing::new(pretty))
}

fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json).collect())
        }
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = serde_json::Map::new();
            for (key, child) in pairs {
                let _ = sorted.insert(key, sort_json(child));
            }
            serde_json::Value::Object(sorted)
        }
        other => other,
    }
}

fn line_diff(left: &str, right: &str) -> Outcome {
    if left == right {
        return Outcome::Same(pane_of(&raw_rows(left, right)));
    }
    let diff = TextDiff::from_lines(left, right);
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut changes = Vec::new();
    // One op can hold several lines. Count and show each line on its own.
    for change in diff.iter_all_changes() {
        for line in split_lines(change.value()) {
            match change.tag() {
                ChangeTag::Insert => added += 1,
                ChangeTag::Delete => removed += 1,
                ChangeTag::Equal => {}
            }
            changes.push((change.tag(), line));
        }
    }
    if added == 0 && removed == 0 {
        return Outcome::Same(pane_of(&raw_rows(left, right)));
    }
    Outcome::Lines(LineDiff {
        view: render(&paired(&changes)),
        body: collapse(&changes),
        added,
        removed,
    })
}

/// Lines of one diff op. A trailing newline is the line end, not an extra empty line.
/// A missing final newline stays part of the last line, so it is a real change.
fn split_lines(value: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = value;
    while let Some((line, tail)) = rest.split_once('\n') {
        lines.push(line.strip_suffix('\r').unwrap_or(line).to_string());
        rest = tail;
    }
    if !rest.is_empty() {
        lines.push(rest.strip_suffix('\r').unwrap_or(rest).to_string());
    }
    lines
}

fn collapse(changes: &[(ChangeTag, String)]) -> String {
    let mut out = String::new();
    let mut covered = 0usize;
    for (start, end) in context_ranges(changes) {
        if start > covered {
            push_line(&mut out, FOLD);
        }
        for (tag, line) in &changes[start..=end] {
            push_line(&mut out, &prefixed(*tag, line));
        }
        covered = end.saturating_add(1);
    }
    if covered < changes.len() {
        push_line(&mut out, FOLD);
    }
    out
}

/// Inclusive index spans that keep [`CONTEXT`] lines around each change. Touching spans merge.
fn context_ranges(changes: &[(ChangeTag, String)]) -> Vec<(usize, usize)> {
    let Some(last) = changes.len().checked_sub(1) else {
        return Vec::new();
    };
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (index, (tag, _)) in changes.iter().enumerate() {
        if *tag == ChangeTag::Equal {
            continue;
        }
        let start = index.saturating_sub(CONTEXT);
        let end = index.saturating_add(CONTEXT).min(last);
        if let Some(span) = ranges.last_mut()
            && start <= span.1.saturating_add(1)
        {
            span.1 = span.1.max(end);
        } else {
            ranges.push((start, end));
        }
    }
    ranges
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Same,
    Added,
    Removed,
    Empty,
}

struct Side {
    mark: Mark,
    text: String,
}

/// One row per line of the two copies. The shorter side is empty on the extra rows.
fn raw_rows(left: &str, right: &str) -> Vec<(Side, Side)> {
    let left_lines = split_lines(left);
    let right_lines = split_lines(right);
    let count = left_lines.len().max(right_lines.len());
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        rows.push((
            plain_side(left_lines.get(index)),
            plain_side(right_lines.get(index)),
        ));
    }
    rows
}

fn plain_side(line: Option<&String>) -> Side {
    match line {
        Some(text) => Side {
            mark: Mark::Same,
            text: text.clone(),
        },
        None => Side {
            mark: Mark::Empty,
            text: String::new(),
        },
    }
}

/// A left and B right. A run of deletions beside a run of insertions shares rows,
/// so a changed line sits on one row. Unchanged stretches outside the context fold.
fn paired(changes: &[(ChangeTag, String)]) -> Vec<(Side, Side)> {
    let mut rows = Vec::new();
    let mut covered = 0usize;
    for (start, end) in context_ranges(changes) {
        if start > covered {
            rows.push(fold_row());
        }
        rows.extend(zip_run(&changes[start..=end]));
        covered = end.saturating_add(1);
    }
    if covered < changes.len() {
        rows.push(fold_row());
    }
    rows
}

fn fold_row() -> (Side, Side) {
    (
        Side {
            mark: Mark::Same,
            text: FOLD.to_string(),
        },
        Side {
            mark: Mark::Same,
            text: FOLD.to_string(),
        },
    )
}

fn zip_run(changes: &[(ChangeTag, String)]) -> Vec<(Side, Side)> {
    let mut rows = Vec::new();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for (tag, line) in changes {
        match tag {
            ChangeTag::Equal => {
                flush_edits(&mut rows, &mut removed, &mut added);
                rows.push((
                    Side {
                        mark: Mark::Same,
                        text: line.clone(),
                    },
                    Side {
                        mark: Mark::Same,
                        text: line.clone(),
                    },
                ));
            }
            ChangeTag::Delete => removed.push(line.clone()),
            ChangeTag::Insert => added.push(line.clone()),
        }
    }
    flush_edits(&mut rows, &mut removed, &mut added);
    rows
}

fn flush_edits(rows: &mut Vec<(Side, Side)>, removed: &mut Vec<String>, added: &mut Vec<String>) {
    let count = removed.len().max(added.len());
    for index in 0..count {
        let left = match removed.get(index) {
            Some(text) => Side {
                mark: Mark::Removed,
                text: text.clone(),
            },
            None => Side {
                mark: Mark::Empty,
                text: String::new(),
            },
        };
        let right = match added.get(index) {
            Some(text) => Side {
                mark: Mark::Added,
                text: text.clone(),
            },
            None => Side {
                mark: Mark::Empty,
                text: String::new(),
            },
        };
        rows.push((left, right));
    }
    removed.clear();
    added.clear();
}

fn pane_of(rows: &[(Side, Side)]) -> Pane {
    Pane(render(rows))
}

/// Header "A" / "B", then one row per line. Columns are padded so the separator lines up.
fn render(rows: &[(Side, Side)]) -> String {
    let width = rows
        .iter()
        .flat_map(|(left, right)| [left.text.chars().count(), right.text.chars().count()])
        .max()
        .unwrap_or(1)
        .clamp(1, COLUMN_CAP);
    let mut out = String::new();
    write_row(
        &mut out,
        &Side {
            mark: Mark::Same,
            text: "A".to_string(),
        },
        &Side {
            mark: Mark::Same,
            text: "B".to_string(),
        },
        width,
    );
    for (left, right) in rows {
        write_row(&mut out, left, right, width);
    }
    out
}

fn write_row(out: &mut String, left: &Side, right: &Side, width: usize) {
    write_side(out, left, width);
    out.push(COLUMN_SEP);
    write_side(out, right, width);
    out.push('\n');
}

fn write_side(out: &mut String, side: &Side, width: usize) {
    let prefix = match side.mark {
        Mark::Removed => '-',
        Mark::Added => '+',
        Mark::Same | Mark::Empty => ' ',
    };
    out.push(prefix);
    out.push_str(&side.text);
    let pad = width.saturating_sub(side.text.chars().count());
    for _ in 0..pad {
        out.push(' ');
    }
}

fn prefixed(tag: ChangeTag, line: &str) -> String {
    let prefix = match tag {
        ChangeTag::Delete => "-",
        ChangeTag::Insert => "+",
        ChangeTag::Equal => " ",
    };
    format!("{prefix}{line}")
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::{
        AddResult, COLUMN_SEP, MAX_BYTES, MAX_LINES, MenuKind, Outcome, Selection, combine_labels,
        compare, menu_for,
    };
    use crate::sensitivity::{Found, Label, Labeling};

    fn sample(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/diff")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
    }

    fn lines_of(outcome: Outcome) -> (usize, usize, String) {
        match outcome {
            Outcome::Lines(diff) => (diff.added(), diff.removed(), diff.text().to_string()),
            other => panic!("expected lines, got {other:?}"),
        }
    }

    #[test]
    fn equal_text_has_no_differences() {
        assert!(compare("same\n", "same\n").is_same());
        let both = compare("same\n", "same\n").view().to_string();
        assert!(
            both.contains("same") && both.matches("same").count() == 2,
            "{both}"
        );
        assert!(compare("", "").is_same());
    }

    #[test]
    fn addition_only() {
        let (added, removed, body) = lines_of(compare("alpha\n", "alpha\nbeta\n"));
        assert_eq!((added, removed), (1, 0));
        assert_eq!(body, " alpha\n+beta\n");
        assert_eq!(
            compare("alpha\n", "alpha\nbeta\n").copy_text().as_str(),
            " alpha\n+beta\n"
        );
    }

    #[test]
    fn deletion_only() {
        let (added, removed, body) = lines_of(compare("alpha\nbeta\n", "alpha\n"));
        assert_eq!((added, removed), (0, 1));
        assert_eq!(body, " alpha\n-beta\n");
    }

    #[test]
    fn a_changed_line_is_a_deletion_and_an_addition() {
        let (added, removed, body) = lines_of(compare("alpha\n", "beta\n"));
        assert_eq!((added, removed), (1, 1));
        assert_eq!(body, "-alpha\n+beta\n");
    }

    #[test]
    fn unchanged_runs_collapse_after_three_lines_of_context() {
        let left: String = (0..20).map(|i| format!("l{i}\n")).collect();
        let right = left.replacen("l10\n", "x\n", 1);
        let (added, removed, body) = lines_of(compare(&left, &right));
        assert_eq!((added, removed), (1, 1));
        assert!(body.starts_with("···\n"), "{body}");
        assert!(body.contains(" l7\n"), "{body}");
        assert!(!body.contains(" l6\n"), "{body}");
        assert!(body.contains("-l10\n"), "{body}");
        assert!(body.contains("+x\n"), "{body}");
        assert!(body.contains(" l13\n"), "{body}");
        assert!(!body.contains(" l14\n"), "{body}");
        assert!(body.ends_with("···\n"), "{body}");
    }

    #[test]
    fn json_key_order_is_not_a_difference() {
        let left = "{\n  \"b\": 1,\n  \"a\": {\"z\": 1, \"y\": 2}\n}\n";
        let right = "{\"a\":{\"y\":2,\"z\":1},\"b\":1}";
        assert!(compare(left, right).is_same());
        let both = compare(&sample("a.json"), &sample("b.json"));
        assert!(both.is_same());
        // The well keeps each copy as stored, so the key order can be read on one row.
        let view = both.view();
        let b_first = view.find("\"b\": 1,").expect("left");
        let a_first = view.find("\"a\": 2,").expect("right");
        assert!(
            b_first < a_first && !view[b_first..a_first].contains('\n'),
            "{view}"
        );
        assert!(view.contains(COLUMN_SEP), "{view}");
    }

    #[test]
    fn json_value_change_is_a_difference() {
        let (added, removed, body) = lines_of(compare(&sample("a.json"), &sample("c.json")));
        assert!(added >= 1 && removed >= 1, "{added} {removed} {body}");
        assert!(body.contains('+') && body.contains('-'), "{body}");
        let view = compare(&sample("a.json"), &sample("c.json"))
            .view()
            .to_string();
        let left = view.find("-  \"a\": 2").expect("left");
        let right = view.find("+  \"a\": 3").expect("right");
        assert!(left < right && !view[left..right].contains('\n'), "{view}");
    }

    #[test]
    fn csv_is_compared_as_text() {
        let left = "b,a\n2,1\n";
        let right = "a,b\n1,2\n";
        let (_, _, body) = lines_of(compare(left, right));
        assert!(body.contains("-b,a"), "{body}");
        assert!(body.contains("+a,b"), "{body}");
    }

    #[test]
    fn sample_texts_mark_the_changed_lines() {
        let (added, removed, body) = lines_of(compare(&sample("a.txt"), &sample("b.txt")));
        assert_eq!((added, removed), (2, 1));
        assert_eq!(body, " alpha\n-bravo\n+delta\n charlie\n+echo\n");
        let view = compare(&sample("a.txt"), &sample("b.txt"))
            .view()
            .to_string();
        assert!(view.contains("-bravo") && view.contains("+delta"), "{view}");
        assert!(view.contains("+echo"), "{view}");
        let bravo = view.find("-bravo").expect("left");
        let delta = view.find("+delta").expect("right");
        assert!(
            bravo < delta && !view[bravo..delta].contains('\n'),
            "{view}"
        );
    }

    #[test]
    fn over_the_size_limit_is_too_large() {
        let at_bytes = "a".repeat(MAX_BYTES);
        assert!(compare(&at_bytes, &at_bytes).is_same());
        let over_bytes = "a".repeat(MAX_BYTES + 1);
        assert_eq!(compare(&over_bytes, "a"), Outcome::TooLarge);
        let at_lines = "x\n".repeat(MAX_LINES);
        assert!(compare(&at_lines, &at_lines).is_same());
        let over_lines = "x\n".repeat(MAX_LINES + 1);
        assert_eq!(compare(&over_lines, &at_lines), Outcome::TooLarge);
        // The catalog follows the system language, so compare through the key.
        assert_eq!(
            Outcome::TooLarge.copy_text().as_str(),
            crate::locale::t("diff_too_large")
        );
        assert_eq!(
            Outcome::Same(super::Pane::default()).copy_text().as_str(),
            crate::locale::t("diff_none")
        );
    }

    #[test]
    fn two_picks_start_the_diff_and_a_third_replaces_the_oldest() {
        let mut selection = Selection::default();
        assert_eq!(selection.add(1), AddResult::One);
        assert!(selection.pair().is_none());
        assert_eq!(selection.add(1), AddResult::Held);
        assert_eq!(selection.add(2), AddResult::Start);
        assert_eq!(selection.pair(), Some((1, 2)));
        assert_eq!(selection.label(1), Some("A"));
        assert_eq!(selection.label(2), Some("B"));
        // The oldest pick (A) goes. The previous B becomes A and the new pick is B.
        assert_eq!(selection.add(3), AddResult::Replace);
        assert_eq!(selection.pair(), Some((2, 3)));
        assert_eq!(selection.ids(), &[2, 3]);
    }

    #[test]
    fn removing_a_pick_drops_the_pair() {
        let mut selection = Selection::default();
        selection.add(1);
        selection.add(2);
        assert!(selection.remove(1));
        assert!(!selection.remove(1));
        assert!(selection.pair().is_none());
        assert_eq!(selection.label(2), Some("A"));
        assert_eq!(selection.len(), 1);
    }

    #[test]
    fn swap_turns_the_sides_around() {
        let mut selection = Selection::default();
        assert!(!selection.swap());
        selection.add(1);
        selection.add(2);
        assert!(selection.swap());
        assert_eq!(selection.pair(), Some((2, 1)));
        assert_eq!(selection.label(2), Some("A"));
        assert_eq!(selection.label(1), Some("B"));
    }

    #[test]
    fn non_text_stays_dimmed() {
        assert_eq!(
            menu_for(&[], 4, false),
            super::ItemMenu {
                kind: MenuKind::Add,
                enabled: false,
            }
        );
        assert_eq!(
            menu_for(&[1], 4, false),
            super::ItemMenu {
                kind: MenuKind::Compare,
                enabled: false,
            }
        );
        assert_eq!(
            menu_for(&[1], 4, true),
            super::ItemMenu {
                kind: MenuKind::Compare,
                enabled: true,
            }
        );
        assert_eq!(
            menu_for(&[1, 2], 9, false),
            super::ItemMenu {
                kind: MenuKind::Add,
                enabled: false,
            }
        );
        assert_eq!(
            menu_for(&[1, 2], 1, true),
            super::ItemMenu {
                kind: MenuKind::Remove,
                enabled: true,
            }
        );
        let selection = {
            let mut selection = Selection::default();
            selection.add(1);
            selection
        };
        assert!(!selection.item_menu(2, false).enabled);
    }

    #[test]
    fn selection_is_empty_after_wipe_or_lock() {
        // Wipe, lock and a cleared history call [`Selection::clear`]. Restart starts empty.
        let mut selection = Selection::default();
        selection.add(1);
        selection.add(2);
        selection.clear();
        assert!(selection.is_empty());
        assert!(selection.pair().is_none());
    }

    #[test]
    fn a_dropped_history_entry_leaves_the_selection() {
        let mut selection = Selection::default();
        selection.add(1);
        selection.add(2);
        assert!(selection.retain(|id| id == 2));
        assert_eq!(selection.ids(), &[2]);
        assert!(!selection.retain(|id| id == 2));
    }

    #[test]
    fn labels_union_and_a_known_sensitive_side_wins_over_checking() {
        let known = |labels: &[Label]| {
            Labeling::Known(Found {
                labels: labels.to_vec(),
            })
        };
        assert_eq!(
            combine_labels(&known(&[]), &known(&[])),
            (false, known(&[]))
        );
        assert_eq!(
            combine_labels(&Labeling::Checking, &known(&[])),
            (false, Labeling::Checking)
        );
        let (sensitive, labeling) =
            combine_labels(&known(&[Label::Financial]), &Labeling::Checking);
        assert!(sensitive);
        assert_eq!(labeling, known(&[Label::Financial]));
        let (sensitive, labeling) = combine_labels(
            &known(&[Label::Financial]),
            &known(&[Label::Pii, Label::Financial]),
        );
        assert!(sensitive);
        assert_eq!(labeling, known(&[Label::Pii, Label::Financial]));
    }
}
