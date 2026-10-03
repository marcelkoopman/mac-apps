fn detect_separator(text: &str) -> Option<u8> {
    let header = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let semis = header.matches(';').count();
    let commas = header.matches(',').count();
    let tabs = header.matches('\t').count();
    if tabs > 0 && tabs >= semis && tabs >= commas {
        Some(b'\t')
    } else if semis >= 1 && semis >= commas {
        Some(b';')
    } else if commas >= 1 {
        Some(b',')
    } else {
        None
    }
}

fn looks_like_delimited_table(text: &str, separator: u8) -> bool {
    let sep = separator as char;
    // The first rows decide; a long copy is not split into lines here.
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(20)
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let width = delimited_field_count(lines[0], sep);
    if width < 2 || nonempty_delimited_field_count(lines[0], sep) < 2 {
        return false;
    }
    let sample_len = lines.len().min(20);
    let sample = &lines[..sample_len];
    let consistent = sample
        .iter()
        .filter(|line| delimited_field_count(line, sep) == width)
        .count();
    let populated = sample
        .iter()
        .filter(|line| nonempty_delimited_field_count(line, sep) >= 2)
        .count();
    consistent * 2 >= sample_len
        && populated * 2 >= sample_len
        && !looks_like_key_value_blob(text)
}

fn delimited_field_count(line: &str, sep: char) -> usize {
    delimited_fields(line, sep).count()
}

fn nonempty_delimited_field_count(line: &str, sep: char) -> usize {
    delimited_fields(line, sep)
        .filter(|field| !field.trim().is_empty())
        .count()
}

fn delimited_fields(line: &str, sep: char) -> impl Iterator<Item = &str> {
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek().is_some_and(|(_, next)| *next == '"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if ch == sep && !in_quotes {
            fields.push(&line[start..idx]);
            start = idx + ch.len_utf8();
        }
    }
    fields.push(&line[start..]);
    fields.into_iter()
}

fn looks_like_key_value_blob(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let labeled = lines
        .iter()
        .filter(|line| {
            line.split_once(':')
                .map(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty() && !k.contains(';'))
                .unwrap_or(false)
        })
        .count();
    labeled * 2 >= lines.len()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::try_format;
    use polars::prelude::{ParquetReader, SerReader};

    #[test]
    fn formats_semicolon_csv_with_trailing_delimiters() {
        let src = "\
Id;Naam;
1;Jan;
2;Anja;";
        assert!(super::looks_like_csv(src));
        let out = try_format(src).expect("df");
        assert!(out.contains("Naam"));
        assert!(out.contains("Jan"));
    }

    #[test]
    fn formats_semicolon_csv() {
        let src = "\
Id;Naam;Salaris
1;Jan;3450
2;Anja;2900";
        let out = try_format(src).expect("df");
        assert!(out.contains("Naam"));
        assert!(out.contains("Jan"));
        assert!(out.contains("3450"));
    }

    #[test]
    fn formats_comma_csv() {
        let src = "name,age\nalice,30\nbob,40";
        let out = try_format(src).expect("csv");
        assert!(out.contains("name"));
        assert!(out.contains("alice"));
        assert!(super::looks_like_csv(src));
        assert!(!super::looks_like_tsv(src));
    }

    #[test]
    fn formats_csv_with_quoted_commas() {
        let src = "\
Id,Naam,Geboortedatum,Adres,Telefoonnummer,Salaris
1,Jan de Vries,1984-05-12,\"Hoofdstraat 45, Groningen\",06-12345678,3450
2,Anja Bakker,1991-11-23,\"Kerkplein 2, Utrecht\",06-87654321,2900
3,Mohammed El Amin,1978-02-05,\"Stationstraat 120, Rotterdam\",06-11223344,4200";
        let out = try_format(src).expect("quoted csv");
        assert!(
            out.contains("Telefoonnummer")
                && out.contains("Salaris")
                && out.contains("Stationstraat 120, Rotterdam")
                && out.contains("Jan de Vries"),
            "{out}"
        );
        assert!(!out.contains('…'), "{out}");
        assert!(out.contains("Naam"));
        assert!(out.contains("Jan de Vries"));
        assert!(out.contains("Groningen"));
        assert!(super::looks_like_csv(src));
        assert_eq!(
            super::delimited_field_count(src.lines().nth(1).unwrap(), ','),
            6
        );
    }

    #[test]
    fn formats_tsv() {
        let src = "name\tage\nalice\t30\nbob\t40";
        let out = try_format(src).expect("tsv");
        assert!(out.contains("name"));
        assert!(out.contains("alice"));
        assert!(super::looks_like_tsv(src));
        assert!(!super::looks_like_csv(src));
    }

    #[test]
    fn formats_json_array() {
        let src = r#"[{"name":"a","n":1},{"name":"b","n":2}]"#;
        let out = try_format(src).expect("df");
        assert!(out.contains("name"));
        assert!(out.contains("a"));
    }

    #[test]
    fn display_env_lifts_the_row_column_and_width_limits() {
        for name in [
            "POLARS_FMT_MAX_ROWS",
            "POLARS_FMT_MAX_COLS",
            "POLARS_TABLE_WIDTH",
        ] {
            let value = super::DISPLAY_ENV
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value);
            assert_eq!(value, Some("-1"), "{name}");
        }
    }

    /// A synthetic look-alike of an energy export: a BOM, a "Definition:" line with semicolons
    /// above the header, dd/mm/yyyy dates, unit-suffixed columns, six all-0.00 columns and a
    /// product prefix shared by a group of columns. No real data.
    pub(crate) const ENERGY_FIXTURE: &str = include_str!("../tests/fixtures/energy_lookalike.csv");

    #[test]
    fn skips_the_definition_line_above_the_header() {
        let start = super::table_start(ENERGY_FIXTURE).expect("table");
        assert_eq!((start.header_line, start.skipped), (1, 1));
        assert_eq!(start.separator, b',');
        assert!(super::table_start(ENERGY_FIXTURE)
            .unwrap()
            .body(ENERGY_FIXTURE)
            .starts_with("Date,"));
        assert_eq!(start.note().as_deref(), Some("Header on line 2"));
        assert!(super::looks_like_csv(ENERGY_FIXTURE));
        assert_eq!(
            crate::format::detect(ENERGY_FIXTURE),
            crate::format::FormatKind::Csv
        );
        let df = super::parse(ENERGY_FIXTURE).expect("df");
        assert_eq!(df.shape(), (40, 20));
        assert_eq!(df.get_column_names()[0].as_str(), "Date");
        let floats = df
            .columns()
            .iter()
            .filter(|c| c.dtype() == &polars::prelude::DataType::Float64)
            .count();
        assert_eq!(floats, 19);
        assert!(super::try_parquet_bytes(ENERGY_FIXTURE).is_some());
        let csv = super::try_csv_text(ENERGY_FIXTURE).expect("csv");
        assert!(csv.starts_with("Date,"), "{}", &csv[..20]);
    }

    #[test]
    fn a_table_from_its_first_line_skips_nothing() {
        let src = "\n\nname,age\nalice,30\nbob,40";
        let start = super::table_start(src).expect("table");
        assert_eq!((start.header_line, start.skipped, start.offset), (2, 0, 2));
        assert_eq!(start.note(), None);
    }

    #[test]
    fn skips_a_title_and_a_blank_line_with_semicolons() {
        let src = "Export 2026\n\nId;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
        let start = super::table_start(src).expect("table");
        assert_eq!((start.header_line, start.skipped), (2, 1));
        assert_eq!(start.separator, b';');
        assert_eq!(super::parse(src).expect("df").shape(), (2, 3));
    }

    #[test]
    fn does_not_skip_into_json_code_or_wide_lines() {
        let json = "[\n{\"a\": 1, \"b\": 2, \"c\": 3},\n{\"a\": 4, \"b\": 5, \"c\": 6}\n]";
        assert_eq!(super::table_start(json), None);
        let df = super::parse(json).expect("json");
        assert_eq!(df.get_column_names().len(), 3);
        let rust = "fn main() {\n    call(a, b, c);\n    call(d, e, f);\n    call(g, h, i);\n}";
        assert_eq!(super::table_start(rust), None);
        assert!(!super::is_table(rust));
        // A line above as wide as the header would belong to the table.
        assert_eq!(super::table_start("a,b,c\nx,y\n1,2\n3,4"), None);
        let many = format!("{}name,age\nalice,30\nbob,40", "note\n".repeat(11));
        assert_eq!(super::table_start(&many), None);
        let ten = format!("{}name,age\nalice,30\nbob,40", "note\n".repeat(10));
        assert_eq!(super::table_start(&ten).map(|s| s.skipped), Some(10));
    }

    #[test]
    fn rejects_plain_text() {
        assert!(try_format("just a sentence about nothing").is_none());
        assert!(try_format("Naam: Jan de Vries\nSalaris: 3450").is_none());
    }

    #[test]
    fn rejects_rust_module_statements() {
        let src = "\
mod appearance;
mod clipboard;
mod compress;
mod dataframe;";
        assert!(!super::looks_like_csv(src));
        assert!(try_format(src).is_none());
    }

    #[test]
    fn exports_csv_as_json_rows() {
        let out = super::try_json_text("name,age\nalice,30\nbob,40").expect("json");
        assert!(out.contains("alice"));
        assert!(out.contains("name"));
        assert!(out.trim_start().starts_with('['));
    }

    #[test]
    fn exports_semicolon_csv() {
        let src = "\
Id;Naam;Salaris
1;Jan;3450
2;Anja;2900";
        let out = super::try_csv_text(src).expect("csv");
        assert!(out.contains("Naam"));
        assert!(out.contains("Jan"));
        assert!(out.contains(',') || out.contains(';'));
    }

    #[test]
    fn rejects_xml_as_dataframe() {
        let src = r#"
<root>
  <person><name>Jan</name><salary>3450</salary></person>
  <person><name>Anja</name><salary>2900</salary></person>
</root>"#;
        assert!(try_format(src).is_none());
        assert!(super::try_parquet_bytes(src).is_none());
    }

    #[test]
    fn parquet_roundtrip_keeps_rows() {
        for src in [
            "name,age\nalice,30\nbob,40",
            r#"[{"name":"alice","age":30},{"name":"bob","age":40}]"#,
        ] {
            let bytes = super::try_parquet_bytes(src).expect(src);
            assert!(bytes.starts_with(b"PAR1"), "{src}");
            assert!(bytes.ends_with(b"PAR1"), "{src}");
            let df = ParquetReader::new(std::io::Cursor::new(bytes))
                .finish()
                .expect(src);
            assert_eq!(df.height(), 2, "{src}");
            assert_eq!(df.width(), 2, "{src}");
        }
    }
}
