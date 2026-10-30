//! The table window's chart (Chart in its toolbar): the table of the version shown, grouped and
//! aggregated with eager polars (`DataFrame::group_by`, no lazy), as the flat JSON the Swift
//! Charts view draws (`swift/CopyCraftCharts`).
//!
//! Bar and line: a text or date column groups the rows (dates per day, month or year), a number
//! column gives the values, counted, summed, averaged or taken the least or the most of. At most
//! [`MAX_GROUPS`] groups: the rest are folded into "Other". Scatter: two number columns, no
//! groups, at most [`MAX_POINTS`] points sampled evenly. Empty and non-numeric values are
//! skipped and counted. Every label comes translated from here; the JSON is wiped when dropped.

use polars::prelude::{Column, DataFrame, DataType, GroupsIndicator};
use zeroize::{Zeroize, Zeroizing};

use std::sync::Arc;

use crate::locale::{Lang, t_in, tf_in};
use crate::table_insight::TrendFacts;

/// At most this many bars or line points; the groups with the fewest rows go into "Other".
pub const MAX_GROUPS: usize = 50;
/// At most this many scatter points, taken evenly from the rows.
pub const MAX_POINTS: usize = 2000;

/// What the chart draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Scatter,
}

impl ChartKind {
    pub const ALL: [ChartKind; 3] = [ChartKind::Bar, ChartKind::Line, ChartKind::Scatter];

    /// "bar", "line", "scatter": the JSON's `kind`.
    pub fn code(self) -> &'static str {
        match self {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Scatter => "scatter",
        }
    }

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            ChartKind::Bar => t_in(lang, "chart_bar"),
            ChartKind::Line => t_in(lang, "chart_line"),
            ChartKind::Scatter => t_in(lang, "chart_scatter"),
        }
    }
}

/// How a group's values become one number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aggregate {
    Count,
    #[default]
    Sum,
    Mean,
    Min,
    Max,
}

impl Aggregate {
    pub const ALL: [Aggregate; 5] = [
        Aggregate::Count,
        Aggregate::Sum,
        Aggregate::Mean,
        Aggregate::Min,
        Aggregate::Max,
    ];

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Aggregate::Count => t_in(lang, "chart_count"),
            Aggregate::Sum => t_in(lang, "chart_sum"),
            Aggregate::Mean => t_in(lang, "chart_mean"),
            Aggregate::Min => t_in(lang, "chart_min"),
            Aggregate::Max => t_in(lang, "chart_max"),
        }
    }
}

/// The step a date column is grouped by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bucket {
    Day,
    /// Monday to Sunday (ISO weeks), labelled by the Monday.
    Week,
    #[default]
    Month,
    Year,
}

impl Bucket {
    pub const ALL: [Bucket; 4] = [Bucket::Day, Bucket::Week, Bucket::Month, Bucket::Year];

    /// "day", "week", "month", "year": the JSON's `unit` (how Swift formats the dates).
    pub fn code(self) -> &'static str {
        match self {
            Bucket::Day => "day",
            Bucket::Week => "week",
            Bucket::Month => "month",
            Bucket::Year => "year",
        }
    }

    pub fn title(self, lang: Lang) -> &'static str {
        match self {
            Bucket::Day => t_in(lang, "chart_by_day"),
            Bucket::Week => t_in(lang, "chart_by_week"),
            Bucket::Month => t_in(lang, "chart_by_month"),
            Bucket::Year => t_in(lang, "chart_by_year"),
        }
    }
}

/// What a column can be in a chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnRole {
    /// Groups the rows (bar, line).
    Text,
    /// Groups the rows per day, month or year (bar, line).
    Date,
    /// Values (bar, line) or an axis (scatter).
    Number,
}

/// A column the chart can use: its place in the frame and its role. Columns of other types
/// (lists, booleans) are left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartColumn {
    pub index: usize,
    pub role: ColumnRole,
}

/// The chart asked for: column places and choices, never names or values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChartRequest {
    pub kind: ChartKind,
    /// The group column (bar, line) or the x column (scatter).
    pub x: usize,
    /// The value column (bar, line; not needed to count) or the y column (scatter).
    pub y: Option<usize>,
    pub aggregate: Aggregate,
    pub bucket: Bucket,
}

impl ChartRequest {
    /// The first chart `columns` allow: a bar per group of the first text or date column, the
    /// average of the first number column (else the row count); without a group column, a
    /// scatter of the first two number columns. `None` when neither fits. The average, not the
    /// sum: a sum per group runs up to totals far from any value in the column.
    pub fn first(columns: &[ChartColumn]) -> Option<ChartRequest> {
        let group = columns.iter().find(|c| c.role != ColumnRole::Number);
        let mut numbers = columns.iter().filter(|c| c.role == ColumnRole::Number);
        let first_number = numbers.next().map(|c| c.index);
        if let Some(group) = group {
            return Some(ChartRequest {
                kind: ChartKind::Bar,
                x: group.index,
                y: first_number,
                aggregate: if first_number.is_some() {
                    Aggregate::Mean
                } else {
                    Aggregate::Count
                },
                bucket: Bucket::Month,
            });
        }
        let second = numbers.next().map(|c| c.index)?;
        Some(ChartRequest {
            kind: ChartKind::Scatter,
            x: first_number?,
            y: Some(second),
            // Kept when the kind changes to bar or line.
            aggregate: Aggregate::Mean,
            bucket: Bucket::Month,
        })
    }
}

/// The kinds `columns` allow: bar and line need a text or date column, a scatter two number
/// columns.
pub fn kinds(columns: &[ChartColumn]) -> Vec<ChartKind> {
    let groups = columns.iter().any(|c| c.role != ColumnRole::Number);
    let numbers = columns
        .iter()
        .filter(|c| c.role == ColumnRole::Number)
        .count();
    ChartKind::ALL
        .into_iter()
        .filter(|kind| match kind {
            ChartKind::Bar | ChartKind::Line => groups,
            ChartKind::Scatter => numbers >= 2,
        })
        .collect()
}

/// The columns the x pop-up offers for `kind`: the text and date columns (bar, line) or the
/// number columns (scatter).
pub fn x_choices(columns: &[ChartColumn], kind: ChartKind) -> Vec<usize> {
    columns
        .iter()
        .filter(|c| (c.role == ColumnRole::Number) == (kind == ChartKind::Scatter))
        .map(|c| c.index)
        .collect()
}

/// The columns the value (or y) pop-up offers: the number columns.
pub fn y_choices(columns: &[ChartColumn]) -> Vec<usize> {
    columns
        .iter()
        .filter(|c| c.role == ColumnRole::Number)
        .map(|c| c.index)
        .collect()
}

/// The role of the column at `index`.
pub fn role_of(columns: &[ChartColumn], index: usize) -> Option<ColumnRole> {
    columns.iter().find(|c| c.index == index).map(|c| c.role)
}

impl ChartRequest {
    /// This request made to fit `columns` (after a pop-up changed, or another version): a kind
    /// they allow, an x and y they offer for it (a scatter's y not its x), counting when there
    /// is no value column. `None` when no chart fits.
    pub fn fitted(self, columns: &[ChartColumn]) -> Option<ChartRequest> {
        let kinds = kinds(columns);
        let kind = if kinds.contains(&self.kind) {
            self.kind
        } else {
            *kinds.first()?
        };
        let xs = x_choices(columns, kind);
        let x = if xs.contains(&self.x) {
            self.x
        } else {
            *xs.first()?
        };
        let ys = y_choices(columns);
        let fits = |y: &usize| kind != ChartKind::Scatter || *y != x;
        let y = match self.y {
            Some(y) if ys.contains(&y) && fits(&y) => Some(y),
            _ => ys.iter().copied().find(fits),
        };
        if kind == ChartKind::Scatter && y.is_none() {
            return None;
        }
        let aggregate = if y.is_none() {
            Aggregate::Count
        } else {
            self.aggregate
        };
        Some(ChartRequest {
            kind,
            x,
            y,
            aggregate,
            bucket: self.bucket,
        })
    }
}

/// A chart worked out on a worker, for the table window: the JSON to draw (with the count of
/// skipped values) or why there is none. `generation` is the window's number of the request:
/// an older one is dropped. Its `Debug` shows no labels or values.
pub struct ChartOutcome {
    pub generation: u64,
    pub result: Result<(Zeroizing<String>, usize), ChartProblem>,
    /// The series for Summarize: a drawn bar or line chart per week or month.
    pub trend: Option<Arc<TrendFacts>>,
}

impl ChartOutcome {
    /// [`build`] and [`ChartData::to_json`] for the window's request number `generation`.
    pub fn build(df: &DataFrame, request: &ChartRequest, lang: Lang, generation: u64) -> Self {
        let result = build(df, request, lang).map(|data| (data.to_json(), data.skipped));
        let trend = if result.is_ok() {
            TrendFacts::build(df, request).map(Arc::new)
        } else {
            None
        };
        ChartOutcome {
            generation,
            result,
            trend,
        }
    }
}

impl Clone for ChartOutcome {
    fn clone(&self) -> Self {
        ChartOutcome {
            generation: self.generation,
            result: match &self.result {
                Ok((json, skipped)) => Ok((Zeroizing::new(json.as_str().to_string()), *skipped)),
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

/// One bar or line point.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    /// The group's text, a date as `yyyy-mm-dd` / `yyyy-mm` / `yyyy`, or "Other".
    pub label: String,
    pub value: f64,
    /// The first day of a date group, `yyyy-mm-dd` (Swift formats it in the app's language).
    pub date: Option<String>,
    /// The folded groups beyond [`MAX_GROUPS`].
    pub other: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Points {
    Bars(Vec<Bar>),
    Pairs(Vec<(f64, f64)>),
}

/// What the Swift view draws ([`ChartData::to_json`]). Its labels are wiped when dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartData {
    pub kind: ChartKind,
    pub lang: Lang,
    pub x_label: String,
    /// The value axis: the legend's text for a bar or line chart ("Gemiddelde van age"),
    /// "Aantal" / "Count" for a count, the column's name for a scatter.
    pub y_label: String,
    /// The legend's one entry.
    pub series: String,
    /// The date step of a date group column.
    pub unit: Option<Bucket>,
    /// Rows left out: an empty group, an empty or non-numeric value.
    pub skipped: usize,
    pub points: Points,
}

impl Drop for ChartData {
    fn drop(&mut self) {
        self.x_label.zeroize();
        self.y_label.zeroize();
        self.series.zeroize();
        match &mut self.points {
            Points::Bars(bars) => {
                for bar in bars.iter_mut() {
                    bar.label.zeroize();
                    bar.value = 0.0;
                    if let Some(date) = bar.date.as_mut() {
                        date.zeroize();
                    }
                }
            }
            Points::Pairs(pairs) => pairs.zeroize(),
        }
    }
}

/// The columns of `df` a chart can use, in order.
pub fn chart_columns(df: &DataFrame) -> Vec<ChartColumn> {
    df.columns()
        .iter()
        .enumerate()
        .filter_map(|(index, column)| {
            let role = role(column.dtype())?;
            Some(ChartColumn { index, role })
        })
        .collect()
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

/// The chart `request` asks for of `df`, labelled in `lang`.
pub fn build(
    df: &DataFrame,
    request: &ChartRequest,
    lang: Lang,
) -> Result<ChartData, ChartProblem> {
    let x = df.columns().get(request.x).ok_or(ChartProblem::Columns)?;
    let y = match request.y {
        Some(index) => Some(df.columns().get(index).ok_or(ChartProblem::Columns)?),
        None => None,
    };
    match request.kind {
        ChartKind::Scatter => scatter(x, y.ok_or(ChartProblem::Columns)?, lang),
        ChartKind::Bar | ChartKind::Line => grouped(df.height(), x, y, request, lang),
    }
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

fn scatter(x: &Column, y: &Column, lang: Lang) -> Result<ChartData, ChartProblem> {
    let xs = numbers(x)?;
    let ys = numbers(y)?;
    let mut pairs: Vec<(f64, f64)> = xs
        .iter()
        .zip(ys.iter())
        .filter(|(a, b)| !a.is_nan() && !b.is_nan())
        .map(|(a, b)| (*a, *b))
        .collect();
    let skipped = xs.len() - pairs.len();
    if pairs.is_empty() {
        return Err(ChartProblem::NoValues);
    }
    if pairs.len() > MAX_POINTS {
        let total = pairs.len();
        let sampled: Vec<(f64, f64)> = (0..MAX_POINTS)
            .map(|i| pairs[i * total / MAX_POINTS])
            .collect();
        pairs.zeroize();
        pairs = sampled;
    }
    let x_label = x.name().to_string();
    let y_label = y.name().to_string();
    let series = tf_in(lang, "chart_scatter_series", &[&y_label, &x_label]);
    Ok(ChartData {
        kind: ChartKind::Scatter,
        lang,
        x_label,
        y_label,
        series,
        unit: None,
        skipped,
        points: Points::Pairs(pairs),
    })
}

/// One group's numbers, before "Other" is folded.
#[derive(Debug, Clone, Copy)]
struct Tally {
    /// Rows with this group.
    rows: usize,
    /// Of those, rows with a value (the rest are skipped).
    values: usize,
    sum: f64,
    min: f64,
    max: f64,
}

impl Tally {
    const EMPTY: Tally = Tally {
        rows: 0,
        values: 0,
        sum: 0.0,
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
    };

    fn add(&mut self, value: f64) {
        self.values += 1;
        self.sum += value;
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    fn fold(&mut self, other: &Tally) {
        self.rows += other.rows;
        self.values += other.values;
        self.sum += other.sum;
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }

    fn value(&self, aggregate: Aggregate) -> f64 {
        match aggregate {
            Aggregate::Count => self.rows as f64,
            Aggregate::Sum => self.sum,
            Aggregate::Mean => self.sum / self.values as f64,
            Aggregate::Min => self.min,
            Aggregate::Max => self.max,
        }
    }
}

/// A group's key: its text or the first day of its date step (days since 1970-01-01).
enum Key {
    Text(String),
    Day(i32),
}

fn grouped(
    height: usize,
    x: &Column,
    y: Option<&Column>,
    request: &ChartRequest,
    lang: Lang,
) -> Result<ChartData, ChartProblem> {
    let counting = request.aggregate == Aggregate::Count;
    let values = match y {
        Some(column) if !counting => Some(numbers(column)?),
        None if !counting => return Err(ChartProblem::Columns),
        _ => None,
    };
    let dates = match role(x.dtype()) {
        Some(ColumnRole::Text) => false,
        Some(ColumnRole::Date) => true,
        _ => return Err(ChartProblem::Columns),
    };
    // The group column as it is (text), or the first day of each date's step: no copy of the
    // text is made for the grouping.
    let key = if dates {
        bucket_days(x, request.bucket)?
    } else {
        x.clone()
    };
    let name = key.name().clone();
    let frame = DataFrame::new(height, vec![key]).map_err(|_| ChartProblem::Columns)?;
    let by = frame
        .group_by_stable([name.as_str()])
        .map_err(|_| ChartProblem::Columns)?;
    let keys = by.keys().into_iter().next().ok_or(ChartProblem::Columns)?;
    let mut groups: Vec<(Key, Tally)> = Vec::new();
    let mut skipped = 0;
    for (place, group) in by.get_groups().iter().enumerate() {
        let rows = group_rows(&group);
        let key = if dates {
            keys.i32()
                .map_err(|_| ChartProblem::Columns)?
                .get(place)
                .map(Key::Day)
        } else {
            keys.str()
                .map_err(|_| ChartProblem::Columns)?
                .get(place)
                .filter(|text| !text.trim().is_empty())
                .map(|text| Key::Text(text.to_string()))
        };
        let Some(key) = key else {
            skipped += rows.len();
            continue;
        };
        let mut tally = Tally::EMPTY;
        for row in rows {
            match &values {
                Some(values) => match values.get(row) {
                    Some(value) if !value.is_nan() => {
                        tally.rows += 1;
                        tally.add(*value);
                    }
                    _ => skipped += 1,
                },
                None => tally.rows += 1,
            }
        }
        if tally.rows > 0 {
            groups.push((key, tally));
        } else if let Key::Text(mut text) = key {
            text.zeroize();
        }
    }
    if groups.is_empty() {
        return Err(ChartProblem::NoValues);
    }
    let other = fold_other(&mut groups);
    // Dates in time order; text bars from the largest value, text lines as they come.
    if dates {
        groups.sort_by_key(|(key, _)| match key {
            Key::Day(day) => *day,
            Key::Text(_) => 0,
        });
    } else if request.kind == ChartKind::Bar {
        groups.sort_by(|a, b| {
            b.1.value(request.aggregate)
                .total_cmp(&a.1.value(request.aggregate))
        });
    }
    let mut bars: Vec<Bar> = groups
        .into_iter()
        .map(|(key, tally)| {
            let value = tally.value(request.aggregate);
            match key {
                Key::Text(label) => Bar {
                    label,
                    value,
                    date: None,
                    other: false,
                },
                Key::Day(day) => Bar {
                    label: date_label(day, request.bucket),
                    value,
                    date: Some(date_label(day, Bucket::Day)),
                    other: false,
                },
            }
        })
        .collect();
    if let Some(tally) = other {
        bars.push(Bar {
            label: t_in(lang, "chart_other").to_string(),
            value: tally.value(request.aggregate),
            date: None,
            other: true,
        });
    }
    let x_label = x.name().to_string();
    let (y_label, series) = match (counting, y) {
        (false, Some(column)) => {
            let name = Zeroizing::new(column.name().to_string());
            // The value axis says what it measures, as the legend does ("Sum of age").
            let series = tf_in(
                lang,
                "chart_series",
                &[&request.aggregate.title(lang), &name.as_str()],
            );
            (series.clone(), series)
        }
        _ => (
            t_in(lang, "chart_count").to_string(),
            t_in(lang, "chart_rows").to_string(),
        ),
    };
    Ok(ChartData {
        kind: request.kind,
        lang,
        x_label,
        y_label,
        series,
        unit: dates.then_some(request.bucket),
        skipped,
        points: Points::Bars(bars),
    })
}

/// The values of a bar or line chart grouped per week or month of a date column, in time order,
/// with every group (none folded into "Other"): `(first day of the step, value)`. `None` for
/// any other chart. Feeds the trend summary ([`crate::table_insight::TrendFacts`]).
pub fn date_series(df: &DataFrame, request: &ChartRequest) -> Option<Vec<(i32, f64)>> {
    if request.kind == ChartKind::Scatter || !matches!(request.bucket, Bucket::Week | Bucket::Month)
    {
        return None;
    }
    let x = df.columns().get(request.x)?;
    if role(x.dtype()) != Some(ColumnRole::Date) {
        return None;
    }
    let values = match request.aggregate {
        Aggregate::Count => None,
        _ => Some(numbers(df.columns().get(request.y?)?).ok()?),
    };
    let days = bucket_days(x, request.bucket).ok()?;
    let days = days.i32().ok()?;
    let mut tallies: std::collections::BTreeMap<i32, Tally> = std::collections::BTreeMap::new();
    for (row, day) in days.iter().enumerate() {
        let Some(day) = day else { continue };
        match &values {
            Some(values) => match values.get(row) {
                Some(value) if !value.is_nan() => {
                    let tally = tallies.entry(day).or_insert(Tally::EMPTY);
                    tally.rows += 1;
                    tally.add(*value);
                }
                _ => {}
            },
            None => tallies.entry(day).or_insert(Tally::EMPTY).rows += 1,
        }
    }
    Some(
        tallies
            .into_iter()
            .map(|(day, tally)| (day, tally.value(request.aggregate)))
            .collect(),
    )
}

/// The row numbers of one group.
fn group_rows(group: &GroupsIndicator<'_>) -> Vec<usize> {
    match group {
        GroupsIndicator::Idx((_, rows)) => rows.iter().map(|&row| row as usize).collect(),
        GroupsIndicator::Slice([first, len]) => {
            let first = *first as usize;
            (first..first + *len as usize).collect()
        }
    }
}

/// Above [`MAX_GROUPS`]: keep the groups with the most rows (the first ones on a tie) and fold
/// the rest into one tally for "Other".
fn fold_other(groups: &mut Vec<(Key, Tally)>) -> Option<Tally> {
    if groups.len() <= MAX_GROUPS {
        return None;
    }
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| groups[b].1.rows.cmp(&groups[a].1.rows).then(a.cmp(&b)));
    let mut keep = vec![false; groups.len()];
    for &place in order.iter().take(MAX_GROUPS - 1) {
        keep[place] = true;
    }
    let mut other = Tally::EMPTY;
    let mut place = 0;
    groups.retain_mut(|(key, tally)| {
        let kept = keep[place];
        place += 1;
        if !kept {
            other.fold(tally);
            if let Key::Text(text) = key {
                text.zeroize();
            }
        }
        kept
    });
    Some(other)
}

/// Each date of `column` as the first day of its `bucket`, in days since 1970-01-01.
fn bucket_days(column: &Column, bucket: Bucket) -> Result<Column, ChartProblem> {
    let dates = if column.dtype().is_date() {
        column.clone()
    } else {
        column
            .cast(&DataType::Date)
            .map_err(|_| ChartProblem::Columns)?
    };
    let days = dates
        .cast(&DataType::Int32)
        .map_err(|_| ChartProblem::Columns)?;
    let days = days.i32().map_err(|_| ChartProblem::Columns)?;
    let starts: Vec<Option<i32>> = days
        .iter()
        .map(|day| day.map(|day| bucket_start(day, bucket)))
        .collect();
    Ok(Column::new("day".into(), starts))
}

fn bucket_start(day: i32, bucket: Bucket) -> i32 {
    let (year, month, _) = civil_from_days(day);
    match bucket {
        Bucket::Day => day,
        // 1970-01-01 was a Thursday: back to the Monday of the week.
        Bucket::Week => day - (day + 3).rem_euclid(7),
        Bucket::Month => days_from_civil(year, month, 1),
        Bucket::Year => days_from_civil(year, 1, 1),
    }
}

/// `yyyy-mm-dd` (a day, a week's Monday), `yyyy-mm` or `yyyy` of `day`.
fn date_label(day: i32, bucket: Bucket) -> String {
    let (year, month, date) = civil_from_days(day);
    match bucket {
        Bucket::Day | Bucket::Week => format!("{year:04}-{month:02}-{date:02}"),
        Bucket::Month => format!("{year:04}-{month:02}"),
        Bucket::Year => format!("{year:04}"),
    }
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

/// Days since 1970-01-01 of `year`-`month`-`day` (the inverse of [`civil_from_days`]).
fn days_from_civil(year: i64, month: u32, day: u32) -> i32 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i32::try_from(era * 146_097 + doe - 719_468).unwrap_or(0)
}

impl ChartData {
    /// The flat JSON the Swift view decodes: `kind`, `locale`, the axis labels, the legend's
    /// `series`, the date `unit` (or null), `skipped` and the `points`: `{label, value, date,
    /// other}` for bars and lines, `{x, y}` for a scatter. Wiped when dropped.
    pub fn to_json(&self) -> Zeroizing<String> {
        use serde_json::{Map, Value, json};
        let points: Vec<Value> = match &self.points {
            Points::Bars(bars) => bars
                .iter()
                .map(|bar| {
                    json!({
                        "label": bar.label,
                        "value": bar.value,
                        "date": bar.date,
                        "other": bar.other,
                    })
                })
                .collect(),
            Points::Pairs(pairs) => pairs
                .iter()
                .map(|(x, y)| json!({ "x": x, "y": y }))
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
        root.insert(
            "unit".into(),
            self.unit.map_or(Value::Null, |unit| unit.code().into()),
        );
        root.insert("skipped".into(), self.skipped.into());
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
