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
    // The average by default, not the sum: a sum per group runs far past the column's values.
    assert_eq!(first, request(ChartKind::Bar, 0, Some(2), Aggregate::Mean));
    // Without a number column: the row count.
    let text = parse_table("city,name\nUtrecht,Anja\nGouda,Jan\n").expect("fake table");
    let counted = ChartRequest::first(&chart_columns(&text)).expect("a chart");
    assert_eq!(counted.aggregate, Aggregate::Count);
    assert_eq!(counted.y, None);
}

#[test]
fn the_value_axis_says_what_it_measures() {
    let ages = parse_table("city,age\nUtrecht,40\nUtrecht,30\nGouda,50\n").expect("fake table");
    let first = ChartRequest::first(&chart_columns(&ages)).expect("a chart");
    let mean = build(&ages, &first, Lang::Nl).expect("chart");
    assert_eq!(mean.y_label, "Gemiddelde van age");
    assert_eq!(mean.series, mean.y_label);
    assert_eq!(
        bars(&mean),
        [("Gouda".to_string(), 50.0), ("Utrecht".to_string(), 35.0)]
    );
    let sum = build(
        &ages,
        &request(ChartKind::Bar, 0, Some(1), Aggregate::Sum),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(sum.y_label, "Sum of age");
    let count = build(
        &ages,
        &request(ChartKind::Bar, 0, None, Aggregate::Count),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(count.y_label, "Count");
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
fn weeks_start_on_monday() {
    // 2026-03-02 is a Monday; 2026-03-08 the Sunday of that week.
    let monday = days_from_civil(2026, 3, 2);
    for day in monday..monday + 7 {
        assert_eq!(bucket_start(day, Bucket::Week), monday);
    }
    assert_eq!(bucket_start(monday - 1, Bucket::Week), monday - 7);
    // Before 1970 too: 1969-12-29 was a Monday.
    assert_eq!(
        bucket_start(-1, Bucket::Week),
        days_from_civil(1969, 12, 29)
    );
    assert_eq!(date_label(monday, Bucket::Week), "2026-03-02");
    let mut by_week = request(ChartKind::Line, 1, Some(2), Aggregate::Sum);
    by_week.bucket = Bucket::Week;
    let week = build(&shop(), &by_week, Lang::En).expect("chart");
    assert_eq!(week.unit, Some(Bucket::Week));
    assert_eq!(Bucket::Week.code(), "week");
    let Points::Bars(points) = &week.points else {
        panic!("bars");
    };
    let total: f64 = points.iter().map(|bar| bar.value).sum();
    assert_eq!(total, 77.0);
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
fn date_steps_beyond_fifty_go_to_other() {
    let mut csv = String::from("day,sales\n");
    // 60 months; the first month has three rows, the others one.
    for row in 0..60 {
        csv.push_str(&format!(
            "{}-{:02}-01,{row}\n",
            2020 + row / 12,
            row % 12 + 1
        ));
    }
    csv.push_str("2020-01-15,100\n2020-01-20,200\n");
    let df = parse_table(&csv).expect("fake table");
    let data = build(
        &df,
        &request(ChartKind::Line, 0, Some(1), Aggregate::Sum),
        Lang::Nl,
    )
    .expect("chart");
    let points = bars(&data);
    assert_eq!(points.len(), MAX_GROUPS);
    assert_eq!(points[0].1, 300.0);
    // Kept: the first month (most rows), then the first 48 others. Other: the last eleven.
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
fn a_text_column_shows_its_ten_values_with_the_most_rows() {
    let mut csv = String::from("shop,sales\n");
    // 60 shops; shop0 has three rows, shop59 two, the others one.
    for row in 0..60 {
        csv.push_str(&format!("shop{row},{row}\n"));
    }
    csv.push_str("shop0,100\nshop0,200\nshop59,1\n");
    let df = parse_table(&csv).expect("fake table");
    for lang in [Lang::Nl, Lang::En] {
        let data = build(
            &df,
            &request(ChartKind::Line, 0, Some(1), Aggregate::Sum),
            lang,
        )
        .expect("chart");
        let points = bars(&data);
        assert_eq!(points.len(), MAX_CATEGORIES);
        // Most rows first picked, then the first ones; lines keep the table's order.
        let labels: Vec<&str> = points.iter().map(|(label, _)| label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "shop0", "shop1", "shop2", "shop3", "shop4", "shop5", "shop6", "shop7", "shop8",
                "shop59"
            ]
        );
        assert_eq!(points[0].1, 300.0);
        assert_eq!(points[9].1, 60.0);
        // No "Other": the rest is left out.
        let Points::Bars(raw) = &data.points else {
            panic!("bars");
        };
        assert!(raw.iter().all(|bar| !bar.other));
        assert!(!labels.contains(&"Overig") && !labels.contains(&"Other"));
    }
    // Ten values or fewer: all of them.
    let shop = build(
        &shop(),
        &request(ChartKind::Bar, 0, None, Aggregate::Count),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(bars(&shop).len(), 3);
}

/// Made-up people like those of a large sample CSV: unique ids counting up, first and last
/// names, e-mail addresses, ages and twelve cities, one with more rows than the next.
fn people() -> DataFrame {
    const FIRST: [&str; 7] = ["Anja", "Bram", "Cor", "Dirk", "Eva", "Fenna", "Gijs"];
    const LAST: [&str; 5] = ["Jansen", "de Vries", "Bakker", "Visser", "Smit"];
    let mut csv = String::from("id,first_name,last_name,email,age,city\n");
    let mut id = 0;
    for city in 0..12 {
        // City 0 has 13 rows, city 1 12, … city 11 two.
        for _ in 0..13 - city {
            id += 1;
            csv.push_str(&format!(
                "{id},{},{},person{id}@example.test,{},City{city}\n",
                FIRST[id % 7],
                LAST[id % 5],
                20 + (id * 7) % 50,
            ));
        }
    }
    parse_table(&csv).expect("fake people table")
}

#[test]
fn the_first_chart_is_the_mean_age_per_city() {
    let df = people();
    let columns = chart_columns(&df);
    let flags: Vec<(bool, bool, bool)> = columns
        .iter()
        .map(|c| (c.category, c.personal, c.id))
        .collect();
    assert_eq!(
        flags,
        [
            (false, false, true),
            // Seven first names over many rows: few values, but a name.
            (true, true, false),
            (true, true, false),
            (false, true, false),
            (false, false, false),
            (true, false, false),
        ]
    );
    let first = ChartRequest::first(&columns).expect("a chart");
    assert_eq!(first, request(ChartKind::Bar, 5, Some(4), Aggregate::Mean));
    let data = build(&df, &first, Lang::Nl).expect("chart");
    assert_eq!(data.x_label, "city");
    assert_eq!(data.y_label, "Gemiddelde van age");
    let points = bars(&data);
    // The ten cities with the most rows, City10 and City11 left out, no Overig.
    assert_eq!(points.len(), 10);
    let mut cities: Vec<String> = points.iter().map(|(label, _)| label.clone()).collect();
    cities.sort_by_key(|city| city[4..].parse::<u32>().expect("number"));
    let expected: Vec<String> = (0..10).map(|city| format!("City{city}")).collect();
    assert_eq!(cities, expected);
    // Bars from the highest average; each the mean of that city's ages.
    assert!(points.windows(2).all(|pair| pair[0].1 >= pair[1].1));
    let ages = df.column("age").expect("age").i64().expect("numbers");
    let towns = df.column("city").expect("city").str().expect("text");
    for (city, mean) in &points {
        let of_city: Vec<f64> = towns
            .iter()
            .zip(ages.iter())
            .filter(|(town, _)| *town == Some(city.as_str()))
            .filter_map(|(_, age)| age.map(|age| age as f64))
            .collect();
        let expected = of_city.iter().sum::<f64>() / of_city.len() as f64;
        assert!((mean - expected).abs() < 1e-9);
    }
}

#[test]
fn ids_and_names_are_spotted() {
    for name in [
        "id",
        "ID",
        "Id",
        "user_id",
        "userId",
        "order-id",
        "Customer ID",
    ] {
        assert!(id_name(name), "{name}");
    }
    for name in ["valid", "idea", "age", "paid", "width"] {
        assert!(!id_name(name), "{name}");
    }
    for name in [
        "first_name",
        "last_name",
        "FirstName",
        "lastname",
        "name",
        "Naam",
        "voornaam",
        "achternaam",
        "customer_name",
        "email",
        "E-mail",
        "e_mail_address",
        "phone",
        "postcode",
    ] {
        assert!(personal_name(name), "{name}");
    }
    for name in [
        "city",
        "category",
        "product_name",
        "country",
        "status",
        "zipper",
    ] {
        assert!(!personal_name(name), "{name}");
    }
    // Unique whole numbers counting up are an id, whatever the name; floats or repeats not.
    let table = parse_table(
        "nr,score,price,addr\n3,5,1.5,a@example.test\n7,5,2.5,b@example.test\n9,6,3.5,x\n",
    )
    .expect("fake table");
    let columns = chart_columns(&table);
    let ids: Vec<bool> = columns.iter().map(|c| c.id).collect();
    assert_eq!(ids, [true, false, false, false]);
    // Mostly values with an @: e-mail addresses, whatever the name.
    assert!(columns[3].personal);
}

#[test]
fn without_a_good_category_the_first_chart_falls_back() {
    let first =
        |csv: &str| ChartRequest::first(&chart_columns(&parse_table(csv).expect("fake table")));
    // Only ids for numbers: the row count per city.
    assert_eq!(
        first("id,city\n1,Utrecht\n2,Gouda\n3,Utrecht\n"),
        Some(request(ChartKind::Bar, 1, None, Aggregate::Count))
    );
    // A different city on every row: a date column instead.
    let mut unique = String::from("city,day,amount\n");
    for row in 0..30 {
        unique.push_str(&format!(
            "Town{row},2026-01-{:02},{}\n",
            row % 28 + 1,
            row % 4
        ));
    }
    assert_eq!(
        first(&unique),
        Some(request(ChartKind::Bar, 1, Some(2), Aggregate::Mean))
    );
    // Only names and two numbers: a scatter, the id last.
    let mut names = String::from("id,first_name,email,age,height\n");
    for row in 0..30 {
        names.push_str(&format!(
            "{row},Name{row},p{row}@example.test,{},{}\n",
            20 + row % 9,
            150 + row % 7
        ));
    }
    let scatter = first(&names).expect("a chart");
    assert_eq!(
        (scatter.kind, scatter.x, scatter.y),
        (ChartKind::Scatter, 3, Some(4))
    );
    // The people without their city: no text but names, so age against the id.
    let mut people = people();
    let _ = people.drop_in_place("city").expect("city");
    let columns = chart_columns(&people);
    let chosen = ChartRequest::first(&columns).expect("a chart");
    assert_eq!(
        (chosen.kind, chosen.x, chosen.y),
        (ChartKind::Scatter, 4, Some(0))
    );
    // Without age as well: the row count per first name, ten bars at most.
    let _ = people.drop_in_place("age").expect("age");
    let counted = ChartRequest::first(&chart_columns(&people)).expect("a chart");
    assert_eq!(counted, request(ChartKind::Bar, 1, None, Aggregate::Count));
    assert!(bars(&build(&people, &counted, Lang::En).expect("chart")).len() <= MAX_CATEGORIES);
    // Only names and an id: the id is the only number, so no scatter; the names it is.
    assert_eq!(
        first("id,first_name\n1,Anja\n2,Bram\n"),
        Some(request(ChartKind::Bar, 1, None, Aggregate::Count))
    );
    // Text with one value only, beside a name: the text that is not a name.
    assert_eq!(
        first("first_name,shop\nAnja,Webwinkel\nBram,Webwinkel\n"),
        Some(request(ChartKind::Bar, 1, None, Aggregate::Count))
    );
    // Nothing to draw: an id and true/false only.
    assert_eq!(first("id,paid\n1,true\n2,false\n"), None);
    assert_eq!(ChartRequest::first(&[]), None);
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
            r#"{"kind":"bar","locale":"nl","x_label":"month","y_label":"Som van revenue","#,
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
