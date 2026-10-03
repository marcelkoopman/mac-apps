/// Which part of a `12/03/2026` date is the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    /// `dd/mm/yyyy`, the default (Dutch and most of Europe).
    DayFirst,
    /// `mm/dd/yyyy`.
    MonthFirst,
}

impl DateOrder {
    fn format(self, separator: char) -> String {
        match self {
            Self::DayFirst => format!("%d{separator}%m{separator}%Y"),
            Self::MonthFirst => format!("%m{separator}%d{separator}%Y"),
        }
    }
}

/// What the text in a column says about its `a/b/yyyy` dates (`/`, `-` or `.` between).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateGuess {
    pub separator: char,
    /// `None` when every day and month is 12 or less, so both orders read every value.
    pub order: Option<DateOrder>,
}

/// The order of the dates in `values` (empty ones skipped), or `None` when one is not an
/// `a/b/yyyy` date, the separators differ, or no order reads them all.
pub fn date_guess<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<DateGuess> {
    let mut separator = None;
    let (mut first_over_12, mut second_over_12, mut any) = (false, false, false);
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let sep = value.chars().find(|ch| matches!(ch, '/' | '-' | '.'))?;
        if *separator.get_or_insert(sep) != sep {
            return None;
        }
        let mut parts = value.split(sep);
        let (a, b, year) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let small = |part: &str| {
            (1..=2).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
        };
        if !small(a) || !small(b) {
            return None;
        }
        let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
        if !(1..=31).contains(&a) || !(1..=31).contains(&b) {
            return None;
        }
        first_over_12 |= a > 12;
        second_over_12 |= b > 12;
        any = true;
    }
    let order = match (first_over_12, second_over_12) {
        (true, true) => return None,
        (true, false) => Some(DateOrder::DayFirst),
        (false, true) => Some(DateOrder::MonthFirst),
        (false, false) => None,
    };
    any.then_some(DateGuess {
        separator: separator?,
        order,
    })
}

/// A text column read as dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateColumn {
    pub order: DateOrder,
    /// Both orders read every value; `order` is the default ([`DateOrder::DayFirst`]) or the
    /// one asked for.
    pub ambiguous: bool,
}

/// Read every text column of `a/b/yyyy` dates in `df` as dates. Where both orders fit,
/// `ambiguous` decides. Returns the columns read.
fn read_dates(df: &mut DataFrame, ambiguous: DateOrder) -> Vec<DateColumn> {
    let mut read = Vec::new();
    let names: Vec<PlSmallStr> = df.get_column_names_owned();
    for name in names {
        let Ok(column) = df.column(name.as_str()) else {
            continue;
        };
        let Ok(text) = column.str() else {
            continue;
        };
        let Some(guess) = date_guess(text.iter().flatten()) else {
            continue;
        };
        let order = guess.order.unwrap_or(ambiguous);
        let Ok(dates) = text.as_date(Some(&order.format(guess.separator)), false) else {
            continue;
        };
        // Only when every date parses (31/02 does not).
        if dates.null_count() != text.null_count() {
            continue;
        }
        if df
            .replace(name.as_str(), dates.into_series().into_column())
            .is_ok()
        {
            read.push(DateColumn {
                order,
                ambiguous: guess.order.is_none(),
            });
        }
    }
    read
}

/// [`read_dates`] day first where both orders fit; the number of columns read.
pub fn read_dates_day_first(df: &mut DataFrame) -> usize {
    read_dates(df, DateOrder::DayFirst).len()
}

/// "Dates read as dd/mm/yyyy" when a column's dates fit both orders, so the card says which one
/// it chose.
pub fn ambiguous_dates_note(dates: &[DateColumn]) -> Option<String> {
    let column = dates.iter().find(|column| column.ambiguous)?;
    Some(match column.order {
        DateOrder::DayFirst => "Dates read as dd/mm/yyyy".to_string(),
        DateOrder::MonthFirst => "Dates read as mm/dd/yyyy".to_string(),
    })
}
