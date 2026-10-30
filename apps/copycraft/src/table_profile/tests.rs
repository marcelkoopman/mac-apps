//! Fake orders only: made-up ids, example.test addresses and amounts.

use super::*;
use crate::dataframe::parse_table;

/// 50 rows: 48 orders and two rows repeated (orders 1 and 2). Every 8th e-mail empty (6 of
/// 50, 12 %); amounts 100.25–109.25 with three outliers (900.5, 950.5 and 5.5 at rows 9, 19
/// and 29); two dates that are no date ("later"), so order_date stays text; one shop.
fn orders_csv() -> String {
    let mut csv = String::from("order_id,email,amount,order_date,shop\n");
    let row = |i: usize| {
        let email = if i.is_multiple_of(8) {
            String::new()
        } else {
            format!("buyer{i}@example.test")
        };
        let amount = match i {
            10 => String::from("900.5"),
            20 => String::from("950.5"),
            30 => String::from("5.5"),
            _ => format!("{}.25", 100 + i % 10),
        };
        let date = if i == 25 || i == 40 {
            String::from("later")
        } else {
            format!("2026-01-{:02}", i % 28 + 1)
        };
        format!("ORD-{i:04},{email},{amount},{date},Webwinkel\n")
    };
    for i in 1..=48 {
        csv.push_str(&row(i));
    }
    csv.push_str(&row(1));
    csv.push_str(&row(2));
    csv
}

fn orders() -> TableProfile {
    profile(&parse_table(&orders_csv()).expect("fake orders"))
}

#[test]
fn counts_per_column_and_table() {
    let p = orders();
    assert_eq!(p.rows, 50);
    assert_eq!(p.duplicate_rows, Some(2));
    let kinds: Vec<ColumnKind> = p.columns.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            ColumnKind::Text,
            ColumnKind::Text,
            ColumnKind::Decimal,
            ColumnKind::Text,
            ColumnKind::Text
        ]
    );
    let [id, email, amount, date, shop] = &p.columns[..] else {
        panic!("five columns");
    };
    assert_eq!((id.missing, id.unique, id.possible_id), (0, 48, true));
    // Nearly unique, but too often empty to be an ID.
    assert_eq!(
        (email.missing, email.unique, email.possible_id),
        (6, 42, false)
    );
    assert_eq!(date.looks_like, Some(LooksLike::Dates));
    assert_eq!(id.looks_like, None);
    assert_eq!((shop.unique, shop.possible_id), (1, false));
    assert!(!amount.possible_id);
    let numbers = amount.numbers.as_ref().expect("numbers");
    assert_eq!(numbers.count, 50);
    assert_eq!(numbers.outliers, 3);
    assert_eq!((numbers.min, numbers.max), (5.5, 950.5));
    assert!(numbers.low_fence() > 5.5 && numbers.high_fence() < 900.5);
    // The furthest past its fence first; rows as in the table (0-based).
    let rows: Vec<(usize, bool)> = numbers.extremes.iter().map(|e| (e.row, e.high)).collect();
    assert_eq!(rows, [(19, true), (9, true), (29, false)]);
}

#[test]
fn quartiles_interpolate() {
    let sorted = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(quantile(&sorted, 0.5), 2.5);
    assert_eq!(quantile(&sorted, 0.25), 1.75);
    assert_eq!(quantile(&sorted, 0.75), 3.25);
    assert_eq!(quantile(&[7.0], 0.25), 7.0);
}

#[test]
fn findings_are_ordered_by_score() {
    let p = orders();
    assert_eq!(
        p.findings,
        [
            Finding::NotRecognised {
                column: 3,
                looks: LooksLike::Dates
            },
            Finding::Missing {
                column: 1,
                count: 6
            },
            Finding::DuplicateRows { count: 2 },
            Finding::Outliers {
                column: 2,
                count: 3
            },
            Finding::OneValue { column: 4 },
        ]
    );
    let scores: Vec<u32> = p.findings.iter().map(|f| f.score(&p)).collect();
    assert_eq!(scores, [80, 62, 54, 52, 30]);
}

#[test]
fn the_top_three_in_dutch_and_english() {
    let p = orders();
    assert_eq!(
        p.top(Lang::Nl),
        [
            "order_date is niet als datum herkend",
            "email heeft 12% missende waarden",
            "2 dubbele rijen (4%)",
        ]
    );
    assert_eq!(
        p.top(Lang::En),
        [
            "order_date was not recognised as a date",
            "email has 12% missing values",
            "2 duplicate rows (4%)",
        ]
    );
    let outliers = Finding::Outliers {
        column: 2,
        count: 3,
    };
    assert_eq!(
        outliers.sentence(&p, Lang::Nl),
        "amount bevat 3 uitschieters"
    );
    assert_eq!(
        outliers.sentence(&p, Lang::En),
        "amount contains 3 outliers"
    );
}

#[test]
fn a_clean_table_has_nothing_that_stands_out() {
    let p = profile(&parse_table("name\tsize\nalpha\t3\nbeta\t4\ngamma\t5\n").expect("tsv"));
    assert!(p.findings.is_empty());
    assert_eq!(p.top(Lang::Nl), ["Geen opvallende punten"]);
    assert_eq!(p.top(Lang::En), ["Nothing stands out"]);
}

#[test]
fn json_and_pasted_tables_are_profiled_alike() {
    let json = r#"[{"shop":"A","sales":3},{"shop":"B","sales":null},{"shop":"B","sales":4}]"#;
    let p = profile(&parse_table(json).expect("json table"));
    assert_eq!(p.rows, 3);
    assert_eq!(p.columns[1].missing, 1);
    assert_eq!(p.columns[0].unique, 2);
}

#[test]
fn the_copy_holds_no_cell_values() {
    let csv = orders_csv();
    let p = orders();
    for lang in [Lang::Nl, Lang::En] {
        let copy = p.markdown(lang);
        let mut lines = csv.lines();
        lines.next();
        for line in lines {
            for value in line.split(',').filter(|v| !v.is_empty()) {
                assert!(!copy.contains(value), "{value} in the copy");
            }
        }
        for name in ["order_id", "email", "amount", "order_date", "shop"] {
            assert!(copy.contains(name));
        }
        for line in p.top(lang) {
            assert!(copy.contains(&line));
        }
    }
    let copy = p.markdown(Lang::En);
    assert!(copy.contains("| Column | Kind | Missing | Unique | Outliers | Note |"));
    assert!(
        copy.contains("| email | Text | 6 (12%) | 42 | – |  |"),
        "{}",
        *copy
    );
    assert!(
        copy.contains("| amount | Decimal | 0 (0%) | 13 | 3 |  |"),
        "{}",
        *copy
    );
}

#[test]
fn the_panel_shows_the_numbers_in_the_language() {
    let p = orders();
    let nl = p.text(Lang::Nl);
    assert!(nl.starts_with("• order_date is niet als datum herkend\n"));
    assert!(nl.contains("50 rijen × 5 kolommen · 2 dubbele rijen (4%)"));
    assert!(nl.contains("min 5,5 · max 950,5"), "{}", *nl);
    let en = p.text(Lang::En);
    assert!(en.contains("min 5.5 · max 950.5"), "{}", *en);
    assert_eq!(percent(1, 3, Lang::Nl), "33%");
    assert_eq!(percent(1, 40, Lang::Nl), "2,5%");
    assert_eq!(whole(1_234_567, Lang::Nl), "1.234.567");
    assert_eq!(whole(1_234_567, Lang::En), "1,234,567");
}

#[test]
fn a_column_name_with_a_bar_stays_one_cell() {
    assert_eq!(cell("a|b\nc"), "a\\|b c");
}
