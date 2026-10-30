//! Fake tables only: revenue per category and month of a made-up shop.

use super::*;
use crate::dataframe::parse_table;

const SHOP: &str = "\
category,month,revenue,units
Books,2026-01-05,10.5,1
Games,2026-01-20,20,2
Books,2026-02-03,4.5,3
Toys,2026-02-14,7,1
Games,2026-02-28,30,4
Books,2026-03-01,5,2
";

fn shop() -> DataFrame {
    parse_table(SHOP).expect("fake shop table")
}

fn bars(data: &ChartData) -> Vec<(String, f64)> {
    match &data.points {
        Points::Bars(bars) => bars.iter().map(|b| (b.label.clone(), b.value)).collect(),
        Points::Pairs(_) => panic!("expected bars"),
    }
}

fn request(kind: ChartKind, x: usize, y: Option<usize>, aggregate: Aggregate) -> ChartRequest {
    ChartRequest {
        kind,
        x,
        y,
        aggregate,
        bucket: Bucket::Month,
    }
}

#[test]
fn columns_get_their_roles() {
    let roles: Vec<ColumnRole> = chart_columns(&shop()).iter().map(|c| c.role).collect();
    assert_eq!(
        roles,
        [
            ColumnRole::Text,
            ColumnRole::Date,
            ColumnRole::Number,
            ColumnRole::Number
        ]
    );
    let first = ChartRequest::first(&chart_columns(&shop())).expect("a chart");
    assert_eq!(first, request(ChartKind::Bar, 0, Some(2), Aggregate::Sum));
}

#[test]
fn sum_per_category_largest_first() {
    let data = build(
        &shop(),
        &request(ChartKind::Bar, 0, Some(2), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(
        bars(&data),
        [
            ("Games".to_string(), 50.0),
            ("Books".to_string(), 20.0),
            ("Toys".to_string(), 7.0)
        ]
    );
    assert_eq!(data.series, "Sum of revenue");
    assert_eq!(data.skipped, 0);
}

#[test]
fn mean_and_count_per_category() {
    let mean = build(
        &shop(),
        &request(ChartKind::Line, 0, Some(2), Aggregate::Mean),
        Lang::En,
    )
    .expect("chart");
    // A line keeps the order the groups come in.
    assert_eq!(
        bars(&mean),
        [
            ("Books".to_string(), 20.0 / 3.0),
            ("Games".to_string(), 25.0),
            ("Toys".to_string(), 7.0)
        ]
    );
    let count = build(
        &shop(),
        &request(ChartKind::Bar, 0, None, Aggregate::Count),
        Lang::Nl,
    )
    .expect("chart");
    assert_eq!(
        bars(&count),
        [
            ("Books".to_string(), 3.0),
            ("Games".to_string(), 2.0),
            ("Toys".to_string(), 1.0)
        ]
    );
    assert_eq!(count.y_label, "Aantal");
    let min = build(
        &shop(),
        &request(ChartKind::Line, 0, Some(3), Aggregate::Min),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(bars(&min)[0], ("Books".to_string(), 1.0));
    let max = build(
        &shop(),
        &request(ChartKind::Line, 0, Some(3), Aggregate::Max),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(bars(&max)[1], ("Games".to_string(), 4.0));
}

#[test]
fn dates_group_per_month_and_year() {
    let df = shop();
    assert_eq!(df.columns()[1].dtype(), &DataType::Date);
    let month = build(
        &df,
        &request(ChartKind::Bar, 1, Some(2), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(
        bars(&month),
        [
            ("2026-01".to_string(), 30.5),
            ("2026-02".to_string(), 41.5),
            ("2026-03".to_string(), 5.0)
        ]
    );
    assert_eq!(month.unit, Some(Bucket::Month));
    let Points::Bars(points) = &month.points else {
        panic!("bars");
    };
    assert_eq!(points[1].date.as_deref(), Some("2026-02-01"));
    let mut by_year = request(ChartKind::Bar, 1, Some(2), Aggregate::Sum);
    by_year.bucket = Bucket::Year;
    let year = build(&df, &by_year, Lang::En).expect("chart");
    assert_eq!(bars(&year), [("2026".to_string(), 77.0)]);
}

#[test]
fn civil_dates_round_trip() {
    for day in [-800_000, -1, 0, 59, 365, 19_782, 20_454, 2_000_000] {
        let (year, month, date) = civil_from_days(day);
        assert_eq!(days_from_civil(year, month, date), day);
    }
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(
        date_label(days_from_civil(2024, 2, 29), Bucket::Day),
        "2024-02-29"
    );
    assert_eq!(
        bucket_start(days_from_civil(2026, 3, 31), Bucket::Month),
        days_from_civil(2026, 3, 1)
    );
}

#[test]
fn groups_beyond_fifty_go_to_other() {
    let mut csv = String::from("shop,sales\n");
    // 60 shops; shop 0 has three rows, the others one.
    for row in 0..60 {
        csv.push_str(&format!("shop{row},{row}\n"));
    }
    csv.push_str("shop0,100\nshop0,200\n");
    let df = parse_table(&csv).expect("fake table");
    let data = build(
        &df,
        &request(ChartKind::Line, 0, Some(1), Aggregate::Sum),
        Lang::Nl,
    )
    .expect("chart");
    let points = bars(&data);
    assert_eq!(points.len(), MAX_GROUPS);
    assert_eq!(points[0], ("shop0".to_string(), 300.0));
    // Kept: shop0 to shop48 (the most rows, then the first ones). Other: shop49 to shop59.
    assert_eq!(points[48].0, "shop48");
    let other: f64 = (49..60).map(f64::from).sum();
    assert_eq!(points[49], ("Overig".to_string(), other));
    let Points::Bars(raw) = &data.points else {
        panic!("bars");
    };
    assert!(raw[49].other && !raw[48].other);
    // The mean of Other is over all its rows, not a mean of means.
    let mean = build(
        &df,
        &request(ChartKind::Line, 0, Some(1), Aggregate::Mean),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(bars(&mean)[49], ("Other".to_string(), other / 11.0));
}

#[test]
fn scatter_samples_evenly() {
    let mut csv = String::from("x,y\n");
    for row in 0..5000 {
        csv.push_str(&format!("{row},{}\n", row * 2));
    }
    let df = parse_table(&csv).expect("fake table");
    let data = build(
        &df,
        &request(ChartKind::Scatter, 0, Some(1), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    let Points::Pairs(pairs) = &data.points else {
        panic!("pairs");
    };
    assert_eq!(pairs.len(), MAX_POINTS);
    assert_eq!(pairs[0], (0.0, 0.0));
    assert_eq!(pairs[1], (2.0, 4.0));
    assert_eq!(pairs[1999], (4997.0, 9994.0));
    assert_eq!(data.skipped, 0);
    assert_eq!(data.series, "y vs. x");
}

#[test]
fn empty_and_non_numeric_values_are_skipped() {
    let numbers_only = "\
category,revenue
Books,10
,5
Games,
Books,
Games,7
Toys,2
";
    let df = parse_table(numbers_only).expect("fake table");
    let data = build(
        &df,
        &request(ChartKind::Bar, 0, Some(1), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(
        bars(&data),
        [
            ("Books".to_string(), 10.0),
            ("Games".to_string(), 7.0),
            ("Toys".to_string(), 2.0)
        ]
    );
    // The row without a category and the two without revenue.
    assert_eq!(data.skipped, 3);
    let count = build(
        &df,
        &request(ChartKind::Bar, 0, Some(1), Aggregate::Count),
        Lang::En,
    )
    .expect("chart");
    // Counting needs no value: only the row without a category is skipped.
    assert_eq!(count.skipped, 1);
    assert_eq!(bars(&count)[0], ("Books".to_string(), 2.0));
}

#[test]
fn wrong_columns_and_no_values() {
    let df = shop();
    let text_as_value = request(ChartKind::Bar, 0, Some(0), Aggregate::Sum);
    assert_eq!(
        build(&df, &text_as_value, Lang::En),
        Err(ChartProblem::Columns)
    );
    let number_as_group = request(ChartKind::Bar, 2, Some(3), Aggregate::Sum);
    assert_eq!(
        build(&df, &number_as_group, Lang::En),
        Err(ChartProblem::Columns)
    );
    let missing = request(ChartKind::Bar, 9, Some(2), Aggregate::Sum);
    assert_eq!(build(&df, &missing, Lang::En), Err(ChartProblem::Columns));
    let scatter_without_y = request(ChartKind::Scatter, 2, None, Aggregate::Sum);
    assert_eq!(
        build(&df, &scatter_without_y, Lang::En),
        Err(ChartProblem::Columns)
    );
    let empty =
        parse_table("category,revenue,units\n,1,1\nBooks,,2\nGames,,3\n").expect("fake table");
    let sum = request(ChartKind::Bar, 0, Some(1), Aggregate::Sum);
    assert_eq!(build(&empty, &sum, Lang::En), Err(ChartProblem::NoValues));
}

#[test]
fn json_snapshot() {
    let data = build(
        &shop(),
        &request(ChartKind::Bar, 1, Some(2), Aggregate::Sum),
        Lang::Nl,
    )
    .expect("chart");
    assert_eq!(
        data.to_json().as_str(),
        concat!(
            r#"{"kind":"bar","locale":"nl","x_label":"month","y_label":"revenue","#,
            r#""series":"Som van revenue","unit":"month","skipped":0,"points":["#,
            r#"{"label":"2026-01","value":30.5,"date":"2026-01-01","other":false},"#,
            r#"{"label":"2026-02","value":41.5,"date":"2026-02-01","other":false},"#,
            r#"{"label":"2026-03","value":5.0,"date":"2026-03-01","other":false}]}"#
        )
    );
    let pairs = parse_table("a,b\n1,2\n3,\n").expect("fake table");
    let scatter = build(
        &pairs,
        &request(ChartKind::Scatter, 0, Some(1), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(
        scatter.to_json().as_str(),
        concat!(
            r#"{"kind":"scatter","locale":"en","x_label":"a","y_label":"b","#,
            r#""series":"b vs. a","unit":null,"skipped":1,"points":[{"x":1.0,"y":2.0}]}"#
        )
    );
}

#[test]
fn the_pop_ups_offer_what_fits() {
    let columns = chart_columns(&shop());
    assert_eq!(
        kinds(&columns),
        [ChartKind::Bar, ChartKind::Line, ChartKind::Scatter]
    );
    assert_eq!(x_choices(&columns, ChartKind::Bar), [0, 1]);
    assert_eq!(x_choices(&columns, ChartKind::Scatter), [2, 3]);
    assert_eq!(y_choices(&columns), [2, 3]);
    assert_eq!(role_of(&columns, 1), Some(ColumnRole::Date));
    assert_eq!(role_of(&columns, 9), None);
    // One number column: no scatter.
    let one = chart_columns(&parse_table("category,revenue\nBooks,1\nGames,2\n").expect("table"));
    assert_eq!(kinds(&one), [ChartKind::Bar, ChartKind::Line]);
}

#[test]
fn a_request_is_fitted_to_the_columns() {
    let columns = chart_columns(&shop());
    // Bar to scatter: the x becomes a number column, the y another one.
    let scatter = request(ChartKind::Scatter, 0, Some(2), Aggregate::Sum)
        .fitted(&columns)
        .expect("fits");
    assert_eq!((scatter.x, scatter.y), (2, Some(3)));
    // Back to a bar: a text column again, the value column kept.
    let bar = request(ChartKind::Bar, 2, Some(3), Aggregate::Mean)
        .fitted(&columns)
        .expect("fits");
    assert_eq!((bar.x, bar.y, bar.aggregate), (0, Some(3), Aggregate::Mean));
    // A request that fits stays as it is.
    let line = request(ChartKind::Line, 1, Some(2), Aggregate::Max);
    assert_eq!(line.fitted(&columns), Some(line));
    // Without number columns: counting; no columns at all: nothing.
    let text = chart_columns(&parse_table("category,shop\nBooks,A\nGames,B\n").expect("table"));
    let counted = request(ChartKind::Bar, 0, Some(2), Aggregate::Sum)
        .fitted(&text)
        .expect("fits");
    assert_eq!((counted.y, counted.aggregate), (None, Aggregate::Count));
    assert_eq!(
        request(ChartKind::Scatter, 0, None, Aggregate::Sum).fitted(&text),
        Some(counted)
    );
    assert_eq!(
        request(ChartKind::Bar, 0, None, Aggregate::Sum).fitted(&[]),
        None
    );
}

#[test]
fn an_outcome_shows_no_content() {
    let outcome = ChartOutcome::build(
        &shop(),
        &request(ChartKind::Bar, 0, Some(2), Aggregate::Sum),
        Lang::En,
        7,
    );
    let (json, skipped) = outcome.result.as_ref().expect("chart");
    assert!(json.contains("Books"));
    assert_eq!(*skipped, 0);
    let shown = format!("{outcome:?}");
    assert!(
        !shown.contains("Books") && !shown.contains("revenue"),
        "{shown}"
    );
    assert!(shown.contains('7'));
    let copy = outcome.clone();
    assert_eq!(copy.generation, 7);
    let problem = ChartOutcome::build(
        &shop(),
        &request(ChartKind::Bar, 2, Some(0), Aggregate::Sum),
        Lang::En,
        8,
    );
    assert!(problem.result.is_err());
}
