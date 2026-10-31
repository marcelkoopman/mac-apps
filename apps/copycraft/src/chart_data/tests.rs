//! Fake tables only: revenue per category and day of a made-up shop, made-up people.

use super::*;
use crate::dataframe::parse_table;

const SHOP: &str = "\
category,day,revenue,units
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

fn pairs(data: &ChartData) -> Vec<(f64, f64)> {
    match &data.points {
        Points::Pairs(pairs) => pairs.clone(),
        Points::Labelled(_) => panic!("expected pairs"),
    }
}

fn labelled(data: &ChartData) -> Vec<(String, f64)> {
    match &data.points {
        Points::Labelled(points) => points.clone(),
        Points::Pairs(_) => panic!("expected labelled points"),
    }
}

fn request(x: usize, y: usize) -> ChartRequest {
    ChartRequest { x, y }
}

#[test]
fn columns_get_their_roles() {
    let columns = chart_columns(&shop());
    let roles: Vec<ColumnRole> = columns.iter().map(|c| c.role).collect();
    assert_eq!(
        roles,
        [
            ColumnRole::Text,
            ColumnRole::Date,
            ColumnRole::Number,
            ColumnRole::Number
        ]
    );
    // Two number columns that are not ids: revenue on x, units on y (a scatter).
    assert_eq!(ChartRequest::first(&columns), Some(request(2, 3)));
    // One such number column: the category on x, the number on y.
    let one = parse_table("category,revenue\nBooks,1\nGames,2\nBooks,3\n").expect("table");
    assert_eq!(
        ChartRequest::first(&chart_columns(&one)),
        Some(request(0, 1))
    );
    // Without a number column: no chart (nothing to put on the y axis).
    let text = parse_table("city,name\nUtrecht,Anja\nGouda,Jan\n").expect("fake table");
    assert_eq!(ChartRequest::first(&chart_columns(&text)), None);
}

#[test]
fn a_text_x_gives_one_point_per_row_in_table_order() {
    for (lang, series) in [
        (Lang::Nl, "revenue tegen category"),
        (Lang::En, "revenue vs. category"),
    ] {
        let data = build(&shop(), &request(0, 2), lang).expect("chart");
        assert_eq!(data.kind, ChartKind::Categories);
        // Nothing added up or averaged: every row, Books three times.
        assert_eq!(
            labelled(&data),
            [
                ("Books".to_string(), 10.5),
                ("Games".to_string(), 20.0),
                ("Books".to_string(), 4.5),
                ("Toys".to_string(), 7.0),
                ("Games".to_string(), 30.0),
                ("Books".to_string(), 5.0)
            ]
        );
        // The axis labels are the column names, nothing more.
        assert_eq!(
            (data.x_label.as_str(), data.y_label.as_str()),
            ("category", "revenue")
        );
        assert_eq!(data.series, series);
        assert_eq!(
            data.note,
            ChartNote {
                skipped: 0,
                shown: 6,
                total: 6
            }
        );
        assert_eq!(data.note.text(lang), "");
    }
}

#[test]
fn a_number_x_gives_a_scatter_in_x_order() {
    let df = parse_table("a,b\n3,30\n1,10\n2,20\n1,11\n").expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    assert_eq!(data.kind, ChartKind::Scatter);
    // Sorted by x; the same x keeps the table's order.
    assert_eq!(
        pairs(&data),
        [(1.0, 10.0), (1.0, 11.0), (2.0, 20.0), (3.0, 30.0)]
    );
    assert_eq!(data.series, "b vs. a");
    // Never one column against itself.
    assert_eq!(
        build(&df, &request(1, 1), Lang::En),
        Err(ChartProblem::Columns)
    );
}

#[test]
fn a_date_x_gives_points_by_day() {
    let df = shop();
    assert_eq!(df.columns()[1].dtype(), &DataType::Date);
    let data = build(&df, &request(1, 2), Lang::En).expect("chart");
    assert_eq!(data.kind, ChartKind::Dates);
    let points = pairs(&data);
    assert_eq!(points.len(), 6);
    // Days since 1970-01-01, one point per row, in date order.
    assert_eq!(civil_from_days(points[0].0 as i32), (2026, 1, 5));
    assert_eq!(points[0].1, 10.5);
    assert_eq!(civil_from_days(points[5].0 as i32), (2026, 3, 1));
    assert!(points.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    // Out of table order too.
    let shuffled =
        parse_table("day,sales\n2026-03-02,3\n2026-03-01,1\n2026-03-03,5\n").expect("table");
    let sorted = build(&shuffled, &request(0, 1), Lang::En).expect("chart");
    let values: Vec<f64> = pairs(&sorted).iter().map(|(_, y)| *y).collect();
    assert_eq!(values, [1.0, 3.0, 5.0]);
}

#[test]
fn a_datetime_x_keeps_the_time_of_day() {
    let df = parse_table("at,sales\n2026-03-01T12:00:00,1\n2026-03-01T00:00:00,2\n")
        .expect("fake table");
    let Some(ColumnRole::Date) = role(df.columns()[0].dtype()) else {
        // This parser keeps the column as text: then it is a category axis instead.
        let data = build(&df, &request(0, 1), Lang::En).expect("chart");
        assert_eq!(data.kind, ChartKind::Categories);
        return;
    };
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    let points = pairs(&data);
    assert_eq!(points[1].0 - points[0].0, 0.5);
    assert_eq!(points[0].1, 2.0);
}

#[test]
fn large_tables_are_sampled_evenly() {
    let mut csv = String::from("x,y\n");
    for row in 0..12_000 {
        csv.push_str(&format!("{row},{}\n", row * 2));
    }
    // One row without a y: skipped, and not in the total.
    csv.push_str("12000,\n");
    let df = parse_table(&csv).expect("fake table");
    let data = build(&df, &request(0, 1), Lang::Nl).expect("chart");
    let points = pairs(&data);
    assert_eq!(points.len(), MAX_POINTS);
    // Every 12 000 / 5 000-th row: the same rows each time.
    assert_eq!(points[0], (0.0, 0.0));
    assert_eq!(points[1], (2.0, 4.0));
    assert_eq!(points[2], (4.0, 8.0));
    assert_eq!(points[3], (7.0, 14.0));
    assert_eq!(points[MAX_POINTS - 1], (11_997.0, 23_994.0));
    let again = build(&df, &request(0, 1), Lang::Nl).expect("chart");
    assert_eq!(pairs(&again), points);
    assert_eq!(
        data.note,
        ChartNote {
            skipped: 1,
            shown: MAX_POINTS,
            total: 12_000
        }
    );
    assert_eq!(
        data.note.text(Lang::Nl),
        "Steekproef van 5.000 van 12.000 rijen · 1 waarde overgeslagen"
    );
    assert_eq!(
        data.note.text(Lang::En),
        "Sample of 5,000 of 12,000 rows · 1 value skipped"
    );
    let note = ChartNote {
        skipped: 0,
        shown: 5000,
        total: 64_659,
    };
    assert_eq!(note.text(Lang::Nl), "Steekproef van 5.000 van 64.659 rijen");
    // Exactly the limit: all rows, no note.
    let mut limit = String::from("x,y\n");
    for row in 0..MAX_POINTS {
        limit.push_str(&format!("{row},{row}\n"));
    }
    let all = build(
        &parse_table(&limit).expect("table"),
        &request(0, 1),
        Lang::En,
    )
    .expect("chart");
    assert_eq!(pairs(&all).len(), MAX_POINTS);
    assert_eq!(all.note.text(Lang::En), "");
}

#[test]
fn a_text_x_is_sampled_in_table_order_too() {
    let mut csv = String::from("shop,sales\n");
    for row in 0..10_000 {
        csv.push_str(&format!("shop{},{row}\n", row % 3));
    }
    let df = parse_table(&csv).expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    let points = labelled(&data);
    assert_eq!(points.len(), MAX_POINTS);
    assert_eq!(points[0], ("shop0".to_string(), 0.0));
    assert_eq!(points[1], ("shop2".to_string(), 2.0));
    assert_eq!(data.note.text(Lang::En), "Sample of 5,000 of 10,000 rows");
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
fn two_numbers_come_before_a_category() {
    // Like a sample people table: an id, names, age, salary and a city. Age against salary,
    // not one of them per city.
    let mut csv = String::from("id,first_name,age,city,salary\n");
    for row in 1..=40 {
        csv.push_str(&format!(
            "{row},Name{},{},City{},{}\n",
            row % 7,
            20 + row * 7 % 45,
            row % 5,
            30_000 + row * 1_337 % 50_000
        ));
    }
    let df = parse_table(&csv).expect("fake table");
    let first = ChartRequest::first(&chart_columns(&df)).expect("a chart");
    assert_eq!(first, request(2, 4));
    let data = build(&df, &first, Lang::En).expect("chart");
    assert_eq!(data.kind, ChartKind::Scatter);
    assert_eq!(
        (data.x_label.as_str(), data.y_label.as_str()),
        ("age", "salary")
    );
    assert_eq!(pairs(&data).len(), 40);
    // An id is no second number: with one number beside it the city is on x again.
    let mut ids = String::from("id,age,city\n");
    for row in 1..=20 {
        ids.push_str(&format!("{row},{},City{}\n", 20 + row % 9, row % 3));
    }
    let df = parse_table(&ids).expect("fake table");
    assert_eq!(
        ChartRequest::first(&chart_columns(&df)),
        Some(request(2, 1))
    );
}

#[test]
fn the_first_chart_is_age_per_city() {
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
    assert_eq!(first, request(5, 4));
    let data = build(&df, &first, Lang::Nl).expect("chart");
    assert_eq!(data.x_label, "city");
    assert_eq!(data.y_label, "age");
    // A point per person, all twelve cities, no Overig.
    let points = labelled(&data);
    assert_eq!(points.len(), df.height());
    let ages = df.column("age").expect("age").i64().expect("numbers");
    let towns = df.column("city").expect("city").str().expect("text");
    for (row, (city, age)) in points.iter().enumerate() {
        assert_eq!(Some(city.as_str()), towns.get(row));
        assert_eq!(Some(*age as i64), ages.get(row));
    }
    assert!(points.iter().any(|(city, _)| city == "City11"));
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
    // Only an id for numbers: the id on the y axis, per city.
    assert_eq!(
        first("id,city\n1,Utrecht\n2,Gouda\n3,Utrecht\n"),
        Some(request(1, 0))
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
    assert_eq!(first(&unique), Some(request(1, 2)));
    // Only names and two numbers: the first against the second, no id or name.
    let mut names = String::from("id,first_name,email,age,height\n");
    for row in 0..30 {
        names.push_str(&format!(
            "{row},Name{row},p{row}@example.test,{},{}\n",
            20 + row % 9,
            150 + row % 7
        ));
    }
    assert_eq!(first(&names), Some(request(3, 4)));
    // The people without their city: no text but names, so the id against age.
    let mut people = people();
    let _ = people.drop_in_place("city").expect("city");
    assert_eq!(
        ChartRequest::first(&chart_columns(&people)),
        Some(request(0, 4))
    );
    // Without age as well: the id is the only number; the first name for x.
    let _ = people.drop_in_place("age").expect("age");
    assert_eq!(
        ChartRequest::first(&chart_columns(&people)),
        Some(request(1, 0))
    );
    // Text with one value only beside a name, and a number: the text that is not a name.
    assert_eq!(
        first("first_name,shop,sales\nAnja,Webwinkel,1\nBram,Webwinkel,2\n"),
        Some(request(1, 2))
    );
    // Never a first chart of a column against itself.
    for csv in [
        SHOP,
        "id,city\n1,Utrecht\n2,Gouda\n3,Utrecht\n",
        "a,b,c,shop\n1,5,9,A\n4,2,7,B\n2,8,3,A\n",
        "first_name,shop,sales\nAnja,Webwinkel,1\nBram,Webwinkel,2\n",
    ] {
        let request = first(csv).expect("a chart");
        assert_ne!(request.x, request.y, "{csv}");
    }
    let request = ChartRequest::first(&chart_columns(&self::people())).expect("a chart");
    assert_ne!(request.x, request.y);
    // One number column beside true/false only: no x beside it.
    assert_eq!(first("sales,paid\n1,true\n2,false\n"), None);
    assert_eq!(first("id,paid\n1,true\n2,false\n"), None);
    assert_eq!(ChartRequest::first(&[]), None);
}

#[test]
fn empty_and_non_numeric_values_are_skipped() {
    let df = parse_table("category,revenue\nBooks,10\n,5\nGames,\nBooks,\nGames,7\nToys,2\n")
        .expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    assert_eq!(
        labelled(&data),
        [
            ("Books".to_string(), 10.0),
            ("Games".to_string(), 7.0),
            ("Toys".to_string(), 2.0)
        ]
    );
    // The row without a category and the two without revenue.
    assert_eq!(data.note.skipped, 3);
    assert_eq!(data.note.text(Lang::Nl), "3 waarden overgeslagen");
    assert_eq!(data.note.text(Lang::En), "3 values skipped");
}

#[test]
fn wrong_columns_and_no_values() {
    let df = shop();
    // Text on the y axis.
    assert_eq!(
        build(&df, &request(2, 0), Lang::En),
        Err(ChartProblem::Columns)
    );
    assert_eq!(
        build(&df, &request(9, 2), Lang::En),
        Err(ChartProblem::Columns)
    );
    assert_eq!(
        build(&df, &request(0, 9), Lang::En),
        Err(ChartProblem::Columns)
    );
    let empty =
        parse_table("category,revenue,units\n,1,1\nBooks,,2\nGames,,3\n").expect("fake table");
    assert_eq!(
        build(&empty, &request(0, 1), Lang::En),
        Err(ChartProblem::NoValues)
    );
    assert_eq!(
        ChartProblem::NoValues.message(Lang::En),
        "No numbers to draw"
    );
}

#[test]
fn json_snapshot() {
    let pairs = parse_table("a,b\n3,4\n1,2\n5,\n").expect("fake table");
    let scatter = build(&pairs, &request(0, 1), Lang::En).expect("chart");
    assert_eq!(
        scatter.to_json().as_str(),
        concat!(
            r#"{"kind":"scatter","locale":"en","x_label":"a","y_label":"b","#,
            r#""series":"b vs. a","points":[{"x":1.0,"y":2.0},{"x":3.0,"y":4.0}]}"#
        )
    );
    let days = parse_table("day,b\n1970-01-03,4\n1970-01-02,2\n").expect("fake table");
    let dates = build(&days, &request(0, 1), Lang::Nl).expect("chart");
    assert_eq!(
        dates.to_json().as_str(),
        concat!(
            r#"{"kind":"dates","locale":"nl","x_label":"day","y_label":"b","#,
            r#""series":"b tegen day","points":[{"x":1.0,"y":2.0},{"x":2.0,"y":4.0}]}"#
        )
    );
    let text = parse_table("shop,b\nB,1\nA,2\n").expect("fake table");
    let categories = build(&text, &request(0, 1), Lang::En).expect("chart");
    assert_eq!(
        categories.to_json().as_str(),
        concat!(
            r#"{"kind":"categories","locale":"en","x_label":"shop","y_label":"b","#,
            r#""series":"b vs. shop","points":[{"label":"B","y":1.0},{"label":"A","y":2.0}]}"#
        )
    );
}

#[test]
fn the_pop_ups_offer_what_fits() {
    let columns = chart_columns(&shop());
    // X: every column; y: the number columns.
    assert_eq!(x_choices(&columns), [0, 1, 2, 3]);
    // Y: the number columns but the x column.
    assert_eq!(y_choices(&columns, 0), [2, 3]);
    assert_eq!(y_choices(&columns, 2), [3]);
    assert_eq!(y_choices(&columns, 3), [2]);
    // True/false columns are not offered.
    let flags = chart_columns(&parse_table("paid,sales\ntrue,1\nfalse,2\n").expect("table"));
    assert_eq!(x_choices(&flags), [1]);
}

#[test]
fn a_request_is_fitted_to_the_columns() {
    let columns = chart_columns(&shop());
    // A request that fits stays as it is, x a number column too.
    assert_eq!(request(3, 2).fitted(&columns), Some(request(3, 2)));
    assert_eq!(request(1, 3).fitted(&columns), Some(request(1, 3)));
    // Text for y, or a column that is gone: the first choice's.
    assert_eq!(request(1, 0).fitted(&columns), Some(request(1, 3)));
    assert_eq!(request(9, 9).fitted(&columns), Some(request(2, 3)));
    // X moved onto the y column: the y goes to the next number column, round to the first.
    assert_eq!(request(3, 3).fitted(&columns), Some(request(3, 2)));
    assert_eq!(request(2, 2).fitted(&columns), Some(request(2, 3)));
    let three =
        chart_columns(&parse_table("a,b,c,shop\n1,5,9,A\n4,2,7,B\n2,8,3,A\n").expect("table"));
    assert_eq!(request(1, 1).fitted(&three), Some(request(1, 2)));
    assert_eq!(request(2, 2).fitted(&three), Some(request(2, 0)));
    // The only number column as x: nothing left for y, so the first choice.
    let one = chart_columns(&parse_table("shop,sales\nA,5\nB,2\nA,8\n").expect("table"));
    assert_eq!(request(1, 1).fitted(&one), Some(request(0, 1)));
    // No number column, or no columns: nothing.
    let text = chart_columns(&parse_table("category,shop\nBooks,A\nGames,B\n").expect("table"));
    assert_eq!(request(0, 1).fitted(&text), None);
    assert_eq!(request(0, 1).fitted(&[]), None);
}

#[test]
fn an_outcome_shows_no_content() {
    let outcome = ChartOutcome::build(&shop(), &request(0, 2), Lang::En, 7);
    let (json, note) = outcome.result.as_ref().expect("chart");
    assert!(json.contains("Books"));
    assert_eq!(note.skipped, 0);
    // A text x: nothing for Summarize.
    assert!(outcome.trend.is_none());
    let shown = format!("{outcome:?}");
    assert!(
        !shown.contains("Books") && !shown.contains("revenue"),
        "{shown}"
    );
    assert!(shown.contains('7'));
    let copy = outcome.clone();
    assert_eq!(copy.generation, 7);
    // A date x: Summarize gets the points.
    let dated = ChartOutcome::build(&shop(), &request(1, 2), Lang::En, 8);
    assert_eq!(
        dated.trend.as_ref().map(|trend| trend.points.len()),
        Some(6)
    );
    let data = build(&shop(), &request(0, 2), Lang::En).expect("chart");
    let debug = format!("{:?}", data.note);
    assert!(!debug.contains("Books"));
    let problem = ChartOutcome::build(&shop(), &request(2, 0), Lang::En, 9);
    assert!(problem.result.is_err());
}
