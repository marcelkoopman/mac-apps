//! The table window's chart (Chart in its toolbar): two columns of the table of the version
//! shown, one point per row, as the flat JSON the Swift Charts view draws
//! (`swift/CopyCraftCharts`). Nothing is added up or averaged.
//!
//! The y column holds numbers; the x column is any column the chart can read. A number x gives
//! a scatter, a date x points along a time axis (both in x order), a text x points per value on
//! a category axis (in table order). Rows with an empty x, or an empty or non-numeric y, are
//! skipped and counted. Above [`MAX_POINTS`] rows an even, fixed sample is drawn (every n-th
//! row) and the note says so. Every label comes translated from here; the JSON is wiped when
//! dropped.

use polars::prelude::{Column, DataFrame, DataType, TimeUnit};
use zeroize::{Zeroize, Zeroizing};

use std::sync::Arc;

use crate::locale::{Lang, t_in, tf_in};
use crate::table_insight::TrendFacts;

/// At most this many points, taken evenly from the rows with values.
pub const MAX_POINTS: usize = 5000;

/// How the x axis reads, from the x column's role (not chosen on its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    /// A number column: a scatter.
    Scatter,
    /// A date column: points on a time axis.
    Dates,
    /// A text column: points per value on a category axis.
    Categories,
}

impl ChartKind {
    /// "scatter", "dates", "categories": the JSON's `kind`.
    pub fn code(self) -> &'static str {
        match self {
            ChartKind::Scatter => "scatter",
            ChartKind::Dates => "dates",
            ChartKind::Categories => "categories",
        }
    }
}

/// What a column can be in a chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnRole {
    /// A category axis (x only).
    Text,
    /// A time axis (x only).
    Date,
    /// A number axis (x or y).
    Number,
}

/// A column the chart can use: its place in the frame, its role and what makes it a good first
/// choice. Columns of other types (lists, booleans) are left out. Only flags: no names or values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartColumn {
    pub index: usize,
    pub role: ColumnRole,
    /// Text with few different values for its rows, not one per person: a good x column.
    pub category: bool,
    /// Text such as first or last names or e-mail addresses: never the first x column.
    pub personal: bool,
    /// Numbers that only number the rows (named id, `*_id`, or unique whole numbers counting
    /// up): never the first y column, and an x column only when nothing else fits.
    pub id: bool,
}

impl ChartColumn {
    /// A column with only its role (none of the first-choice flags).
    pub fn plain(index: usize, role: ColumnRole) -> ChartColumn {
        ChartColumn {
            index,
            role,
            category: false,
            personal: false,
            id: false,
        }
    }
}

/// The chart asked for: two column places, never names or values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChartRequest {
    /// The x column: any column the chart can read.
    pub x: usize,
    /// The y column: a number column.
    pub y: usize,
}

impl ChartRequest {
    /// The first chart `columns` allow, as a person would pick it. With two number columns
    /// that are not ids: the first on x, the next on y (a scatter, e.g. age against salary).
    /// Otherwise y is the first number column that is not an id (else an id), and x another
    /// column: a text column with few values (never names or e-mail addresses), else a date
    /// column, a number column that is not an id, any text column but names, an id, and last
    /// a column of names. `None` without a number column, or with nothing for x beside it.
    pub fn first(columns: &[ChartColumn]) -> Option<ChartRequest> {
        let number = |id: bool| move |c: &&ChartColumn| c.role == ColumnRole::Number && c.id == id;
        let mut plain = columns.iter().filter(number(false));
        if let (Some(x), Some(y)) = (plain.next(), plain.next()) {
            return Some(ChartRequest {
                x: x.index,
                y: y.index,
            });
        }
        let y = columns
            .iter()
            .find(number(false))
            .or_else(|| columns.iter().find(number(true)))?
            .index;
        let text = |c: &&ChartColumn| c.role == ColumnRole::Text;
        let order: [&dyn Fn(&&ChartColumn) -> bool; 6] = [
            &|c| text(c) && c.category && !c.personal,
            &|c| c.role == ColumnRole::Date,
            &number(false),
            &|c| text(c) && !c.personal,
            &number(true),
            &text,
        ];
        let x = order
            .iter()
            .find_map(|fits| columns.iter().filter(|c| c.index != y).find(|c| fits(c)))?
            .index;
        Some(ChartRequest { x, y })
    }

    /// This request made to fit `columns` (after a pop-up changed, or another version): the x
    /// kept when it is still offered, else the first choice's; the y kept when it is offered
    /// beside that x. An x that took the y's column moves the y to the next number column
    /// (round to the first); a y that is gone becomes the first choice's (or the first left).
    /// An x with no other number column beside it gives the first choice. Never x == y.
    /// `None` when no chart fits.
    pub fn fitted(self, columns: &[ChartColumn]) -> Option<ChartRequest> {
        let first = ChartRequest::first(columns)?;
        let x = if x_choices(columns).contains(&self.x) {
            self.x
        } else {
            first.x
        };
        let ys = y_choices(columns, x);
        let y = if ys.contains(&self.y) {
            self.y
        } else if self.y == x {
            match ys.iter().find(|&&y| y > self.y).or(ys.first()) {
                Some(&next) => next,
                None => return Some(first),
            }
        } else if ys.contains(&first.y) {
            first.y
        } else {
            match ys.first() {
                Some(&y) => y,
                None => return Some(first),
            }
        };
        Some(ChartRequest { x, y })
    }
}

/// The columns the x pop-up offers: every column the chart can read.
pub fn x_choices(columns: &[ChartColumn]) -> Vec<usize> {
    columns.iter().map(|c| c.index).collect()
}

/// The columns the y pop-up offers beside the x column `x`: the other number columns.
pub fn y_choices(columns: &[ChartColumn], x: usize) -> Vec<usize> {
    columns
        .iter()
        .filter(|c| c.role == ColumnRole::Number && c.index != x)
        .map(|c| c.index)
        .collect()
}

/// What the note under the pop-ups says about the points: values skipped, and a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChartNote {
    /// Rows left out: an empty x, an empty or non-numeric y.
    pub skipped: usize,
    /// The points drawn.
    pub shown: usize,
    /// The rows with values; more than `shown` when a sample is drawn.
    pub total: usize,
}

impl ChartNote {
    /// "Steekproef van 5.000 van 64.659 rijen · 3 waarden overgeslagen", or empty.
    pub fn text(self, lang: Lang) -> String {
        use crate::table_profile::whole;
        let mut parts: Vec<String> = Vec::new();
        if self.shown < self.total {
            parts.push(tf_in(
                lang,
                "chart_sample",
                &[&whole(self.shown, lang), &whole(self.total, lang)],
            ));
        }
        match self.skipped {
            0 => {}
            1 => parts.push(t_in(lang, "chart_one_skipped").to_string()),
            n => parts.push(tf_in(lang, "chart_skipped", &[&whole(n, lang)])),
        }
        parts.join(" · ")
    }
}

/// A chart worked out on a worker, for the table window: the JSON to draw (with its note) or
/// why there is none. `generation` is the window's number of the request: an older one is
/// dropped. Its `Debug` shows no labels or values.
pub struct ChartOutcome {
    pub generation: u64,
    pub result: Result<(Zeroizing<String>, ChartNote), ChartProblem>,
    /// The points for Summarize: a date x with at least two points.
    pub trend: Option<Arc<TrendFacts>>,
}

impl ChartOutcome {
    /// [`build`] and [`ChartData::to_json`] for the window's request number `generation`;
    /// Summarize gets the points drawn.
    pub fn build(df: &DataFrame, request: &ChartRequest, lang: Lang, generation: u64) -> Self {
        match build(df, request, lang) {
            Ok(data) => ChartOutcome {
                generation,
                trend: TrendFacts::from_chart(&data).map(Arc::new),
                result: Ok((data.to_json(), data.note)),
            },
            Err(problem) => ChartOutcome {
                generation,
                result: Err(problem),
                trend: None,
            },
        }
    }
}

impl Clone for ChartOutcome {
    fn clone(&self) -> Self {
        ChartOutcome {
            generation: self.generation,
            result: match &self.result {
                Ok((json, note)) => Ok((Zeroizing::new(json.as_str().to_string()), *note)),
                Err(problem) => Err(*problem),
            },
            trend: self.trend.clone(),
        }
    }
}

impl std::fmt::Debug for ChartOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChartOutcome")
            .field("generation", &self.generation)
            .field("drawn", &self.result.is_ok())
            .field("trend", &self.trend.is_some())
            .finish_non_exhaustive()
    }
}

/// Why no chart could be drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartProblem {
    /// The columns asked for are not there or do not fit the chart.
    Columns,
    /// No row has a value to draw.
    NoValues,
}

impl ChartProblem {
    pub fn message(self, lang: Lang) -> &'static str {
        match self {
            ChartProblem::Columns => t_in(lang, "chart_no_columns"),
            ChartProblem::NoValues => t_in(lang, "chart_no_values"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Points {
    /// `(x, y)`: a number x, or a date x in days since 1970-01-01 (with the time of day as a
    /// fraction); in x order.
    Pairs(Vec<(f64, f64)>),
    /// `(x's text, y)`, in table order.
    Labelled(Vec<(String, f64)>),
}

/// What the Swift view draws ([`ChartData::to_json`]). Its labels are wiped when dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartData {
    pub kind: ChartKind,
    pub lang: Lang,
    /// The x column's name.
    pub x_label: String,
    /// The y column's name.
    pub y_label: String,
    /// The chart in words for VoiceOver and the legend: "age tegen city" / "age vs. city".
    pub series: String,
    pub note: ChartNote,
    pub points: Points,
}

impl Drop for ChartData {
    fn drop(&mut self) {
        self.x_label.zeroize();
        self.y_label.zeroize();
        self.series.zeroize();
        match &mut self.points {
            Points::Pairs(pairs) => pairs.zeroize(),
            Points::Labelled(points) => {
                for (label, value) in points.iter_mut() {
                    label.zeroize();
                    *value = 0.0;
                }
            }
        }
    }
}

/// The columns of `df` a chart can use, in order, with what makes each a good first choice.
pub fn chart_columns(df: &DataFrame) -> Vec<ChartColumn> {
    let rows = df.height();
    df.columns()
        .iter()
        .enumerate()
        .filter_map(|(index, column)| {
            let role = role(column.dtype())?;
            let mut chart = ChartColumn::plain(index, role);
            match role {
                ColumnRole::Text => {
                    chart.personal = personal_name(column.name()) || mostly_addresses(column);
                    chart.category = few_values(column, rows);
                }
                ColumnRole::Number => {
                    chart.id = id_name(column.name()) || counts_up(column);
                }
                ColumnRole::Date => {}
            }
            Some(chart)
        })
        .collect()
}

/// So many different values or fewer always make a good x column.
const FEW_VALUES: usize = 10;

/// The words of a column name, lower case: split at anything not a letter or digit and where
/// a capital follows a small letter (`userId` is "user", "id").
fn name_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut after_small = false;
    for c in name.chars() {
        let boundary = !c.is_alphanumeric() || (c.is_uppercase() && after_small);
        if boundary && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        }
        after_small = c.is_lowercase();
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Words that make a column one value per person (e-mail, phone, address and the like).
const PERSONAL_WORDS: [&str; 22] = [
    "email",
    "mail",
    "phone",
    "telephone",
    "telefoon",
    "tel",
    "mobile",
    "mobiel",
    "address",
    "adres",
    "street",
    "straat",
    "postcode",
    "zip",
    "iban",
    "uuid",
    "guid",
    "url",
    "ip",
    "firstname",
    "lastname",
    "fullname",
];

/// Words a person's name column is made of (`first_name`, `achternaam`, `customer name`).
const NAME_WORDS: [&str; 20] = [
    "name",
    "naam",
    "first",
    "last",
    "full",
    "given",
    "family",
    "middle",
    "sur",
    "surname",
    "user",
    "username",
    "voornaam",
    "achternaam",
    "voor",
    "achter",
    "tussenvoegsel",
    "customer",
    "klant",
    "contact",
];

/// A column name for names, e-mail addresses and the like: `first_name`, `lastName`, `email`,
/// `e-mail`, `phone`. `product_name` is not (only `name`/`naam` with person words).
fn personal_name(name: &str) -> bool {
    let words = name_words(name);
    words.iter().any(|w| PERSONAL_WORDS.contains(&w.as_str()))
        || (!words.is_empty()
            && words
                .iter()
                .any(|w| w.contains("name") || w.contains("naam"))
            && words.iter().all(|w| NAME_WORDS.contains(&w.as_str())))
}

/// More than half of the first values (at most 100 filled ones) hold an `@`: e-mail addresses.
fn mostly_addresses(column: &Column) -> bool {
    let Ok(values) = column.str() else {
        return false;
    };
    let (mut filled, mut at) = (0usize, 0usize);
    for value in values
        .iter()
        .flatten()
        .filter(|v| !v.trim().is_empty())
        .take(100)
    {
        filled += 1;
        if value.contains('@') {
            at += 1;
        }
    }
    filled > 0 && at * 2 > filled
}

/// At least two different values, and few of them for the rows: at most [`FEW_VALUES`], or
/// on average two rows or more per value.
fn few_values(column: &Column, rows: usize) -> bool {
    let Ok(distinct) = column.as_materialized_series().n_unique() else {
        return false;
    };
    // Empty cells count as a value in `n_unique`.
    let distinct = distinct.saturating_sub(usize::from(column.null_count() > 0));
    distinct >= 2 && (distinct <= FEW_VALUES || distinct * 2 <= rows)
}

/// `id`, `ID`, `user_id`, `userId`, `order-id`: the name of a row number.
fn id_name(name: &str) -> bool {
    name_words(name).last().is_some_and(|w| w == "id")
}

/// Whole numbers, at least two, each larger than the one before (so all different): a row number.
fn counts_up(column: &Column) -> bool {
    if !column.dtype().is_integer() || column.null_count() > 0 {
        return false;
    }
    let Ok(cast) = column.cast(&DataType::Int64) else {
        return false;
    };
    let Ok(values) = cast.i64() else {
        return false;
    };
    let mut last: Option<i64> = None;
    for value in values.iter() {
        let Some(value) = value else { return false };
        if last.is_some_and(|last| value <= last) {
            return false;
        }
        last = Some(value);
    }
    values.len() >= 2
}

fn role(dtype: &DataType) -> Option<ColumnRole> {
    if dtype.is_string() {
        Some(ColumnRole::Text)
    } else if dtype.is_date() || dtype.is_datetime() {
        Some(ColumnRole::Date)
    } else if dtype.is_primitive_numeric() {
        Some(ColumnRole::Number)
    } else {
        None
    }
}

/// The chart `request` asks for of `df`, labelled in `lang`: one point per row with values.
pub fn build(
    df: &DataFrame,
    request: &ChartRequest,
    lang: Lang,
) -> Result<ChartData, ChartProblem> {
    if request.x == request.y {
        return Err(ChartProblem::Columns);
    }
    let x = df.columns().get(request.x).ok_or(ChartProblem::Columns)?;
    let y = df.columns().get(request.y).ok_or(ChartProblem::Columns)?;
    let ys = numbers(y)?;
    let kind = match role(x.dtype()) {
        Some(ColumnRole::Number) => ChartKind::Scatter,
        Some(ColumnRole::Date) => ChartKind::Dates,
        Some(ColumnRole::Text) => ChartKind::Categories,
        None => return Err(ChartProblem::Columns),
    };
    let x_label = x.name().to_string();
    let y_label = y.name().to_string();
    let series = tf_in(lang, "chart_scatter_series", &[&y_label, &x_label]);
    let (points, note) = match kind {
        ChartKind::Categories => {
            let texts = x.str().map_err(|_| ChartProblem::Columns)?;
            let rows: Vec<usize> = (0..ys.len())
                .filter(|&row| {
                    ys[row].is_finite()
                        && texts.get(row).is_some_and(|text| !text.trim().is_empty())
                })
                .collect();
            let (rows, note) = sample(rows, ys.len());
            let points = rows
                .into_iter()
                .filter_map(|row| Some((texts.get(row)?.to_string(), ys[row])))
                .collect();
            (Points::Labelled(points), note)
        }
        ChartKind::Scatter | ChartKind::Dates => {
            let xs = if kind == ChartKind::Dates {
                days(x)?
            } else {
                numbers(x)?
            };
            let rows: Vec<usize> = (0..ys.len())
                .filter(|&row| ys[row].is_finite() && xs.get(row).is_some_and(|v| v.is_finite()))
                .collect();
            let (rows, note) = sample(rows, ys.len());
            let mut pairs: Vec<(f64, f64)> =
                rows.into_iter().map(|row| (xs[row], ys[row])).collect();
            // In x order (stable: rows with the same x keep the table's order).
            pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
            (Points::Pairs(pairs), note)
        }
    };
    let empty = match &points {
        Points::Pairs(pairs) => pairs.is_empty(),
        Points::Labelled(points) => points.is_empty(),
    };
    if empty {
        return Err(ChartProblem::NoValues);
    }
    Ok(ChartData {
        kind,
        lang,
        x_label,
        y_label,
        series,
        note,
        points,
    })
}

/// At most [`MAX_POINTS`] of `rows` (the rows with values, of `height`), taken evenly: every
/// n-th, the same ones each time. With the note on what was skipped and sampled.
fn sample(rows: Vec<usize>, height: usize) -> (Vec<usize>, ChartNote) {
    let total = rows.len();
    let note = |shown: usize| ChartNote {
        skipped: height - total,
        shown,
        total,
    };
    if total <= MAX_POINTS {
        return (rows, note(total));
    }
    let picked = (0..MAX_POINTS)
        .map(|i| rows[i * total / MAX_POINTS])
        .collect();
    (picked, note(MAX_POINTS))
}

/// A number column as `f64`, `NaN` where empty or not a number.
fn numbers(column: &Column) -> Result<Zeroizing<Vec<f64>>, ChartProblem> {
    if role(column.dtype()) != Some(ColumnRole::Number) {
        return Err(ChartProblem::Columns);
    }
    let cast = column
        .cast(&DataType::Float64)
        .map_err(|_| ChartProblem::Columns)?;
    let floats = cast.f64().map_err(|_| ChartProblem::Columns)?;
    Ok(Zeroizing::new(
        floats
            .iter()
            .map(|value| value.filter(|v| v.is_finite()).unwrap_or(f64::NAN))
            .collect(),
    ))
}

/// A date column as days since 1970-01-01 (a time of day as the fraction), `NaN` where empty.
fn days(column: &Column) -> Result<Zeroizing<Vec<f64>>, ChartProblem> {
    let per_day = match column.dtype() {
        DataType::Date => 1.0,
        DataType::Datetime(TimeUnit::Milliseconds, _) => 86_400_000.0,
        DataType::Datetime(TimeUnit::Microseconds, _) => 86_400_000_000.0,
        DataType::Datetime(TimeUnit::Nanoseconds, _) => 86_400_000_000_000.0,
        _ => return Err(ChartProblem::Columns),
    };
    let cast = column
        .cast(&DataType::Int64)
        .map_err(|_| ChartProblem::Columns)?;
    let ticks = cast.i64().map_err(|_| ChartProblem::Columns)?;
    Ok(Zeroizing::new(
        ticks
            .iter()
            .map(|tick| tick.map_or(f64::NAN, |tick| tick as f64 / per_day))
            .collect(),
    ))
}

/// Year, month and day of `days` since 1970-01-01 (proleptic Gregorian; H. Hinnant's
/// `civil_from_days`).
pub(crate) fn civil_from_days(days: i32) -> (i64, u32, u32) {
    let z = i64::from(days) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

impl ChartData {
    /// The flat JSON the Swift view decodes: `kind`, `locale`, the axis labels, `series` and
    /// the `points`: `{x, y}` (a number, or days since 1970-01-01 for dates) or `{label, y}`
    /// (a text x). Wiped when dropped.
    pub fn to_json(&self) -> Zeroizing<String> {
        use serde_json::{Map, Value, json};
        let points: Vec<Value> = match &self.points {
            Points::Pairs(pairs) => pairs
                .iter()
                .map(|(x, y)| json!({ "x": x, "y": y }))
                .collect(),
            Points::Labelled(points) => points
                .iter()
                .map(|(label, y)| json!({ "label": label, "y": y }))
                .collect(),
        };
        let mut root = Map::new();
        root.insert("kind".into(), self.kind.code().into());
        root.insert(
            "locale".into(),
            match self.lang {
                Lang::Nl => "nl",
                Lang::En => "en",
            }
            .into(),
        );
        root.insert("x_label".into(), self.x_label.clone().into());
        root.insert("y_label".into(), self.y_label.clone().into());
        root.insert("series".into(), self.series.clone().into());
        root.insert("points".into(), Value::Array(points));
        let mut value = Value::Object(root);
        // Measured first, so the buffer never grows (a grown buffer leaves its old copy behind).
        let mut size = Measure(0);
        let _ = serde_json::to_writer(&mut size, &value);
        let mut out = Zeroizing::new(Vec::with_capacity(size.0));
        let _ = serde_json::to_writer(&mut *out, &value);
        wipe(&mut value);
        let text = String::from_utf8(std::mem::take(&mut *out)).unwrap_or_default();
        Zeroizing::new(text)
    }
}

/// Counts the bytes written, keeps none.
struct Measure(usize);

impl std::io::Write for Measure {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Zeroize every string in `value`.
fn wipe(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Object(map) => map.values_mut().for_each(wipe),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
