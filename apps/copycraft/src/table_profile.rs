//! The profile of a table (*Profile* in the table window): per column its type, missing values
//! (null or empty) and unique values, and for number columns min, max, mean, median, the
//! quartiles and the outliers outside the IQR fences; per table the rows that repeat an
//! earlier row. Text that mostly (over 90 %) reads as dates or numbers is pointed out, and
//! columns with an (almost) unique value per row are marked as a possible ID.
//!
//! Works on the DataFrame of the table window, whatever the table came from (CSV, TSV, a JSON
//! table, a pasted grid). Eager polars only. Lives in memory only: never in history or on disk;
//! the names it holds are zeroized when it goes. The findings and the copy (Markdown) hold
//! column names and numbers, never a cell's value. Nothing here is logged.
//!
//! The next feature (AI explanation of outliers) reuses [`NumberStats`]: the fences and the
//! most deviating rows ([`NumberStats::extremes`]).

use std::collections::HashSet;
use std::sync::Arc;

use polars::prelude::{Column, DataFrame, DataType};
use zeroize::{Zeroize, Zeroizing};

use crate::locale::{Lang, t_in, tf_in};

/// How many findings the profile shows at the top.
pub const TOP_FINDINGS: usize = 3;
/// The most deviating rows kept per number column (for an explanation later).
pub const MAX_EXTREMES: usize = 20;
/// Text where more than this share of the values reads as dates or numbers is pointed out.
const LOOKS_LIKE_SHARE: f64 = 0.9;
/// A column with more missing values than this share is a finding.
const MISSING_SHARE: f64 = 0.05;
/// A possible ID: at least this share of the rows has a value, and as many unique values.
const ID_SHARE: f64 = 0.95;
/// Fewer rows than this say little: no ID mark and no one-value finding.
const MIN_ROWS: usize = 5;

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Integer,
    Decimal,
    Date,
    DateTime,
    Boolean,
    Other,
}

impl ColumnKind {
    fn of(dtype: &DataType) -> ColumnKind {
        if dtype.is_string() {
            ColumnKind::Text
        } else if dtype.is_integer() {
            ColumnKind::Integer
        } else if dtype.is_float() {
            ColumnKind::Decimal
        } else if dtype.is_date() {
            ColumnKind::Date
        } else if dtype.is_datetime() {
            ColumnKind::DateTime
        } else if dtype.is_bool() {
            ColumnKind::Boolean
        } else {
            ColumnKind::Other
        }
    }

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            ColumnKind::Text => t_in(lang, "profile_kind_text"),
            ColumnKind::Integer => t_in(lang, "profile_kind_integer"),
            ColumnKind::Decimal => t_in(lang, "profile_kind_decimal"),
            ColumnKind::Date => t_in(lang, "profile_kind_date"),
            ColumnKind::DateTime => t_in(lang, "profile_kind_datetime"),
            ColumnKind::Boolean => t_in(lang, "profile_kind_boolean"),
            ColumnKind::Other => t_in(lang, "profile_kind_other"),
        }
    }

    fn is_number(self) -> bool {
        matches!(self, ColumnKind::Integer | ColumnKind::Decimal)
    }
}

/// What a text column mostly reads as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooksLike {
    Dates,
    Numbers,
}

/// A row outside the IQR fences: its row in the table (0-based), its value and on which side.
/// No `Debug`: the value is a cell's.
#[derive(Clone, Copy, PartialEq)]
pub struct Extreme {
    pub row: usize,
    pub value: f64,
    /// Above the upper fence (else below the lower one).
    pub high: bool,
}

/// The numbers of a number column (its values, empty ones aside). No `Debug` (cell values).
#[derive(Clone, PartialEq)]
pub struct NumberStats {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub q1: f64,
    pub q3: f64,
    /// Values below [`Self::low_fence`] or above [`Self::high_fence`].
    pub outliers: usize,
    /// The outliers furthest from their fence first, at most [`MAX_EXTREMES`].
    pub extremes: Vec<Extreme>,
}

impl NumberStats {
    pub fn iqr(&self) -> f64 {
        self.q3 - self.q1
    }

    /// Q1 − 1.5 × IQR.
    pub fn low_fence(&self) -> f64 {
        self.q1 - 1.5 * self.iqr()
    }

    /// Q3 + 1.5 × IQR.
    pub fn high_fence(&self) -> f64 {
        self.q3 + 1.5 * self.iqr()
    }
}

/// One column of the profile. No `Debug` (its name and numbers).
#[derive(Clone, PartialEq)]
pub struct ColumnProfile {
    /// The column's place in the table.
    pub index: usize,
    pub name: String,
    pub kind: ColumnKind,
    /// Null or empty (only spaces).
    pub missing: usize,
    /// Distinct values, missing ones not counted.
    pub unique: usize,
    pub numbers: Option<NumberStats>,
    pub looks_like: Option<LooksLike>,
    pub possible_id: bool,
}

/// Something that stands out, by column index (no names, no values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    /// Text that mostly reads as dates or numbers but was not read as such.
    NotRecognised {
        column: usize,
        looks: LooksLike,
    },
    Missing {
        column: usize,
        count: usize,
    },
    Outliers {
        column: usize,
        count: usize,
    },
    DuplicateRows {
        count: usize,
    },
    OneValue {
        column: usize,
    },
}

/// The profile of one table version. No `Debug`: nothing of it may reach a log.
#[derive(Clone, PartialEq)]
pub struct TableProfile {
    pub rows: usize,
    pub columns: Vec<ColumnProfile>,
    /// Rows equal to an earlier row in every column; `None` when they could not be compared.
    pub duplicate_rows: Option<usize>,
    /// Every finding, the most serious first ([`Finding::score`]).
    pub findings: Vec<Finding>,
}

/// What the profile thread sends back for request `generation` of the table window: the
/// profile, or `None` when the window had no table. Shared, not copied: the app keeps it for
/// *Copy Profile*, the window shows it; the last one to let go zeroizes it.
#[derive(Clone)]
pub struct ProfileOutcome {
    pub generation: u64,
    pub profile: Option<Arc<TableProfile>>,
}

/// Without the profile: its names and numbers stay out of any log.
impl std::fmt::Debug for ProfileOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileOutcome")
            .field("generation", &self.generation)
            .field("profile", &self.profile.is_some())
            .finish()
    }
}

impl ProfileOutcome {
    /// Work out the profile of `df` (on a thread of its own: a big table takes a moment).
    pub fn build(df: &DataFrame, generation: u64) -> Self {
        Self {
            generation,
            profile: Some(Arc::new(profile(df))),
        }
    }
}

impl Drop for TableProfile {
    fn drop(&mut self) {
        for column in &mut self.columns {
            column.name.zeroize();
            if let Some(numbers) = column.numbers.as_mut() {
                for extreme in &mut numbers.extremes {
                    extreme.value.zeroize();
                    extreme.row.zeroize();
                }
                for value in [
                    &mut numbers.min,
                    &mut numbers.max,
                    &mut numbers.mean,
                    &mut numbers.median,
                    &mut numbers.q1,
                    &mut numbers.q3,
                ] {
                    value.zeroize();
                }
            }
        }
    }
}

/// The profile of `df`.
pub fn profile(df: &DataFrame) -> TableProfile {
    let rows = df.height();
    let columns: Vec<ColumnProfile> = df
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| column_profile(index, column, rows))
        .collect();
    let duplicate_rows = duplicate_rows(df);
    let mut profile = TableProfile {
        rows,
        columns,
        duplicate_rows,
        findings: Vec::new(),
    };
    profile.findings = findings(&profile);
    profile
}

fn column_profile(index: usize, column: &Column, rows: usize) -> ColumnProfile {
    let kind = ColumnKind::of(column.dtype());
    let (missing, unique, looks_like) = match column.str() {
        Ok(text) => {
            let values: Vec<&str> = text
                .iter()
                .flatten()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect();
            let unique = values.iter().collect::<HashSet<_>>().len();
            (rows - values.len(), unique, looks_like(&values))
        }
        Err(_) => {
            let present = column.drop_nulls();
            let unique = present.n_unique().unwrap_or(0);
            (column.null_count(), unique, None)
        }
    };
    let present = rows - missing;
    let possible_id = rows >= MIN_ROWS
        && matches!(kind, ColumnKind::Text | ColumnKind::Integer)
        && present as f64 >= ID_SHARE * rows as f64
        && unique as f64 >= ID_SHARE * present as f64;
    let numbers = kind.is_number().then(|| number_stats(column)).flatten();
    ColumnProfile {
        index,
        name: column.name().to_string(),
        kind,
        missing,
        unique,
        numbers,
        looks_like,
        possible_id,
    }
}

/// Whether more than 90 % of `values` read as dates (ISO, `a/b/yyyy`) or as numbers (a code
/// with a leading zero is no number).
fn looks_like(values: &[&str]) -> Option<LooksLike> {
    if values.is_empty() {
        return None;
    }
    let share = |hits: usize| hits as f64 / values.len() as f64 > LOOKS_LIKE_SHARE;
    let dates = values
        .iter()
        .filter(|value| {
            crate::dataframe::iso_days(value).is_some()
                || crate::dataframe::date_guess([**value]).is_some()
        })
        .count();
    if share(dates) {
        return Some(LooksLike::Dates);
    }
    let numbers = values
        .iter()
        .filter(|value| {
            crate::dataframe::plain_number(value) && !crate::table_ops::has_leading_zero(value)
        })
        .count();
    share(numbers).then_some(LooksLike::Numbers)
}

/// The numbers of a number column; `None` when it has no (finite) values.
fn number_stats(column: &Column) -> Option<NumberStats> {
    let cast = column.cast(&DataType::Float64).ok()?;
    let floats = cast.f64().ok()?;
    let mut values: Zeroizing<Vec<(usize, f64)>> = Zeroizing::new(
        floats
            .iter()
            .enumerate()
            .filter_map(|(row, value)| value.filter(|v| v.is_finite()).map(|v| (row, v)))
            .collect(),
    );
    if values.is_empty() {
        return None;
    }
    let mut sorted: Zeroizing<Vec<f64>> =
        Zeroizing::new(values.iter().map(|(_, value)| *value).collect());
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    let mean = sorted.iter().sum::<f64>() / count as f64;
    let mut stats = NumberStats {
        count,
        min: sorted[0],
        max: sorted[count - 1],
        mean,
        median: quantile(&sorted, 0.5),
        q1: quantile(&sorted, 0.25),
        q3: quantile(&sorted, 0.75),
        outliers: 0,
        extremes: Vec::new(),
    };
    let (low, high) = (stats.low_fence(), stats.high_fence());
    // How far past its fence, the furthest first (the first row first on a tie).
    values.retain(|(_, value)| *value < low || *value > high);
    stats.outliers = values.len();
    values.sort_by(|a, b| {
        let past = |value: f64| {
            if value > high {
                value - high
            } else {
                low - value
            }
        };
        past(b.1).total_cmp(&past(a.1)).then(a.0.cmp(&b.0))
    });
    stats.extremes = values
        .iter()
        .take(MAX_EXTREMES)
        .map(|(row, value)| Extreme {
            row: *row,
            value: *value,
            high: *value > high,
        })
        .collect();
    Some(stats)
}

/// The `q` quantile of `sorted` (not empty), interpolated between the two nearest values.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    let place = q * (sorted.len() - 1) as f64;
    let below = place.floor() as usize;
    let above = place.ceil() as usize;
    sorted[below] + (sorted[above] - sorted[below]) * (place - below as f64)
}

/// Rows equal to an earlier row in every column.
fn duplicate_rows(df: &DataFrame) -> Option<usize> {
    if df.width() == 0 || df.height() == 0 {
        return Some(0);
    }
    let names: Vec<_> = df.get_column_names_owned();
    let groups = df.group_by(names).ok()?.get_groups().len();
    Some(df.height() - groups)
}

impl Finding {
    /// How serious it is, a fixed score: a type not recognised first, then missing values and
    /// duplicate rows (by their share), outliers (by theirs) and a column of one value.
    pub fn score(&self, profile: &TableProfile) -> u32 {
        let share = |count: usize| {
            let rows = profile.rows.max(1) as f64;
            (count as f64 * 100.0 / rows).round() as u32
        };
        match *self {
            Finding::NotRecognised { .. } => 80,
            Finding::Missing { count, .. } => 50 + share(count).min(40),
            Finding::DuplicateRows { count } => 50 + share(count).min(40),
            Finding::Outliers { count, .. } => 40 + (share(count) * 2).min(40),
            Finding::OneValue { .. } => 30,
        }
    }

    /// A short sentence in `lang`, with the column's name and numbers only.
    pub fn sentence(&self, profile: &TableProfile, lang: Lang) -> String {
        let name = |column: usize| {
            profile
                .columns
                .get(column)
                .map_or("", |column| column.name.as_str())
        };
        match *self {
            Finding::NotRecognised {
                column,
                looks: LooksLike::Dates,
            } => tf_in(lang, "profile_not_dates", &[&name(column)]),
            Finding::NotRecognised {
                column,
                looks: LooksLike::Numbers,
            } => tf_in(lang, "profile_not_numbers", &[&name(column)]),
            Finding::Missing { column, count } => tf_in(
                lang,
                "profile_missing",
                &[&name(column), &percent(count, profile.rows, lang)],
            ),
            Finding::Outliers { column, count } => match count {
                1 => tf_in(lang, "profile_one_outlier", &[&name(column)]),
                n => tf_in(lang, "profile_outliers", &[&name(column), &whole(n, lang)]),
            },
            Finding::DuplicateRows { count } => match count {
                1 => tf_in(
                    lang,
                    "profile_one_duplicate",
                    &[&percent(1, profile.rows, lang)],
                ),
                n => tf_in(
                    lang,
                    "profile_duplicates",
                    &[&whole(n, lang), &percent(n, profile.rows, lang)],
                ),
            },
            Finding::OneValue { column } => tf_in(lang, "profile_one_value", &[&name(column)]),
        }
    }
}

fn findings(profile: &TableProfile) -> Vec<Finding> {
    let mut found = Vec::new();
    for column in &profile.columns {
        let index = column.index;
        if column.kind == ColumnKind::Text
            && let Some(looks) = column.looks_like
        {
            found.push(Finding::NotRecognised {
                column: index,
                looks,
            });
        }
        if profile.rows > 0 && column.missing as f64 > MISSING_SHARE * profile.rows as f64 {
            found.push(Finding::Missing {
                column: index,
                count: column.missing,
            });
        }
        if let Some(numbers) = &column.numbers
            && numbers.outliers > 0
        {
            found.push(Finding::Outliers {
                column: index,
                count: numbers.outliers,
            });
        }
        if profile.rows >= MIN_ROWS && column.unique == 1 {
            found.push(Finding::OneValue { column: index });
        }
    }
    if let Some(count) = profile.duplicate_rows.filter(|count| *count > 0) {
        found.push(Finding::DuplicateRows { count });
    }
    // Stable: on a tie the column order (the duplicate rows last) stays.
    found.sort_by_key(|finding| std::cmp::Reverse(finding.score(profile)));
    found
}

impl TableProfile {
    /// The top findings as sentences, or "Nothing stands out".
    pub fn top(&self, lang: Lang) -> Vec<String> {
        if self.findings.is_empty() {
            return vec![t_in(lang, "profile_nothing").to_string()];
        }
        self.findings
            .iter()
            .take(TOP_FINDINGS)
            .map(|finding| finding.sentence(self, lang))
            .collect()
    }

    /// What the panel shows: the top findings, the table's size and duplicates, and one line
    /// per column (with the numbers of a number column; on screen only, not in the copy).
    pub fn text(&self, lang: Lang) -> Zeroizing<String> {
        let mut out = Zeroizing::new(String::new());
        for line in self.top(lang) {
            out.push_str("• ");
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&self.summary(lang));
        out.push_str("\n\n");
        for column in &self.columns {
            out.push_str(&column.name);
            out.push_str(" — ");
            out.push_str(&column.facts(self.rows, lang).join(" · "));
            if let Some(numbers) = &column.numbers {
                out.push_str("\n    ");
                out.push_str(&tf_in(
                    lang,
                    "profile_numbers_line",
                    &[
                        &number(numbers.min, lang),
                        &number(numbers.max, lang),
                        &number(numbers.mean, lang),
                        &number(numbers.median, lang),
                        &number(numbers.q1, lang),
                        &number(numbers.q3, lang),
                    ],
                ));
            }
            out.push('\n');
        }
        out
    }

    /// The profile as a Markdown table for the clipboard: column names, types and counts, no
    /// cell values (no min, max or median, which are values of a cell).
    pub fn markdown(&self, lang: Lang) -> Zeroizing<String> {
        let mut out = Zeroizing::new(String::new());
        out.push_str("## ");
        out.push_str(t_in(lang, "profile_title"));
        out.push_str("\n\n");
        out.push_str(&self.summary(lang));
        out.push_str("\n\n");
        for line in self.top(lang) {
            out.push_str("- ");
            out.push_str(&cell(&line));
            out.push('\n');
        }
        out.push('\n');
        let head = [
            t_in(lang, "profile_head_column"),
            t_in(lang, "profile_head_type"),
            t_in(lang, "profile_head_missing"),
            t_in(lang, "profile_head_unique"),
            t_in(lang, "profile_head_outliers"),
            t_in(lang, "profile_head_note"),
        ];
        out.push_str(&format!("| {} |\n", head.join(" | ")));
        out.push_str(&format!("|{}\n", "---|".repeat(head.len())));
        for column in &self.columns {
            let outliers = column
                .numbers
                .as_ref()
                .map_or(String::from("–"), |numbers| whole(numbers.outliers, lang));
            let row = [
                cell(&column.name),
                column.kind.title(lang).to_string(),
                format!(
                    "{} ({})",
                    whole(column.missing, lang),
                    percent(column.missing, self.rows, lang)
                ),
                whole(column.unique, lang),
                outliers,
                column.notes(lang).join(", "),
            ];
            out.push_str(&format!("| {} |\n", row.join(" | ")));
        }
        out
    }

    /// "1.234 rijen × 5 kolommen · 3 dubbele rijen".
    fn summary(&self, lang: Lang) -> String {
        let mut summary = tf_in(
            lang,
            "profile_size",
            &[&whole(self.rows, lang), &whole(self.columns.len(), lang)],
        );
        if let Some(count) = self.duplicate_rows {
            summary.push_str(" · ");
            summary.push_str(&tf_in(
                lang,
                "profile_duplicate_count",
                &[&whole(count, lang), &percent(count, self.rows, lang)],
            ));
        }
        summary
    }
}

impl ColumnProfile {
    /// Type, missing, unique and its notes, for the panel.
    fn facts(&self, rows: usize, lang: Lang) -> Vec<String> {
        let mut facts = vec![
            self.kind.title(lang).to_string(),
            tf_in(
                lang,
                "profile_missing_count",
                &[
                    &whole(self.missing, lang),
                    &percent(self.missing, rows, lang),
                ],
            ),
            tf_in(lang, "profile_unique_count", &[&whole(self.unique, lang)]),
        ];
        if let Some(numbers) = &self.numbers {
            facts.push(tf_in(
                lang,
                "profile_outlier_count",
                &[&whole(numbers.outliers, lang)],
            ));
        }
        facts.extend(self.notes(lang));
        facts
    }

    /// "possible ID", "looks like dates", "looks like numbers".
    fn notes(&self, lang: Lang) -> Vec<String> {
        let mut notes = Vec::new();
        if self.possible_id {
            notes.push(t_in(lang, "profile_possible_id").to_string());
        }
        if self.kind == ColumnKind::Text {
            match self.looks_like {
                Some(LooksLike::Dates) => notes.push(t_in(lang, "profile_looks_dates").to_string()),
                Some(LooksLike::Numbers) => {
                    notes.push(t_in(lang, "profile_looks_numbers").to_string());
                }
                None => {}
            }
        }
        notes
    }
}

/// A Markdown table cell: `|` escaped, on one line.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// `n` with the thousands mark of `lang` (`1.234` / `1,234`).
fn whole(n: usize, lang: Lang) -> String {
    let separator = match lang {
        Lang::Nl => '.',
        Lang::En => ',',
    };
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

/// `part` of `total` as a percentage: whole from 10 %, else one decimal (`4,5%` / `4.5%`).
fn percent(part: usize, total: usize, lang: Lang) -> String {
    if total == 0 {
        return String::from("0%");
    }
    let share = part as f64 * 100.0 / total as f64;
    let text = if share >= 10.0 || share.fract() == 0.0 {
        format!("{share:.0}")
    } else {
        format!("{share:.1}")
    };
    format!("{}%", decimal_mark(&text, lang))
}

/// `value` with at most two decimals, the decimal mark of `lang` (no thousands marks).
fn number(value: f64, lang: Lang) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    decimal_mark(if text == "-0" { "0" } else { text }, lang)
}

fn decimal_mark(text: &str, lang: Lang) -> String {
    match lang {
        Lang::Nl => text.replace('.', ","),
        Lang::En => text.to_string(),
    }
}

#[cfg(test)]
mod tests;
