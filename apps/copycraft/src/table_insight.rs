//! Explain (outliers, in the table profile) and Summarize (a trend, in the chart): the facts
//! Rust works out, the prompt for the Apple model on the device (`mac_ai::Job::run_facts`), and
//! the check of its answer.
//!
//! The model never sees a table cell. It gets the column's statistics and the values of at
//! most [`MAX_ROWS`] outliers (no other column of those rows), or the points of a chart over
//! a date column (date and value, as drawn) with fixed facts about them. Every number in its answer must be one of those numbers
//! ([`check_answer`]); otherwise only the Rust fact sentence is shown. Nothing here is logged
//! or stored: prompts and answers are wiped when dropped.

use zeroize::{Zeroize, Zeroizing};

use crate::chart_data::{ChartData, ChartKind, Points, civil_from_days};
use crate::locale::{Lang, t_in, tf_in};
use crate::table_profile::{ColumnProfile, Extreme, TableProfile, decimal_mark, number, whole};

/// The most outlier rows that go to the model (the furthest past the fence first).
pub const MAX_ROWS: usize = 20;
/// An answer longer than this, in characters, is no "1 or 2 sentences" and is not shown.
pub const MAX_ANSWER_CHARS: usize = 600;

/// The model's language for the app's.
pub fn model_language(lang: Lang) -> mac_ai::Language {
    match lang {
        Lang::Nl => mac_ai::Language::Dutch,
        Lang::En => mac_ai::Language::English,
    }
}

// ---------------------------------------------------------------------------------------------
// Outliers

/// The places in `profile.columns` of the number columns with outliers: the ones Explain offers.
pub fn explainable(profile: &TableProfile) -> Vec<usize> {
    profile
        .columns
        .iter()
        .enumerate()
        .filter(|(_, column)| {
            column
                .numbers
                .as_ref()
                .is_some_and(|stats| stats.outliers > 0 && !stats.extremes.is_empty())
        })
        .map(|(place, _)| place)
        .collect()
}

/// The outlier rows that go to the model: at most [`MAX_ROWS`], the furthest past the IQR
/// fence first (as the profile keeps them). Fewer when the column has fewer outliers.
pub fn outlier_rows(column: &ColumnProfile) -> &[Extreme] {
    match &column.numbers {
        Some(stats) => &stats.extremes[..stats.extremes.len().min(MAX_ROWS)],
        None => &[],
    }
}

/// `value` as a multiple of `mean` with one decimal (`6,2` / `6.2`), when the mean is above 0.
fn factor(value: f64, mean: f64, lang: Lang) -> Option<String> {
    if mean <= 0.0 || !mean.is_finite() {
        return None;
    }
    let factor = value / mean;
    if !factor.is_finite() {
        return None;
    }
    let text = format!("{factor:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    Some(decimal_mark(if text == "-0" { "0" } else { text }, lang))
}

/// The facts the model gets to explain the outliers of `column`: its name and statistics, and
/// per outlier row the value, the factor against the mean and the side. No other cell.
pub fn outlier_prompt(column: &ColumnProfile, lang: Lang) -> Option<Zeroizing<String>> {
    let stats = column.numbers.as_ref()?;
    let rows = outlier_rows(column);
    if rows.is_empty() {
        return None;
    }
    let n = |value: f64| number(value, lang);
    let mut lines = vec![
        tf_in(lang, "insight_p_column", &[&column.name]),
        tf_in(lang, "insight_p_count", &[&whole(stats.count, lang)]),
        tf_in(lang, "insight_p_mean", &[&n(stats.mean)]),
        tf_in(lang, "insight_p_median", &[&n(stats.median)]),
        tf_in(lang, "insight_p_quartiles", &[&n(stats.q1), &n(stats.q3)]),
        tf_in(
            lang,
            "insight_p_fences",
            &[&n(stats.low_fence()), &n(stats.high_fence())],
        ),
        tf_in(lang, "insight_p_outliers", &[&whole(stats.outliers, lang)]),
        tf_in(lang, "insight_p_rows", &[&rows.len()]),
    ];
    for row in rows {
        let side = if row.high {
            t_in(lang, "insight_p_above")
        } else {
            t_in(lang, "insight_p_below")
        };
        lines.push(match factor(row.value, stats.mean, lang) {
            Some(times) => format!("- {}, {times}×, {side}", n(row.value)),
            None => format!("- {}, {side}", n(row.value)),
        });
    }
    let text = Zeroizing::new(lines.join("\n"));
    lines.zeroize();
    Some(text)
}

/// The Rust fact sentence of `column`'s outliers, shown always (and alone when the model's
/// answer is not shown).
pub fn outlier_sentence(column: &ColumnProfile, lang: Lang) -> Option<Zeroizing<String>> {
    let stats = column.numbers.as_ref()?;
    let furthest = outlier_rows(column).first()?;
    let n = |value: f64| number(value, lang);
    let first = tf_in(
        lang,
        "insight_out_sentence",
        &[
            &column.name,
            &whole(stats.outliers, lang),
            &whole(stats.count, lang),
            &n(stats.low_fence()),
            &n(stats.high_fence()),
        ],
    );
    let second = match factor(furthest.value, stats.mean, lang) {
        Some(times) => tf_in(
            lang,
            "insight_out_furthest",
            &[&n(furthest.value), &times, &n(stats.mean)],
        ),
        None => tf_in(
            lang,
            "insight_out_furthest_plain",
            &[&n(furthest.value), &n(stats.mean)],
        ),
    };
    Some(Zeroizing::new(format!("{first} {second}")))
}

// ---------------------------------------------------------------------------------------------
// Trend

/// The points of a chart over a date column, for Summarize: the values by date as drawn (one
/// per row, at most the chart's window of [`crate::chart_data::WINDOW`] rows), with the y
/// column's name. Wiped when dropped; its
/// `Debug` shows no name or value.
pub struct TrendFacts {
    /// The y column's name.
    pub column: String,
    /// `(day, value)`, oldest first.
    pub points: Vec<(i32, f64)>,
}

impl std::fmt::Debug for TrendFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrendFacts")
            .field("points", &self.points.len())
            .finish_non_exhaustive()
    }
}

impl Drop for TrendFacts {
    fn drop(&mut self) {
        self.column.zeroize();
        for point in self.points.iter_mut() {
            *point = (0, 0.0);
        }
    }
}

/// Where the series peaks and dips, how it went from start to end, and its longest run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrendShape {
    /// Places in [`TrendFacts::points`]: the first on a tie.
    pub peak: usize,
    pub low: usize,
    /// The longest run of steps that each go up (`true`) or each go down, as
    /// `(first place, last place, up)`; at least two steps. A rise wins a tie.
    pub run: Option<(usize, usize, bool)>,
}

impl TrendFacts {
    /// The trend of a drawn chart: only one over a date column, with at least two points
    /// (in date order, as drawn: the window's rows).
    pub fn from_chart(chart: &ChartData) -> Option<TrendFacts> {
        let Points::Pairs(pairs) = &chart.points else {
            return None;
        };
        if chart.kind != ChartKind::Dates || pairs.len() < 2 {
            return None;
        }
        let points = pairs
            .iter()
            .map(|(day, value)| (day.floor() as i32, *value))
            .collect();
        Some(TrendFacts {
            column: chart.y_label.clone(),
            points,
        })
    }

    pub fn shape(&self) -> TrendShape {
        let mut peak = 0;
        let mut low = 0;
        for (place, (_, value)) in self.points.iter().enumerate() {
            if *value > self.points[peak].1 {
                peak = place;
            }
            if *value < self.points[low].1 {
                low = place;
            }
        }
        let longest = |up: bool| -> Option<(usize, usize)> {
            let mut best: Option<(usize, usize)> = None;
            let mut start = 0;
            for place in 1..self.points.len() {
                let (before, now) = (self.points[place - 1].1, self.points[place].1);
                let goes = if up { now > before } else { now < before };
                if !goes {
                    start = place;
                    continue;
                }
                if best.is_none_or(|(a, b)| place - start > b - a) {
                    best = Some((start, place));
                }
            }
            best.filter(|(a, b)| b - a >= 2)
        };
        let run = match (longest(true), longest(false)) {
            (Some(rise), Some(fall)) if fall.1 - fall.0 > rise.1 - rise.0 => {
                Some((fall.0, fall.1, false))
            }
            (Some(rise), _) => Some((rise.0, rise.1, true)),
            (None, Some(fall)) => Some((fall.0, fall.1, false)),
            (None, None) => None,
        };
        TrendShape { peak, low, run }
    }

    /// The label of point `place`: its day, "2 maart 2026" / "2 March 2026".
    pub fn label(&self, place: usize, lang: Lang) -> String {
        let day = self.points.get(place).map_or(0, |(day, _)| *day);
        let (year, month, date) = civil_from_days(day);
        format!("{date} {} {year}", month_name(month, lang))
    }

    /// The Rust fact sentences: peak and lowest point, start to end, the longest run. Shown
    /// always, and passed to the model as fixed facts.
    pub fn sentence(&self, lang: Lang) -> Zeroizing<String> {
        let shape = self.shape();
        let n = |place: usize| number(self.points[place].1, lang);
        let label = |place: usize| self.label(place, lang);
        let mut parts = vec![tf_in(
            lang,
            "insight_trend_peak",
            &[
                &label(shape.peak),
                &n(shape.peak),
                &label(shape.low),
                &n(shape.low),
            ],
        )];
        let last = self.points.len() - 1;
        let (first_value, last_value) = (self.points[0].1, self.points[last].1);
        let change = if first_value != 0.0 {
            let share = (last_value - first_value) / first_value.abs() * 100.0;
            let text = if share.abs() >= 10.0 {
                format!("{share:+.0}")
            } else {
                format!("{share:+.1}")
            };
            format!(" ({}%)", decimal_mark(&text, lang))
        } else {
            String::new()
        };
        let key_args: [&dyn std::fmt::Display; 5] =
            [&label(0), &n(0), &label(last), &n(last), &change];
        parts.push(if last_value > first_value {
            tf_in(lang, "insight_trend_up", &key_args)
        } else if last_value < first_value {
            tf_in(lang, "insight_trend_down", &key_args)
        } else {
            tf_in(lang, "insight_trend_flat", &key_args)
        });
        if let Some((from, to, up)) = shape.run {
            let args: [&dyn std::fmt::Display; 3] = [&(to - from), &label(from), &label(to)];
            parts.push(if up {
                tf_in(lang, "insight_trend_rise", &args)
            } else {
                tf_in(lang, "insight_trend_fall", &args)
            });
        }
        let text = Zeroizing::new(parts.join(" "));
        parts.zeroize();
        text
    }

    /// The facts the model gets: the y column's name, its points as "label: value", and the fixed
    /// facts of [`TrendFacts::sentence`].
    pub fn prompt(&self, lang: Lang) -> Zeroizing<String> {
        let mut lines = vec![tf_in(lang, "insight_p_series", &[&self.column])];
        let count = self.points.len();
        lines.push(tf_in(lang, "insight_p_by_date", &[&count]));
        for (place, (_, value)) in self.points.iter().enumerate() {
            lines.push(format!(
                "- {}: {}",
                self.label(place, lang),
                number(*value, lang)
            ));
        }
        lines.push(t_in(lang, "insight_p_facts").to_string());
        lines.push(self.sentence(lang).to_string());
        let text = Zeroizing::new(lines.join("\n"));
        lines.zeroize();
        text
    }
}

fn month_name(month: u32, lang: Lang) -> &'static str {
    match month {
        1 => t_in(lang, "month_1"),
        2 => t_in(lang, "month_2"),
        3 => t_in(lang, "month_3"),
        4 => t_in(lang, "month_4"),
        5 => t_in(lang, "month_5"),
        6 => t_in(lang, "month_6"),
        7 => t_in(lang, "month_7"),
        8 => t_in(lang, "month_8"),
        9 => t_in(lang, "month_9"),
        10 => t_in(lang, "month_10"),
        11 => t_in(lang, "month_11"),
        _ => t_in(lang, "month_12"),
    }
}

// ---------------------------------------------------------------------------------------------
// Runs

/// What a model run gave: an answer that passed [`check_answer`], an answer left out (its
/// numbers did not fit), or why there is none. Holds no prompt.
#[derive(Clone)]
pub enum InsightResult {
    Answer(Zeroizing<String>),
    LeftOut,
    Failed(mac_ai::Error),
}

/// A model run's result for the table window's run number `run` (an older run is dropped).
/// Its `Debug` shows no answer.
#[derive(Clone)]
pub struct InsightOutcome {
    pub run: u64,
    pub result: InsightResult,
}

impl std::fmt::Debug for InsightOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match &self.result {
            InsightResult::Answer(_) => "answer",
            InsightResult::LeftOut => "left-out",
            InsightResult::Failed(error) => error.kind(),
        };
        f.debug_struct("InsightOutcome")
            .field("run", &self.run)
            .field("result", &result)
            .finish()
    }
}

/// Run `task` on `facts` with the model and check its answer against them. Blocks: call it on
/// a worker thread. Nothing is logged; the raw answer is wiped.
pub fn run_and_check(
    job: &mac_ai::Job,
    task: mac_ai::FactTask,
    facts: &str,
    lang: Lang,
) -> InsightResult {
    match job.run_facts(task, model_language(lang), facts) {
        Ok(mut answer) => {
            let checked = check_answer(&answer, facts, lang);
            answer.zeroize();
            match checked {
                Some(text) => InsightResult::Answer(text),
                None => InsightResult::LeftOut,
            }
        }
        Err(error) => InsightResult::Failed(error),
    }
}

/// The text of *Copy* under an explanation, on its way to the app's copy. Wiped when
/// dropped; its `Debug` shows only its length.
#[derive(Clone)]
pub struct InsightText(pub Zeroizing<String>);

impl std::fmt::Debug for InsightText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InsightText")
            .field("chars", &self.0.chars().count())
            .finish()
    }
}

// ---------------------------------------------------------------------------------------------
// The check

/// The numbers written in `text` the way `lang` writes them (NL `1.234,5`, EN `1,234.5`), as
/// `(absolute value, decimals)`. Digits right after a letter (`Q1`, `H2O`) are part of a name,
/// not a number; a sign is ignored (values are compared without it).
pub fn numbers_in(text: &str, lang: Lang) -> Vec<(f64, u32)> {
    let (group, mark) = match lang {
        Lang::Nl => ('.', ','),
        Lang::En => (',', '.'),
    };
    let chars: Vec<char> = text.chars().collect();
    let digit = |at: usize| chars.get(at).is_some_and(|c| c.is_ascii_digit());
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if !digit(at) {
            at += 1;
            continue;
        }
        let glued = at > 0 && chars[at - 1].is_alphabetic();
        let mut whole_part = String::new();
        loop {
            while digit(at) {
                whole_part.push(chars[at]);
                at += 1;
            }
            // A thousands mark: exactly three digits follow it.
            if chars.get(at) == Some(&group)
                && digit(at + 1)
                && digit(at + 2)
                && digit(at + 3)
                && !digit(at + 4)
            {
                at += 1;
                continue;
            }
            break;
        }
        let mut fraction = String::new();
        if chars.get(at) == Some(&mark) && digit(at + 1) {
            at += 1;
            while digit(at) {
                fraction.push(chars[at]);
                at += 1;
            }
        }
        if glued {
            continue;
        }
        let written = if fraction.is_empty() {
            whole_part.clone()
        } else {
            format!("{whole_part}.{fraction}")
        };
        if let Ok(value) = written.parse::<f64>() {
            found.push((value, fraction.len() as u32));
        }
    }
    found
}

/// `answer` cleaned up, when every number in it is a number of `facts` (allowing the rounding
/// its own decimals imply: `6` fits `6,2`); `None` when one is not, or the answer is empty or
/// too long. Then only the Rust fact sentence is shown.
pub fn check_answer(answer: &str, facts: &str, lang: Lang) -> Option<Zeroizing<String>> {
    let trimmed = answer
        .trim()
        .trim_matches(|c| matches!(c, '"' | '“' | '”' | '\'' | '«' | '»'))
        .trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_ANSWER_CHARS {
        return None;
    }
    let allowed = numbers_in(facts, lang);
    let fits = |(value, decimals): (f64, u32)| {
        let slack = 0.5 * 10f64.powi(-(decimals as i32)) + 1e-9;
        allowed
            .iter()
            .any(|(fact, _)| (value - fact).abs() <= slack)
    };
    if numbers_in(trimmed, lang).into_iter().all(fits) {
        Some(Zeroizing::new(
            trimmed.split_whitespace().collect::<Vec<_>>().join(" "),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
