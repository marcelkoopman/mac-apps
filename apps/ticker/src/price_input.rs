//! Prices typed by the user (Add Price Watch prompt, `ticker add/remove`), in Dutch or English
//! notation: `68.000`, `68.000,00`, `68000`, `68000.5`, `68,5`, `€ 1.234,56`, `68,000.50`.

/// Parse a price. Rules for the separators:
/// - both `.` and `,`: the last one is the decimal separator, the other groups thousands;
/// - only `,`: one is the decimal comma (`68,5`), several group thousands (`1,234,567`);
/// - only `.`: several group thousands (`1.234.567`); one is a thousands separator when exactly
///   three digits follow and 1–3 digits (not starting with 0) precede it (`68.000` = 68000),
///   otherwise the decimal point (`68000.5`, `0.125`). Write `2,479` for 2.479.
///
/// Thousands groups must have three digits. Spaces and a leading or trailing `€` are ignored.
/// Letters (so also `inf`, `NaN` and exponents) are refused, as is anything not finite.
pub fn parse_price(input: &str) -> Result<f64, String> {
    let cleaned: String = input
        .trim()
        .trim_start_matches('€')
        .trim_end_matches('€')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let (negative, body) = match cleaned.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    let invalid = || format!("Not a price: {:?}", input.trim());
    if body.is_empty()
        || !body.chars().any(|c| c.is_ascii_digit())
        || !body
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return Err(invalid());
    }

    let dots = body.matches('.').count();
    let commas = body.matches(',').count();
    // (decimal separator, thousands separator)
    let (decimal, group) = match (dots, commas) {
        (0, 0) => (None, None),
        (_, 0) if dots > 1 => (None, Some('.')),
        (_, 0) => {
            let (int, frac) = body.split_once('.').ok_or_else(invalid)?;
            let grouped = frac.len() == 3 && (1..=3).contains(&int.len()) && !int.starts_with('0');
            if grouped {
                (None, Some('.'))
            } else {
                (Some('.'), None)
            }
        }
        (0, 1) => (Some(','), None),
        (0, _) => (None, Some(',')),
        _ => {
            if body.rfind(',') > body.rfind('.') {
                (Some(','), Some('.'))
            } else {
                (Some('.'), Some(','))
            }
        }
    };

    let (int, frac) = match decimal {
        Some(d) => {
            let (int, frac) = body.rsplit_once(d).ok_or_else(invalid)?;
            if frac.contains(d) || int.contains(d) || frac.chars().any(|c| !c.is_ascii_digit()) {
                return Err(invalid());
            }
            (int, frac)
        }
        None => (body, ""),
    };
    let digits = match group {
        Some(g) => {
            let groups: Vec<&str> = int.split(g).collect();
            let first_ok = (1..=3).contains(&groups[0].len());
            let rest_ok = groups[1..].iter().all(|grp| grp.len() == 3);
            if !first_ok || !rest_ok {
                return Err(invalid());
            }
            groups.concat()
        }
        None => int.to_string(),
    };
    if digits.chars().any(|c| !c.is_ascii_digit()) || (digits.is_empty() && frac.is_empty()) {
        return Err(invalid());
    }
    let text = format!(
        "{}{}.{}",
        if negative { "-" } else { "" },
        if digits.is_empty() { "0" } else { &digits },
        if frac.is_empty() { "0" } else { frac }
    );
    let value: f64 = text.parse().map_err(|_| invalid())?;
    if !value.is_finite() {
        return Err(invalid());
    }
    Ok(value)
}

/// A watch target: [`parse_price`], and more than zero unless the asset has `allow_negative`
/// (day-ahead power, where "below 0" is a real alert).
pub fn parse_watch_target(input: &str, allow_negative: bool) -> Result<f64, String> {
    let value = parse_price(input)?;
    if value <= 0.0 && !allow_negative {
        return Err(format!(
            "Target price must be above 0 (got {:?})",
            input.trim()
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> f64 {
        parse_price(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn dutch_notation() {
        assert_eq!(ok("68.000"), 68000.0);
        assert_eq!(ok("68.000,00"), 68000.0);
        assert_eq!(ok("68,5"), 68.5);
        assert_eq!(ok("1.234.567"), 1234567.0);
        assert_eq!(ok("1.234.567,89"), 1234567.89);
        assert_eq!(ok("€ 1.234,56"), 1234.56);
        assert_eq!(ok("0,19"), 0.19);
        assert_eq!(ok("2,479"), 2.479);
        assert_eq!(ok(" 66.553,00 "), 66553.0);
    }

    #[test]
    fn english_and_plain_notation() {
        assert_eq!(ok("68000"), 68000.0);
        assert_eq!(ok("68000.5"), 68000.5);
        assert_eq!(ok("68,000.50"), 68000.5);
        assert_eq!(ok("1,234,567"), 1234567.0);
        assert_eq!(ok("0.125"), 0.125);
        assert_eq!(ok("0.19"), 0.19);
        assert_eq!(ok(".5"), 0.5);
        assert_eq!(ok("5."), 5.0);
        assert_eq!(ok("1234.567"), 1234.567);
        assert_eq!(ok("+12"), 12.0);
        assert_eq!(ok("-0,05"), -0.05);
    }

    #[test]
    fn refuses_garbage_inf_nan_and_bad_groups() {
        for bad in [
            "",
            " ",
            "€",
            "abc",
            "inf",
            "-inf",
            "NaN",
            "infinity",
            "1e5",
            "12a",
            "1.23.45",
            "12.34.567",
            "1,2,3",
            "1.234,5,6",
            "--1",
            "1.2,3.4",
            ".",
            ",",
            "1 000x",
            "1234.567,8",
        ] {
            assert!(parse_price(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn watch_target_must_be_positive() {
        assert_eq!(parse_watch_target("68.000", false), Ok(68000.0));
        assert!(parse_watch_target("0", false).is_err());
        assert!(parse_watch_target("0,00", false).is_err());
        assert!(parse_watch_target("-5", false).is_err());
        assert!(parse_watch_target("NaN", false).is_err());
    }

    #[test]
    fn watch_target_may_be_zero_or_negative_with_allow_negative() {
        assert_eq!(parse_watch_target("0", true), Ok(0.0));
        assert_eq!(parse_watch_target("-0,05", true), Ok(-0.05));
        assert!(parse_watch_target("NaN", true).is_err());
        assert!(parse_watch_target("inf", true).is_err());
    }

    #[test]
    fn huge_values_stay_finite() {
        let big = "9".repeat(400);
        assert!(parse_price(&big).is_err());
    }
}
