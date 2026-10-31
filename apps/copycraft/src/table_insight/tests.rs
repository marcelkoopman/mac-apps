//! Fake data only: made-up shops, customers and amounts.

use super::*;
use crate::chart_data::{ChartRequest, build};
use crate::dataframe::parse_table;
use crate::table_profile::profile;
use polars::prelude::DataFrame;

/// 200 rows: amounts 10–14 with 30 outliers (1001–1030, a higher one later), a customer
/// column and a second number column with a value that may never reach the model.
fn amounts_csv(outliers: usize) -> String {
    let mut csv = String::from("customer,amount,secret_code\n");
    for i in 0..200 {
        let amount = if i >= 200 - outliers {
            format!("{}", 1000 + (i - (200 - outliers)) + 1)
        } else {
            format!("{}", 10 + i % 5)
        };
        csv.push_str(&format!("Klant-Geheim-{i},{amount},777777\n"));
    }
    csv
}

fn amount_column(outliers: usize) -> ColumnProfile {
    let df = parse_table(&amounts_csv(outliers)).expect("fake amounts");
    let mut table = profile(&df);
    assert_eq!(explainable(&table), [1]);
    table.columns.swap_remove(1)
}

#[test]
fn twenty_rows_at_most_the_furthest_first() {
    let column = amount_column(30);
    let stats = column.numbers.as_ref().expect("numbers");
    assert_eq!(stats.outliers, 30);
    let rows = outlier_rows(&column);
    assert_eq!(rows.len(), MAX_ROWS);
    // Sorted by deviation: 1030 first, down to 1011.
    let values: Vec<f64> = rows.iter().map(|row| row.value).collect();
    let expected: Vec<f64> = (1011..=1030).rev().map(f64::from).collect();
    assert_eq!(values, expected);
    assert!(rows.iter().all(|row| row.high));
    assert!(values.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn between_five_and_twenty_rows_go_to_the_model() {
    for (outliers, sent) in [(5, 5), (12, 12), (20, 20), (25, 20)] {
        let column = amount_column(outliers);
        assert_eq!(outlier_rows(&column).len(), sent, "{outliers} outliers");
        let prompt = outlier_prompt(&column, Lang::En).expect("prompt");
        assert_eq!(prompt.lines().filter(|l| l.starts_with("- ")).count(), sent);
    }
    // Fewer outliers: only those, nothing is padded.
    let column = amount_column(3);
    assert_eq!(outlier_rows(&column).len(), 3);
}

#[test]
fn the_prompt_holds_statistics_only() {
    let column = amount_column(30);
    for lang in [Lang::Nl, Lang::En] {
        let prompt = outlier_prompt(&column, lang).expect("prompt");
        // No other column: no customer, no other number column's value, no row number.
        assert!(!prompt.contains("Klant"), "{lang:?}");
        assert!(!prompt.contains("Geheim"), "{lang:?}");
        assert!(!prompt.contains("secret_code"), "{lang:?}");
        assert!(!prompt.contains("777"), "{lang:?}");
        assert!(prompt.contains("amount"));
        // Every number in it is a statistic or one of the outlier values.
        let stats = column.numbers.as_ref().expect("numbers");
        let mut known: Vec<f64> = vec![
            stats.count as f64,
            stats.mean,
            stats.median,
            stats.q1,
            stats.q3,
            stats.low_fence(),
            stats.high_fence(),
            stats.outliers as f64,
            MAX_ROWS as f64,
        ];
        for row in outlier_rows(&column) {
            known.push(row.value);
            known.push(row.value / stats.mean);
        }
        for (value, decimals) in numbers_in(&prompt, lang) {
            let slack = 0.5 * 10f64.powi(-(decimals as i32)) + 1e-9;
            assert!(
                known.iter().any(|k| (k.abs() - value).abs() <= slack),
                "{lang:?}: {value} is not a statistic"
            );
        }
    }
    let nl = outlier_prompt(&column, Lang::Nl).expect("prompt");
    let en = outlier_prompt(&column, Lang::En).expect("prompt");
    assert!(nl.contains("Gemiddelde: ") && nl.contains("IQR-grenzen: "));
    assert!(en.contains("Mean: ") && en.contains("IQR bounds: "));
    assert!(en.contains("above the bound"));
}

#[test]
fn the_outlier_sentence_needs_no_model() {
    let column = amount_column(3);
    let stats = column.numbers.as_ref().expect("numbers");
    let en = outlier_sentence(&column, Lang::En).expect("sentence");
    let mean = number(stats.mean, Lang::En);
    assert!(
        en.starts_with("amount: 3 of 200 values fall outside the IQR bounds"),
        "{}",
        en.as_str()
    );
    assert!(en.contains("The furthest is 1003, "));
    assert!(en.contains(&format!("× the mean of {mean}.")));
    let nl = outlier_sentence(&column, Lang::Nl).expect("sentence");
    assert!(nl.contains("3 van 200 waarden vallen buiten de IQR-grenzen"));
    // No outliers, no sentence: a text column.
    let df = parse_table(&amounts_csv(3)).expect("fake amounts");
    let table = profile(&df);
    assert!(outlier_sentence(&table.columns[0], Lang::En).is_none());
    assert!(outlier_prompt(&table.columns[0], Lang::En).is_none());
}

#[test]
fn factors_against_the_mean() {
    assert_eq!(factor(60.0, 10.0, Lang::En).as_deref(), Some("6"));
    assert_eq!(factor(62.0, 10.0, Lang::Nl).as_deref(), Some("6,2"));
    assert_eq!(factor(1.0, 10.0, Lang::En).as_deref(), Some("0.1"));
    assert_eq!(factor(5.0, 0.0, Lang::En), None);
    assert_eq!(factor(5.0, -2.0, Lang::En), None);
}

/// A year of made-up sales, one row on the first of each month.
const MONTHS: [f64; 12] = [
    10.0, 12.0, 15.0, 14.0, 18.0, 22.0, 25.0, 24.0, 30.0, 35.0, 50.0, 40.0,
];

/// A shop, a day and the revenue of that day; the rows not in date order.
fn sales() -> DataFrame {
    let mut csv = String::from("shop,day,revenue\n");
    for (month, value) in MONTHS.iter().enumerate().rev() {
        let shop = if month % 2 == 0 { "Noord" } else { "Zuid" };
        csv.push_str(&format!("{shop},2025-{:02}-01,{value}\n", month + 1));
    }
    parse_table(&csv).expect("fake sales")
}

/// The trend of the chart of `y` against `x` of `df`, as drawn.
fn trend_of(df: &DataFrame, x: usize, y: usize) -> Option<TrendFacts> {
    let chart = build(df, &ChartRequest { x, y }, Lang::En).expect("chart");
    TrendFacts::from_chart(&chart)
}

#[test]
fn trend_facts_of_the_points_by_date() {
    let trend = trend_of(&sales(), 1, 2).expect("trend");
    // In date order, as drawn: every row's value, nothing added up.
    let values: Vec<f64> = trend.points.iter().map(|(_, v)| *v).collect();
    assert_eq!(values, MONTHS);
    assert_eq!(trend.total, 12);
    let shape = trend.shape();
    assert_eq!((shape.peak, shape.low), (10, 0));
    // Two rises of three steps (April–July, August–November): the first wins.
    assert_eq!(shape.run, Some((3, 6, true)));
    assert_eq!(trend.label(10, Lang::Nl), "1 november 2025");
    assert_eq!(trend.label(2, Lang::En), "1 March 2025");
    let en = trend.sentence(Lang::En);
    assert_eq!(
        en.as_str(),
        "Peak on 1 November 2025 (50); lowest point on 1 January 2025 (10). Rose from 10 on \
         1 January 2025 to 40 on 1 December 2025 (+300%). Longest rise: 3 times in a row, from \
         1 April 2025 to 1 July 2025."
    );
    let nl = trend.sentence(Lang::Nl);
    assert!(nl.starts_with("Piek op 1 november 2025 (50); laagste punt op 1 januari 2025 (10)."));
    assert!(nl.contains("Gestegen van 10 op 1 januari 2025 naar 40 op 1 december 2025 (+300%)."));
    let prompt = trend.prompt(Lang::En);
    assert!(prompt.starts_with("Column: revenue\nValues by date, one per row (12):\n"));
    assert!(prompt.contains("- 1 November 2025: 50\n"));
    assert!(prompt.ends_with(en.as_str()));
    let dutch = trend.prompt(Lang::Nl);
    assert!(dutch.starts_with("Kolom: revenue\nWaarden op datum, één per rij (12):\n"));
    // The y column only: no shop.
    assert!(!prompt.contains("Noord") && !prompt.contains("Zuid"));
}

#[test]
fn falls_and_flats() {
    let mut csv = String::from("day,amount\n");
    for (month, value) in [(1, 9.0), (2, 8.0), (3, 7.0), (4, 9.5), (5, 9.0)] {
        csv.push_str(&format!("2026-{month:02}-10,{value}\n"));
    }
    let df = parse_table(&csv).expect("fake");
    let trend = trend_of(&df, 0, 1).expect("trend");
    assert_eq!(trend.shape().run, Some((0, 2, false)));
    let en = trend.sentence(Lang::En);
    assert!(
        en.contains("Unchanged at 9 from 10 January 2026 to 10 May 2026."),
        "{}",
        en.as_str()
    );
    assert!(en.contains("Longest fall: 2 times in a row, from 10 January 2026 to 10 March 2026."));
}

#[test]
fn only_a_chart_over_a_date_column() {
    let df = sales();
    // A text x or a number x: no trend.
    assert!(trend_of(&df, 0, 2).is_none());
    let numbers = parse_table("a,b\n1,2\n3,4\n").expect("fake");
    assert!(trend_of(&numbers, 0, 1).is_none());
    // One point is no trend.
    let one = parse_table("day,amount\n2026-01-01,5\n2026-01-09,\n").expect("fake");
    assert!(trend_of(&one, 0, 1).is_none());
    // Two rows on one day: two points, as drawn.
    let same = parse_table("day,amount\n2026-01-01,5\n2026-01-01,6\n").expect("fake");
    let trend = trend_of(&same, 0, 1).expect("trend");
    assert_eq!(trend.points, [(20_454, 5.0), (20_454, 6.0)]);
}

#[test]
fn beyond_sixty_points_the_last_ones() {
    let mut csv = String::from("day,amount\n");
    // 70 Mondays from 2024-01-01, value = week number.
    let start = 19_723; // 2024-01-01
    for week in 0..70 {
        let (year, month, date) = civil_from_days(start + week * 7);
        csv.push_str(&format!("{year:04}-{month:02}-{date:02},{}\n", week + 1));
    }
    let df = parse_table(&csv).expect("fake");
    let trend = trend_of(&df, 0, 1).expect("trend");
    assert_eq!((trend.points.len(), trend.total), (MAX_TREND_POINTS, 70));
    assert_eq!(trend.points[0].1, 11.0);
    assert_eq!(trend.label(0, Lang::En), "11 March 2024");
    let prompt = trend.prompt(Lang::Nl);
    assert!(prompt.contains("Alleen de laatste 60 van 70 punten."));
    assert!(prompt.contains("- 11 maart 2024: 11\n"));
    // A steady rise: one run over every step.
    assert_eq!(trend.shape().run, Some((0, 59, true)));
}

#[test]
fn a_sampled_chart_gives_the_sample() {
    let mut csv = String::from("day,amount\n");
    for row in 0..6_000 {
        let (year, month, date) = civil_from_days(row);
        csv.push_str(&format!("{year:04}-{month:02}-{date:02},{row}\n"));
    }
    let df = parse_table(&csv).expect("fake");
    let trend = trend_of(&df, 0, 1).expect("trend");
    // The points drawn (5 000 of 6 000), of them the last 60.
    assert_eq!(trend.total, crate::chart_data::MAX_POINTS);
    let last = trend.points.last().expect("points");
    assert_eq!(*last, (5_998, 5_998.0));
}

#[test]
fn numbers_are_read_the_language_s_way() {
    assert_eq!(
        numbers_in("Q1 12,5 en 1.234,56 of 7×; -3", Lang::Nl),
        [(12.5, 1), (1234.56, 2), (7.0, 0), (3.0, 0)]
    );
    assert_eq!(
        numbers_in("Q3 12.5 and 1,234.56, 2026.", Lang::En),
        [(12.5, 1), (1234.56, 2), (2026.0, 0)]
    );
    // An English decimal in a Dutch answer reads as two numbers: it will not fit.
    assert_eq!(numbers_in("1.5", Lang::Nl), [(1.0, 0), (5.0, 0)]);
    assert!(numbers_in("geen getallen", Lang::Nl).is_empty());
}

#[test]
fn the_answer_must_use_the_facts_numbers() {
    let column = amount_column(30);
    let facts = outlier_prompt(&column, Lang::Nl).expect("prompt");
    let stats = column.numbers.as_ref().expect("numbers");
    let times = factor(1030.0, stats.mean, Lang::Nl).expect("factor");
    // A fake model answer with the facts' numbers (one rounded) passes, cleaned up.
    let good = format!(
        "  \"Deze 30 waarden liggen boven de IQR-grens; de hoogste, 1030, is {times}× het \
         gemiddelde.\"  "
    );
    let checked = check_answer(&good, &facts, Lang::Nl).expect("fits");
    assert!(checked.starts_with("Deze 30 waarden"));
    assert!(!checked.contains('"'));
    let rounded = "De hoogste is ongeveer 1.030 en Q1 ligt laag.";
    assert!(check_answer(rounded, &facts, Lang::Nl).is_some());
    // A number of its own: only the Rust sentence is shown.
    let made_up = "Deze transacties zijn 99× het gemiddelde.";
    assert!(check_answer(made_up, &facts, Lang::Nl).is_none());
    let wrong_value = "De hoogste waarde is 1040.";
    assert!(check_answer(wrong_value, &facts, Lang::Nl).is_none());
    // Empty or far too long: not shown.
    assert!(check_answer("  ", &facts, Lang::Nl).is_none());
    let long = "Deze waarden vallen op. ".repeat(40);
    assert!(check_answer(&long, &facts, Lang::Nl).is_none());
}

#[test]
fn a_trend_answer_is_checked_too() {
    let trend = trend_of(&sales(), 1, 2).expect("trend");
    let facts = trend.prompt(Lang::En);
    let good = "Revenue has grown since 1 March 2025, with a peak of 50 in November.";
    assert_eq!(
        check_answer(good, &facts, Lang::En)
            .as_deref()
            .map(String::as_str),
        Some(good)
    );
    let growth = "Revenue grew by 300% over the year.";
    assert!(check_answer(growth, &facts, Lang::En).is_some());
    let wrong = "Revenue peaked at 55 in November.";
    assert!(check_answer(wrong, &facts, Lang::En).is_none());
    let wrong_pct = "Revenue grew by 250% over the year.";
    assert!(check_answer(wrong_pct, &facts, Lang::En).is_none());
}

#[test]
fn debug_shows_no_name_or_value() {
    let trend = trend_of(&sales(), 1, 2).expect("trend");
    let text = format!("{trend:?}");
    assert!(!text.contains("revenue") && !text.contains("50"));
    assert_eq!(model_language(Lang::Nl), mac_ai::Language::Dutch);
    assert_eq!(model_language(Lang::En), mac_ai::Language::English);
}

#[test]
fn without_the_model_the_run_fails_cleanly() {
    let job = mac_ai::Job::new();
    let column = amount_column(5);
    let facts = outlier_prompt(&column, Lang::En).expect("prompt");
    let outcome = InsightOutcome {
        run: 7,
        result: run_and_check(&job, mac_ai::FactTask::ExplainOutliers, &facts, Lang::En),
    };
    // Off a Mac (and in a build without the bridge) the model is not there.
    #[cfg(not(target_os = "macos"))]
    assert!(matches!(
        outcome.result,
        InsightResult::Failed(mac_ai::Error::Unavailable)
    ));
    let debug = format!("{outcome:?}");
    assert!(debug.contains("run: 7"));
    assert!(!debug.contains("amount"));
    let text = InsightText(Zeroizing::new(String::from("Fake answer about amount")));
    assert_eq!(format!("{text:?}"), "InsightText { chars: 24 }");
}
