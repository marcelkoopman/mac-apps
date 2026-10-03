// How the card shows a table: column names with a long shared prefix shortened, and for wide
// tables a column overview. Display only: Copy, Save and the conversions keep the real names.

/// Above this many columns the Dataframe view opens on the column overview.
pub const OVERVIEW_MIN_COLUMNS: usize = 7;
/// Example values per column in the overview…
const OVERVIEW_SAMPLES: usize = 3;
/// …as many as fit in this many characters (polars cuts longer cells at 32).
const OVERVIEW_EXAMPLES_CHARS: usize = 30;
/// A prefix is shortened when this many columns share it…
const PREFIX_MIN_COLUMNS: usize = 3;
/// …and it is at least this many characters long (separator included).
const PREFIX_MIN_CHARS: usize = 10;
/// Where a product or group name ends in a column name ("Sunbox 7 - PV1 Generation").
const PREFIX_SEPARATORS: [&str; 6] = [" - ", " – ", " — ", ": ", " | ", " / "];
/// Stands for the shortened prefix.
const ELLIPSIS: &str = "… ";

/// The prefixes of `name` that end in a separator, shortest first.
fn prefixes(name: &str) -> Vec<&str> {
    let mut ends: Vec<usize> = PREFIX_SEPARATORS
        .iter()
        .flat_map(|sep| name.match_indices(sep).map(move |(at, _)| at + sep.len()))
        .filter(|&end| end < name.len())
        .collect();
    ends.sort_unstable();
    ends.dedup();
    ends.into_iter().map(|end| &name[..end]).collect()
}

/// Column names for display: where at least [`PREFIX_MIN_COLUMNS`] names share a prefix of
/// [`PREFIX_MIN_CHARS`] or more up to a separator, it becomes "…" in each (the longest such
/// prefix per name, so each group is shortened on its own). Names stay as they are when
/// shortening would make two of them the same.
pub fn display_names(names: &[String]) -> Vec<String> {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for name in names {
        for prefix in prefixes(name) {
            *counts.entry(prefix).or_default() += 1;
        }
    }
    let shown: Vec<String> = names
        .iter()
        .map(|name| {
            let shared = prefixes(name).into_iter().rev().find(|prefix| {
                prefix.chars().count() >= PREFIX_MIN_CHARS
                    && counts.get(prefix).copied().unwrap_or(0) >= PREFIX_MIN_COLUMNS
            });
            match shared {
                Some(prefix) => format!("{ELLIPSIS}{}", &name[prefix.len()..]),
                None => name.clone(),
            }
        })
        .collect();
    let unique: std::collections::HashSet<&String> = shown.iter().collect();
    if unique.len() == shown.len() {
        shown
    } else {
        names.to_vec()
    }
}

/// `df` with its columns renamed for display ([`display_names`]).
fn display_frame(df: DataFrame) -> DataFrame {
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let shown = display_names(&names);
    if shown == names {
        return df;
    }
    let mut df = df;
    if df.set_column_names(&shown).is_err() {
        // Not expected: `display_names` keeps the names unique.
        return df;
    }
    df
}

/// A cell as text; `None` for an empty one.
pub fn cell_text(value: AnyValue) -> Option<String> {
    match value {
        AnyValue::Null => None,
        AnyValue::String(text) => Some(text.to_string()),
        AnyValue::StringOwned(text) => Some(text.to_string()),
        other => Some(other.to_string()),
    }
}

/// The column overview: one row per column with its (display) name, type and the first few
/// different values. `None` for a table of at most [`OVERVIEW_MIN_COLUMNS`] - 1 columns.
pub fn overview(df: &DataFrame) -> Option<String> {
    if df.width() < OVERVIEW_MIN_COLUMNS {
        return None;
    }
    let names: Vec<String> = df
        .get_column_names()
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    let mut types = Vec::with_capacity(names.len());
    let mut examples = Vec::with_capacity(names.len());
    for column in df.columns() {
        types.push(column.dtype().to_string());
        let mut seen: Vec<String> = Vec::new();
        for row in 0..column.len() {
            if seen.len() == OVERVIEW_SAMPLES {
                break;
            }
            if let Some(value) = column.get(row).ok().and_then(cell_text)
                && !seen.contains(&value)
            {
                let joined: usize = seen.iter().map(|v| v.chars().count() + 2).sum();
                if !seen.is_empty() && joined + value.chars().count() > OVERVIEW_EXAMPLES_CHARS {
                    break;
                }
                seen.push(value);
            }
        }
        examples.push(seen.join(", "));
    }
    let table = DataFrame::new(
        names.len(),
        vec![
            Column::new("column".into(), display_names(&names)),
            Column::new("type".into(), types),
            Column::new("examples".into(), examples),
        ],
    )
    .ok()?;
    render(table)
}
