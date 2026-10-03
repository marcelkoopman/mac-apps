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
                // A key with a separator in it is cells of a row before a time (`…,14:05`).
                .map(|(k, v)| {
                    !k.trim().is_empty()
                        && !v.trim().is_empty()
                        && !k.contains([';', ',', '\t'])
                })
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
    fn display_names_shorten_a_long_prefix_shared_per_group() {
        let names: Vec<String> = [
            "Date",
            "Sunbox 7 X1500 Max - PV1",
            "Sunbox 7 X1500 Max - PV2",
            "Sunbox 7 X1500 Max - PV3",
            "Heat pump unit: inlet",
            "Heat pump unit: outlet",
            "Heat pump unit: power",
            "Short - a",
            "Short - b",
            "Short - c",
            "Lone product name - X",
        ]
        .map(String::from)
        .to_vec();
        let shown = super::display_names(&names);
        assert_eq!(
            shown,
            [
                "Date",
                "… PV1",
                "… PV2",
                "… PV3",
                "… inlet",
                "… outlet",
                "… power",
                // Too short a prefix to shorten; one column alone is not a group.
                "Short - a",
                "Short - b",
                "Short - c",
                "Lone product name - X",
            ]
        );
        // Shortening that would make two names the same leaves them all as they are.
        let clash: Vec<String> = [
            "Product one long - Total",
            "Product one long - Peak",
            "Product one long - Low",
            "… Total",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(super::display_names(&clash), clash);
    }

    #[test]
    fn a_wide_table_has_a_column_overview() {
        let df = super::parse_table(ENERGY_FIXTURE).expect("table");
        let overview = super::overview(&df).expect("overview");
        // A list, not a polars grid: a count line, plain headings, aligned lines.
        let lines: Vec<&str> = overview.lines().collect();
        assert_eq!(lines[0], "20 columns · 40 rows");
        assert_eq!(lines[1], "");
        assert!(!overview.contains("shape:"), "{overview}");
        assert!(!overview.contains("---"), "{overview}");
        assert!(!overview.contains("str"), "{overview}");
        assert!(!overview.contains('│'), "{overview}");
        let headings = lines[2];
        assert!(headings.starts_with("column "), "{headings}");
        let type_at = headings.find("type").expect("type");
        let values_at = headings.find("values").expect("values");
        assert_eq!(lines.len(), 3 + 20);
        let date = lines[3];
        assert!(date.starts_with("Date "), "{date}");
        assert_eq!(&date[type_at..type_at + 4], "date");
        assert_eq!(&date[values_at..], "2026-03-09 – 2026-04-22");
        let pv4 = lines
            .iter()
            .find(|line| line.starts_with("… PV4 Generation (kWh)"))
            .expect("PV4");
        let chars: Vec<char> = pv4.chars().collect();
        let cell = |from: usize| chars[from..].iter().collect::<String>();
        assert!(cell(type_at).starts_with("number "), "{pv4}");
        assert_eq!(cell(values_at), "always 0.0");
        // A narrow table has one too; it opens on its grid.
        let narrow = super::parse_table("a,b\n1,2").expect("table");
        assert!(super::overview(&narrow).is_some());
        assert!(!super::shows_overview(None, 2) && super::shows_overview(None, 7));
        assert!(super::shows_overview(Some(true), 2) && !super::shows_overview(Some(false), 20));
        assert!(!super::shows_overview(Some(true), 0));
    }

    #[test]
    fn the_overview_sums_up_each_column_by_type() {
        use polars::prelude::{AnyValue, Column, DataType, TimeUnit};
        let df = super::parse_table(ENERGY_FIXTURE).expect("table");
        let summary = |name: &str| super::values_summary(df.column(name).expect("column"));
        let kind = |name: &str| super::friendly_type(df.column(name).expect("column").dtype());
        // Dates: first – last. Numbers: min – max, compact. All-0.00 columns: always 0.0.
        assert_eq!(kind("Date"), "date");
        assert_eq!(summary("Date"), "2026-03-09 – 2026-04-22");
        assert_eq!(kind("Home Usage (kWh)"), "number");
        assert_eq!(summary("Home Usage (kWh)"), "0.0 – 11.92");
        assert_eq!(summary("Grid Import (kWh)"), "0.32 – 11.79");
        assert_eq!(summary("Battery Discharge (kWh)"), "0.5 – 11.74");
        for zero in ["Grid Export (kWh)", "Sunbox 7 X1500 Max - PV4 Generation (kWh)"] {
            assert_eq!(summary(zero), "always 0.0", "{zero}");
        }
        let overview = super::overview(&df).expect("overview");
        assert!(!overview.contains("examples"), "{overview}");
        assert!(!overview.contains("f64"), "{overview}");

        // Whole numbers, text, yes/no and empty cells.
        let df = super::parse_table(concat!(
            "id,name,city,active,score,note\n",
            "1,Jan,Utrecht,true,3,x\n",
            "2,Anja,Utrecht,false,,x\n",
            "3,Piet,Amsterdam aan de Amstel en verder,true,5,x\n",
            "4,Kees,Utrecht,true,,\n",
        ))
        .expect("table");
        let summary = |name: &str| super::values_summary(df.column(name).expect("column"));
        let kind = |name: &str| super::friendly_type(df.column(name).expect("column").dtype());
        assert_eq!(kind("id"), "whole number");
        assert_eq!(summary("id"), "1 – 4");
        assert_eq!(summary("score"), "3 – 5 · 2 empty");
        assert_eq!(kind("name"), "text");
        // Every value once: no most common one.
        assert_eq!(summary("name"), "4 distinct");
        assert_eq!(summary("city"), "2 distinct · most common: Utrecht");
        assert_eq!(summary("note"), "always x · 1 empty");
        assert_eq!(kind("active"), "yes/no");
        assert_eq!(summary("active"), "true 3 · false 1");
        // A long most common value is cut to 20 characters, the last one "…".
        let long = super::parse_table(
            "k,v\n1,Amsterdam aan de Amstel en verder\n2,Amsterdam aan de Amstel en verder\n3,b",
        )
        .expect("table");
        assert_eq!(
            super::values_summary(long.column("v").expect("column")),
            "2 distinct · most common: Amsterdam aan de Am…"
        );
        // Datetimes: first – last, no zero fraction.
        let stamps = Column::new("t".into(), [1_774_000_000_000_i64, 1_774_086_400_000])
            .cast(&DataType::Datetime(TimeUnit::Milliseconds, None))
            .expect("cast");
        assert_eq!(super::friendly_type(stamps.dtype()), "datetime");
        let text = super::values_summary(&stamps);
        assert!(text.starts_with("2026-03-20 "), "{text}");
        assert!(text.contains(" – 2026-03-21 "), "{text}");
        assert!(!text.contains('.'), "{text}");
        // Small and large numbers stay short.
        assert_eq!(super::number_text(&AnyValue::Float64(1.0 / 3.0)).as_deref(), Some("0.3333"));
        assert_eq!(super::number_text(&AnyValue::Float64(12.0)).as_deref(), Some("12.0"));
        assert_eq!(super::number_text(&AnyValue::Float64(2e20)).as_deref(), Some("2.000e20"));
    }

    #[test]
    fn a_table_version_renders_writes_and_saves_from_its_frame() {
        let df = super::parse_table("name,n\na,1\nb,2\nc,3").expect("table");
        let preview = super::frame_preview(&df, 2, None).expect("preview");
        assert_eq!((preview.rows, preview.shown_rows), (3, 2));
        assert!(preview.grid.contains("shape: (2, 2)"), "{}", preview.grid);
        assert!(super::frame_grid(&df).expect("grid").contains("shape: (3, 2)"));
        assert_eq!(super::frame_csv(&df).as_deref(), Some("name,n\na,1\nb,2\nc,3\n"));
        let parquet = super::frame_parquet(&df).expect("parquet");
        let back = ParquetReader::new(std::io::Cursor::new(parquet))
            .finish()
            .expect("read");
        assert!(back.equals(&df));
    }

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
        let csv = super::frame_csv(&df).expect("csv");
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
    fn guesses_the_date_order_from_days_over_12() {
        use super::{DateGuess, DateOrder, date_guess};
        let day_first = DateGuess {
            separator: '/',
            order: Some(DateOrder::DayFirst),
        };
        assert_eq!(date_guess(["09/03/2026", "", "22/04/2026"]), Some(day_first));
        assert_eq!(
            date_guess(["03/22/2026", "4/1/2026"]).and_then(|g| g.order),
            Some(DateOrder::MonthFirst)
        );
        let ambiguous = date_guess(["01.02.2024", "03.04.2024"]).expect("dates");
        assert_eq!((ambiguous.separator, ambiguous.order), ('.', None));
        // Both orders fail, or it is not a date column.
        assert_eq!(date_guess(["13/01/2024", "01/13/2024"]), None);
        assert_eq!(date_guess(["01/02/24"]), None);
        assert_eq!(date_guess(["01/02/2024", "01-03-2024"]), None);
        assert_eq!(date_guess(["2024-01-02"]), None);
        assert_eq!(date_guess(["1.5", "2.25"]), None);
        assert_eq!(date_guess(["32/01/2024"]), None);
        assert_eq!(date_guess([""]), None);
    }

    #[test]
    fn a_value_that_forces_month_first_is_said_so_and_a_mixed_column_stays_text() {
        use polars::prelude::DataType;
        let src = "when,amount\n01/13/2026,1\n02/03/2026,2";
        let preview = super::try_format_preview(src, 200, None).expect("preview");
        assert!(preview.dates[0].forced_month_first());
        assert!(preview.grid.contains("2026-02-03"), "{}", preview.grid);
        assert_eq!(
            super::dates_note(&preview.dates).as_deref(),
            Some("Dates read as mm/dd/yyyy")
        );
        let (_, notes) = super::parse_table_with(src, Default::default()).expect("table");
        assert!(notes.forced_month_first && !notes.ambiguous_dates);
        assert_eq!(notes.meta_notes(), ["Dates read as mm/dd/yyyy"]);
        // Day first, forced or not, goes unsaid.
        let day_first = super::parse_table_with("when,n\n13/01/2026,1\n02/03/2026,2", Default::default());
        assert!(day_first.expect("table").1.meta_notes().is_empty());
        // One value only fits dd/mm, another only mm/dd: the column stays text.
        let mixed = "when,amount\n13/01/2026,1\n01/14/2026,2\n02/03/2026,3";
        let (df, notes) = super::parse_table_with(mixed, Default::default()).expect("table");
        assert_eq!(df.column("when").expect("when").dtype(), &DataType::String);
        assert!(notes.meta_notes().is_empty());
    }

    #[test]
    fn the_frozen_column_is_the_first_column_of_every_grid_line() {
        let preview = super::try_format_preview("fruit,color\napple,red\nbanana,yellow", 10, None)
            .expect("preview");
        let grid = &preview.grid;
        let lines = super::frozen_column(grid).expect("frozen");
        let utf16: Vec<u16> = grid.encode_utf16().collect();
        let part = |line: &super::FrozenLine| String::from_utf16(&utf16[line.start..line.end]).unwrap();
        let parts: Vec<String> = lines.iter().map(part).collect();
        assert_eq!(parts.len(), grid.lines().count());
        assert_eq!(parts[0], "", "the shape line scrolls");
        assert!(parts[1].starts_with('┌') && parts[1].ends_with('┬'), "{parts:?}");
        assert!(parts.iter().any(|p| p == "│ fruit  ┆"), "{parts:?}");
        assert!(parts.iter().any(|p| p == "│ banana ┆"), "{parts:?}");
        assert!(parts.last().unwrap().ends_with('┴'));
        // Every line but the last has its newline right after it.
        for line in &lines[..lines.len() - 1] {
            assert_eq!(utf16[line.newline.expect("newline")], u16::from(b'\n'));
        }
        assert_eq!(lines.last().unwrap().newline, None);
        // One column has nothing to keep in view; neither has text that is no grid.
        let one = polars::prelude::df!("fruit" => ["apple", "banana"]).expect("frame");
        let one = super::frame_preview(&one, 10, None).expect("preview");
        assert_eq!(super::frozen_column(&one.grid), None);
        assert_eq!(super::frozen_column("just words"), None);
    }

    #[test]
    fn reads_day_first_dates_as_dates() {
        use polars::prelude::DataType;
        let preview = super::try_format_preview(ENERGY_FIXTURE, 200, None).expect("preview");
        assert_eq!(preview.dates.len(), 1);
        assert!(!preview.dates[0].ambiguous);
        assert_eq!(preview.dates[0].order, super::DateOrder::DayFirst);
        assert!(preview.grid.contains("2026-03-09"), "{}", preview.grid);
        assert_eq!(super::dates_note(&preview.dates), None);
        let bytes = super::try_parquet_bytes(ENERGY_FIXTURE).expect("parquet");
        let df = ParquetReader::new(std::io::Cursor::new(bytes))
            .finish()
            .expect("read");
        assert_eq!(df.columns()[0].dtype(), &DataType::Date);
    }

    #[test]
    fn ambiguous_dates_are_read_day_first_and_said_so() {
        let src = "when,amount\n01/02/2024,1\n03/04/2024,2";
        let preview = super::try_format_preview(src, 200, None).expect("preview");
        assert!(preview.dates[0].ambiguous);
        assert!(preview.grid.contains("2024-02-01"), "{}", preview.grid);
        assert_eq!(
            super::dates_note(&preview.dates).as_deref(),
            Some("Dates read as dd/mm/yyyy")
        );
        // 31 February is no date: the column stays text.
        let bad = super::try_format_preview("when,amount\n31/02/2024,1\n15/03/2024,2", 200, None)
            .expect("preview");
        assert!(bad.dates.is_empty());
        assert!(bad.grid.contains("31/02/2024"));
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
    fn exports_semicolon_csv() {
        let src = "\
Id;Naam;Salaris
1;Jan;3450
2;Anja;2900";
        let out = super::frame_csv(&super::parse(src).expect("df")).expect("csv");
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

    fn names(df: &polars::prelude::DataFrame) -> Vec<String> {
        df.get_column_names()
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    /// Save in the Dataframe view: CSV of the table as shown, read back the same.
    fn assert_csv_round_trip(df: &polars::prelude::DataFrame) -> String {
        use polars::prelude::DataType;
        let bytes = super::TableFile::Csv.bytes(df).expect("csv");
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "no BOM");
        let csv = String::from_utf8(bytes).expect("UTF-8");
        let header = csv.lines().next().expect("header");
        let back = super::parse_table(&csv).expect("read back");
        assert_eq!(back.shape(), df.shape());
        assert_eq!(names(&back), names(df));
        assert!(header.contains(','), "{header}");
        // Dates come back as dates (`yyyy-mm-dd`), datetimes as datetimes, without Fix types.
        for (column, read) in df.columns().iter().zip(back.columns()) {
            if matches!(column.dtype(), DataType::Date | DataType::Datetime(_, _)) {
                assert_eq!(read.dtype(), column.dtype(), "{}", column.name());
            }
        }
        csv
    }

    #[test]
    fn a_table_saves_as_csv_with_its_names_and_iso_dates() {
        let df = super::parse_table(ENERGY_FIXTURE).expect("table");
        let csv = assert_csv_round_trip(&df);
        let mut lines = csv.lines();
        // The real column names, also the long shared-prefix ones the card shortens.
        assert_eq!(lines.next(), Some(names(&df).join(",").as_str()));
        assert!(lines.next().is_some_and(|row| row.starts_with("2026-03-09,")));
        // A version: its own columns and rows.
        let version = crate::table_ops::drop_constant(&df).expect("step");
        assert!(version.width() < df.width());
        assert_csv_round_trip(&version);
        // A tab or `;` source still saves comma separated; a name or value with a comma is quoted.
        let tsv = "Naam\tPlaats, land\tWanneer\nJan\tDen Haag, NL\t13/03/2026\nPiet\tUtrecht\t14/03/2026";
        let df = super::parse_table(tsv).expect("tsv");
        let csv = assert_csv_round_trip(&df);
        assert!(csv.starts_with("Naam,\"Plaats, land\",Wanneer\n"), "{csv}");
        assert!(csv.contains("Jan,\"Den Haag, NL\",2026-03-13"), "{csv}");
        let semis = "id;prijs\n1;2,50\n2;3,75";
        assert_csv_round_trip(&super::parse_table(semis).expect("semis"));
    }

    #[test]
    fn parquet_stays_a_choice() {
        let df = super::parse_table(ENERGY_FIXTURE).expect("table");
        let bytes = super::TableFile::Parquet.bytes(&df).expect("parquet");
        let back = ParquetReader::new(std::io::Cursor::new(bytes))
            .finish()
            .expect("read");
        assert_eq!(back.shape(), df.shape());
        assert_eq!(super::TableFile::ALL[0], super::TableFile::Csv);
        assert_eq!(super::TableFile::Csv.extension(), "csv");
        assert_eq!(super::TableFile::Parquet.extension(), "parquet");
    }

    #[test]
    fn iso_dates_and_datetimes_are_read_as_such() {
        use polars::prelude::{DataType, TimeUnit};
        let src = "day,at,stamp,n\n\
                   2026-03-09,2026-03-09T14:05,2026-03-09 14:05:30.25,1\n\
                   ,2026-03-10T00:00:59,2024-02-29 23:59:59,2\n\
                   2026-12-31,2026-03-11T09:30:00.123456789,2026-03-11 09:30,3";
        let df = super::parse_table(src).expect("table");
        let dtype = |name: &str| df.column(name).expect(name).dtype().clone();
        assert_eq!(dtype("day"), DataType::Date);
        assert_eq!(dtype("at"), DataType::Datetime(TimeUnit::Microseconds, None));
        assert_eq!(dtype("stamp"), DataType::Datetime(TimeUnit::Microseconds, None));
        assert_eq!(df.column("day").expect("day").null_count(), 1);
        let shown = super::frame_preview(&df, 10, Some(false)).expect("preview").grid;
        assert!(shown.contains("2026-03-09 14:05:30.250"), "{shown}");
        assert!(shown.contains("2024-02-29 23:59:59"), "{shown}");
        // The card's view of the copied text reads them the same way.
        let preview = super::try_format_preview(src, 10, Some(true)).expect("preview");
        assert!(preview.overview.as_deref().is_some_and(|o| o.contains("datetime")));
        // A saved CSV reads back the same.
        assert_csv_round_trip(&df);
        // One value that is not one (or does not exist), or dates mixed with datetimes: text.
        for odd in [
            "day,n\n2026-03-09,1\n2026-02-30,1",
            "day,n\n2026-03-09,1\nsoon,1",
            "day,n\n2026-03-09,1\n2026-03-09T10:00,1",
            "at,n\n2026-03-09T24:00,1\n,2\n,3",
            "at,n\n2026-03-09T10:00+02:00,1\n,2\n,3",
            "at,n\n2026-03-09T10:0,1\n,2\n,3",
            "day,n\n2026-3-9,1\n,2\n,3",
        ] {
            let df = super::parse_table(odd).expect(odd);
            assert_eq!(df.columns()[0].dtype(), &DataType::String, "{odd}");
        }
        // dd/mm/yyyy dates and the question when both orders fit are as before.
        let both = "when,iso\n01/02/2024,2024-02-01\n03/04/2024,2024-04-03";
        let (df, notes) = super::parse_table_with(both, super::ReadOptions::default()).expect("table");
        assert_eq!(df.column("when").expect("when").dtype(), &DataType::Date);
        assert_eq!(df.column("iso").expect("iso").dtype(), &DataType::Date);
        assert!(notes.ambiguous_dates);
        assert_eq!(notes.meta_notes(), ["Dates read as dd/mm/yyyy"]);
    }

    #[test]
    fn iso_day_numbers_are_the_calendar_ones() {
        assert_eq!(super::iso_days("1970-01-01"), Some(0));
        assert_eq!(super::iso_days("2000-03-01"), Some(11_017));
        assert_eq!(super::iso_days("1969-12-31"), Some(-1));
        assert_eq!(super::iso_days("2024-02-29"), Some(19_782));
        assert_eq!(super::iso_days("2023-02-29"), None);
        assert_eq!(super::iso_days("1900-02-29"), None);
        assert_eq!(super::iso_days("2000-02-29"), Some(11_016));
        assert_eq!(super::iso_micros("1970-01-01T00:00:01.5"), Some(1_500_000));
        assert_eq!(super::iso_micros("1970-01-02 00:00"), Some(86_400_000_000));
        assert_eq!(super::iso_micros("1970-01-01T00:00."), None);
    }
}
