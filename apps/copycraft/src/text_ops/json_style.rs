//! Format JSON… in Text ▾: the copied JSON laid out with a 2-space indent (Format) or on one
//! line (Compress), with the keys of every object sorted when asked.
//!
//! It reuses the JSON the app already reads (`serde_json`, keys in their copied order) and the
//! key sort of the JSON diff ([`crate::diff::sort_json`]); there is no parser of its own. Only
//! a small scan of the copied text (outside strings) makes sure nothing is lost: a key that
//! is there twice (JSON keeps the last one) or a number that `serde_json` cannot carry exactly
//! (a big integer, more than 15 significant digits) refuses the step instead of changing data.
//! Other spellings of the same value are rewritten the way `serde_json` writes them: `1.50`
//! becomes `1.5`, `1E3` becomes `1000.0`, `\u00e9` becomes `é` and `\/` becomes `/`.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

/// What Format JSON… makes of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonStyle {
    /// One line instead of a 2-space indent.
    pub compress: bool,
    /// The keys of every object in byte order.
    pub sort: bool,
}

/// Why the JSON is not restyled. Display is a fixed English text without the problem's
/// position or message; the card's NL/EN text is [`JsonStyleError::note`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonStyleError {
    /// It does not parse: 1-based line and column in the copied text, and what is wrong.
    #[error("the JSON does not parse")]
    Broken(crate::validate::JsonProblem),
    /// A key appears twice in one object; restyling would keep only the last.
    #[error("the JSON has a duplicate key")]
    DuplicateKeys,
    /// A number would come back with another value (too big or too precise).
    #[error("a JSON number would change")]
    LossyNumbers,
}

impl JsonStyleError {
    /// For the dialog and the meta line: `Line 4, column 1: trailing comma`.
    pub fn note(&self) -> String {
        match self {
            JsonStyleError::Broken(problem) => problem.note(),
            JsonStyleError::DuplicateKeys => crate::locale::t("json_duplicate_keys").to_string(),
            JsonStyleError::LossyNumbers => crate::locale::t("json_numbers_lossy").to_string(),
        }
    }
}

/// `text` (JSON) laid out as `style` asks. A final newline stays when the copy had one;
/// whitespace around the JSON does not.
pub fn restyle(text: &str, style: JsonStyle) -> Result<String, JsonStyleError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|err| {
        JsonStyleError::Broken(crate::validate::broken_json(text).unwrap_or_else(|| {
            let full = err.to_string();
            let message = full
                .rfind(" at line ")
                .map_or(full.as_str(), |at| &full[..at])
                .to_string();
            crate::validate::JsonProblem {
                line: err.line().max(1),
                column: err.column().max(1),
                message,
            }
        }))
    })?;
    let scan = scan(text);
    if scan.keys != key_count(&value) {
        return Err(JsonStyleError::DuplicateKeys);
    }
    // In the copied order (`preserve_order`) the numbers come back one for one.
    let compact = serde_json::to_string(&value).unwrap_or_default();
    let written = scan_numbers(&compact);
    if written.len() != scan.numbers.len()
        || scan
            .numbers
            .iter()
            .zip(&written)
            .any(|(copied, back)| copied != back && lossy(copied))
    {
        return Err(JsonStyleError::LossyNumbers);
    }
    let value = if style.sort {
        crate::diff::sort_json(value)
    } else {
        value
    };
    let mut out = if style.compress {
        if style.sort {
            serde_json::to_string(&value).unwrap_or_default()
        } else {
            compact
        }
    } else {
        // serde_json's pretty printer indents with two spaces.
        serde_json::to_string_pretty(&value).unwrap_or_default()
    };
    if text.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// A number whose rewrite may change its value: an integer (one `serde_json` writes another
/// way no longer fits 64 bits) or one with more than 15 significant digits.
fn lossy(number: &str) -> bool {
    let integer = !number.contains(['.', 'e', 'E']);
    let mantissa = number.split(['e', 'E']).next().unwrap_or(number);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let significant = digits.trim_start_matches('0').trim_end_matches('0').len();
    integer || significant > 15
}

/// Keys of every object, nested ones too.
fn key_count(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map.len() + map.values().map(key_count).sum::<usize>(),
        serde_json::Value::Array(items) => items.iter().map(key_count).sum(),
        _ => 0,
    }
}

/// What the copied text holds outside its strings.
struct Scan<'a> {
    /// Strings followed by `:`.
    keys: usize,
    /// The number literals, as copied, in order.
    numbers: Vec<&'a str>,
}

/// Scan JSON that parses: strings (with their escapes) are skipped, a string followed by `:`
/// is a key, and a run of `-+.0-9eE` outside strings is a number.
fn scan(text: &str) -> Scan<'_> {
    let bytes = text.as_bytes();
    let mut keys = 0;
    let mut numbers = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
                at += 1;
                let mut next = at;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if bytes.get(next) == Some(&b':') {
                    keys += 1;
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = at;
                while at < bytes.len()
                    && matches!(bytes[at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    at += 1;
                }
                numbers.push(&text[start..at]);
            }
            _ => at += 1,
        }
    }
    Scan { keys, numbers }
}

/// The number literals of `text`, in order.
fn scan_numbers(text: &str) -> Vec<&str> {
    scan(text).numbers
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: JsonStyle = JsonStyle {
        compress: false,
        sort: false,
    };
    const COMPRESS: JsonStyle = JsonStyle {
        compress: true,
        sort: false,
    };

    #[test]
    fn format_indents_with_two_spaces_in_the_copied_order() {
        assert_eq!(
            restyle(r#"{"b":1,"a":[1,2]}"#, FORMAT).unwrap(),
            "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ]\n}"
        );
        // A final newline stays; whitespace around the JSON goes.
        assert_eq!(restyle("  [1,2]  \n", FORMAT).unwrap(), "[\n  1,\n  2\n]\n");
        assert_eq!(restyle("{}", FORMAT).unwrap(), "{}");
    }

    #[test]
    fn compress_puts_it_on_one_line() {
        let pretty = "{\n  \"b\": 1,\n  \"a\": [\n    1,\n    2\n  ],\n  \"s\": \"x y\"\n}\n";
        assert_eq!(
            restyle(pretty, COMPRESS).unwrap(),
            "{\"b\":1,\"a\":[1,2],\"s\":\"x y\"}\n"
        );
        // Format then Compress is back where it started.
        let line = r#"{"b":1,"a":[1,2]}"#;
        assert_eq!(
            restyle(&restyle(line, FORMAT).unwrap(), COMPRESS).unwrap(),
            line
        );
    }

    #[test]
    fn sort_orders_the_keys_of_nested_objects_too() {
        let text = r#"{"b":1,"a":[{"z":1,"y":{"d":4,"c":3}}],"A":true}"#;
        let sorted = JsonStyle {
            compress: true,
            sort: true,
        };
        assert_eq!(
            restyle(text, sorted).unwrap(),
            r#"{"A":true,"a":[{"y":{"c":3,"d":4},"z":1}],"b":1}"#
        );
        // Arrays keep their order.
        assert_eq!(restyle("[3,1,2]", sorted).unwrap(), "[3,1,2]");
        let pretty = restyle(
            r#"{"b":1,"a":[1,2]}"#,
            JsonStyle {
                compress: false,
                sort: true,
            },
        )
        .unwrap();
        assert_eq!(pretty, "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": 1\n}");
    }

    #[test]
    fn broken_json_says_line_and_column() {
        let Err(JsonStyleError::Broken(problem)) =
            restyle("{\n  \"a\": 1,\n  \"b\": 2,\n}\n", FORMAT)
        else {
            panic!("broken");
        };
        assert_eq!((problem.line, problem.column), (4, 1));
        assert_eq!(problem.message, "trailing comma");
        let Err(JsonStyleError::Broken(problem)) = restyle("[1, 2,\n 3 4]", FORMAT) else {
            panic!("broken");
        };
        assert_eq!((problem.line, problem.column), (2, 4));
        // Not meant as JSON at all: still where it stops.
        let Err(JsonStyleError::Broken(problem)) = restyle("{a: 1}", FORMAT) else {
            panic!("broken");
        };
        assert_eq!((problem.line, problem.column), (1, 2));
    }

    #[test]
    fn nothing_is_lost() {
        assert_eq!(
            restyle(r#"{"a":1,"a":2}"#, FORMAT),
            Err(JsonStyleError::DuplicateKeys)
        );
        assert_eq!(
            restyle(r#"{"a":{"k":1},"b":{"k":2}}"#, COMPRESS).unwrap(),
            r#"{"a":{"k":1},"b":{"k":2}}"#
        );
        // A key-like string in a value or array is not a key.
        assert!(restyle(r#"{"a":"b:c","l":["x", "y"]}"#, COMPRESS).is_ok());
        assert!(restyle(r#"{"a\"":1,"a":2}"#, COMPRESS).is_ok());
        assert_eq!(
            restyle("[123456789012345678901234567890]", FORMAT),
            Err(JsonStyleError::LossyNumbers)
        );
        assert_eq!(
            restyle("[0.12345678901234567890]", FORMAT),
            Err(JsonStyleError::LossyNumbers)
        );
        // Big integers that fit, and other spellings of the same value, are fine.
        assert_eq!(
            restyle(
                "[18446744073709551615,-9223372036854775808,1.50,1E3,-0.5e-2]",
                COMPRESS
            )
            .unwrap(),
            "[18446744073709551615,-9223372036854775808,1.5,1000.0,-0.005]"
        );
        assert_eq!(restyle(r#"["\u00e9\/"]"#, COMPRESS).unwrap(), "[\"é/\"]");
    }

    #[test]
    fn the_error_note_reads_like_the_meta_line() {
        let Err(err) = restyle("[1,]", FORMAT) else {
            panic!("broken");
        };
        assert!(err.note().contains('1'), "{}", err.note());
    }
}
