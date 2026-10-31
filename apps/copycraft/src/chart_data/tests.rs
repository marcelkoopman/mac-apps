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
    ChartRequest { x, y, start: 0 }
}

fn at(start: usize, x: usize, y: usize) -> ChartRequest {
    ChartRequest { x, y, start }
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
fn a_text_x_gives_one_bar_per_row_in_table_order() {
    for (lang, series) in [
        (Lang::Nl, "revenue tegen category"),
        (Lang::En, "revenue vs. category"),
    ] {
        let data = build(&shop(), &request(0, 2), lang).expect("chart");
        assert_eq!(data.kind, ChartKind::Bars);
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
                start: 0,
                end: 6,
                total: 6,
                skipped: 0,
            }
        );
        assert_eq!(data.note.text(lang), "");
    }
}

#[test]
fn rows_with_the_same_text_stay_separate_bars() {
    let df = parse_table("last_name,salary\nWilliams,3100\nSmith,2800\nWilliams,4200\n")
        .expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    assert_eq!(
        labelled(&data),
        [
            ("Williams".to_string(), 3100.0),
            ("Smith".to_string(), 2800.0),
            ("Williams".to_string(), 4200.0)
        ]
    );
    // Three bars in the JSON, both Williams rows, in table order.
    assert_eq!(
        data.to_json().as_str(),
        concat!(
            r#"{"kind":"bars","locale":"en","x_label":"last_name","y_label":"salary","#,
            r#""series":"salary vs. last_name","points":[{"label":"Williams","y":3100.0},"#,
            r#"{"label":"Smith","y":2800.0},{"label":"Williams","y":4200.0}]}"#
        )
    );
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
        // This parser keeps the column as text: then it gives bars instead.
        let data = build(&df, &request(0, 1), Lang::En).expect("chart");
        assert_eq!(data.kind, ChartKind::Bars);
        return;
    };
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    let points = pairs(&data);
    assert_eq!(points[1].0 - points[0].0, 0.5);
    assert_eq!(points[0].1, 2.0);
}

/// `rows` rows of x = row, y = row * 2 (fake numbers).
fn counting(rows: usize) -> DataFrame {
    let mut csv = String::from("x,y\n");
    for row in 0..rows {
        csv.push_str(&format!("{row},{}\n", row * 2));
    }
    parse_table(&csv).expect("fake table")
}

#[test]
fn only_a_window_of_fifty_rows_is_drawn() {
    let df = counting(64_659);
    let data = build(&df, &request(0, 1), Lang::Nl).expect("chart");
    let points = pairs(&data);
    // Rows 1–50, nothing more, whatever the table's size.
    assert_eq!(points.len(), WINDOW);
    assert_eq!(points[0], (0.0, 0.0));
    assert_eq!(points[WINDOW - 1], (49.0, 98.0));
    assert_eq!(
        data.note,
        ChartNote {
            start: 0,
            end: 50,
            total: 64_659,
            skipped: 0,
        }
    );
    assert_eq!(data.note.text(Lang::Nl), "Rij 1–50 van 64.659");
    assert_eq!(data.note.text(Lang::En), "Rows 1–50 of 64,659");
    assert_eq!(data.note.previous(), None);
    assert_eq!(data.note.next(), Some(50));
    // The window after: rows 51–100.
    let second = build(&df, &at(50, 0, 1), Lang::En).expect("chart");
    assert_eq!(pairs(&second)[0], (50.0, 100.0));
    assert_eq!(second.note.text(Lang::En), "Rows 51–100 of 64,659");
    assert_eq!(second.note.previous(), Some(0));
    // The last window has the 9 rows left; no next.
    let last = build(&df, &at(64_650, 0, 1), Lang::En).expect("chart");
    assert_eq!(pairs(&last).len(), 9);
    assert_eq!(pairs(&last)[8], (64_658.0, 129_316.0));
    assert_eq!(last.note.text(Lang::En), "Rows 64,651–64,659 of 64,659");
    assert_eq!(last.note.next(), None);
    assert_eq!(last.note.previous(), Some(64_600));
    // The JSON stays small: 50 points.
    assert!(data.to_json().len() < 2_000);
}

#[test]
fn the_window_stays_inside_the_table() {
    // 120 rows: windows from 0, 50 and 100 (the last one 20 rows).
    let note = |start| ChartNote::window(start, 120);
    assert_eq!((note(0).start, note(0).end), (0, 50));
    assert_eq!((note(100).start, note(100).end), (100, 120));
    // Past the end: the last window. Between two: the one it is in.
    assert_eq!((note(500).start, note(500).end), (100, 120));
    assert_eq!((note(73).start, note(73).end), (50, 100));
    // Paging forward and back walks 0, 50, 100 and stops at both ends.
    let mut starts = vec![0];
    let mut shown = note(0);
    while let Some(next) = shown.next() {
        starts.push(next);
        shown = note(next);
    }
    assert_eq!(starts, [0, 50, 100]);
    assert_eq!(shown.previous(), Some(50));
    assert_eq!(note(50).previous(), Some(0));
    assert_eq!(note(0).previous(), None);
    // Exactly one window, or fewer rows: no paging, no note.
    for total in [0, 1, 50] {
        let one = ChartNote::window(0, total);
        assert_eq!((one.previous(), one.next()), (None, None), "{total}");
        assert_eq!(one.text(Lang::En), "", "{total}");
    }
    assert_eq!(ChartNote::window(0, 51).next(), Some(50));
    // A table that got shorter: the chart moves back to its last window.
    let data = build(&counting(120), &at(9_000, 0, 1), Lang::En).expect("chart");
    assert_eq!(data.note.start, 100);
    assert_eq!(pairs(&data).len(), 20);
}

#[test]
fn rows_without_values_are_skipped_inside_the_window() {
    let mut csv = String::from("shop,sales\n");
    for row in 0..120 {
        // Every tenth row has no sales.
        let sales = if row % 10 == 3 {
            String::new()
        } else {
            row.to_string()
        };
        csv.push_str(&format!("shop{},{sales}\n", row % 3));
    }
    let df = parse_table(&csv).expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    let points = labelled(&data);
    // 50 rows, 5 of them without sales: 45 bars, in table order.
    assert_eq!(points.len(), 45);
    assert_eq!(points[0], ("shop0".to_string(), 0.0));
    assert_eq!(points[3], ("shop1".to_string(), 4.0));
    assert_eq!(data.note.skipped, 5);
    assert_eq!(
        data.note.text(Lang::En),
        "Rows 1–50 of 120 · 5 values skipped"
    );
    assert_eq!(
        data.note.text(Lang::Nl),
        "Rij 1–50 van 120 · 5 waarden overgeslagen"
    );
    // A window without a single value: no chart, but the window is still known for paging.
    let mut gap = String::from("x,y\n");
    for row in 0..100 {
        let y = if row < 50 {
            row.to_string()
        } else {
            String::new()
        };
        gap.push_str(&format!("{row},{y}\n"));
    }
    let gap = parse_table(&gap).expect("fake table");
    let outcome = ChartOutcome::build(&gap, &at(50, 0, 1), Lang::En, 1);
    assert!(matches!(outcome.result, Err(ChartProblem::NoValues)));
    assert_eq!(outcome.note.previous(), Some(0));
    assert_eq!(outcome.note.text(Lang::En), "Rows 51–100 of 100");
    let back = ChartOutcome::build(&gap, &request(0, 1), Lang::En, 2);
    assert!(back.result.is_ok());
}

#[test]
fn long_labels_are_cut() {
    let long = "a".repeat(200);
    let name = "é".repeat(60);
    let df = parse_table(&format!("{name},b\n{long},1\nshort,2\n")).expect("fake table");
    let data = build(&df, &request(0, 1), Lang::En).expect("chart");
    let points = labelled(&data);
    assert_eq!(points[0].0.chars().count(), MAX_LABEL_CHARS);
    assert!(points[0].0.ends_with('…'));
    assert_eq!(points[1].0, "short");
    assert_eq!(data.x_label.chars().count(), MAX_LABEL_CHARS);
    assert_eq!(
        cut(&"x".repeat(MAX_LABEL_CHARS)),
        "x".repeat(MAX_LABEL_CHARS)
    );
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
    // A bar per person of the first 50, no Overig.
    let points = labelled(&data);
    assert_eq!(points.len(), WINDOW);
    let ages = df.column("age").expect("age").i64().expect("numbers");
    let towns = df.column("city").expect("city").str().expect("text");
    for (row, (city, age)) in points.iter().enumerate() {
        assert_eq!(Some(city.as_str()), towns.get(row));
        assert_eq!(Some(*age as i64), ages.get(row));
    }
    // The next window has the other people.
    let rest = build(&df, &at(WINDOW, 5, 4), Lang::Nl).expect("chart");
    assert_eq!(labelled(&rest).len(), df.height() - WINDOW);
    assert_eq!(rest.note.next(), None);
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
            r#"{"kind":"bars","locale":"en","x_label":"shop","y_label":"b","#,
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
    // The window stays when the columns change.
    assert_eq!(at(150, 3, 3).fitted(&columns), Some(at(150, 3, 2)));
    assert_eq!(at(150, 1, 1).fitted(&one), Some(at(150, 0, 1)));
}

#[test]
fn an_outcome_shows_no_content() {
    let outcome = ChartOutcome::build(&shop(), &request(0, 2), Lang::En, 7);
    let json = outcome.result.as_ref().expect("chart");
    assert!(json.contains("Books"));
    assert_eq!(outcome.note.skipped, 0);
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
