use crate::format::FormatKind;

pub fn shows_format(source: &str) -> bool {
    crate::format::format_text(source) != source
}

pub fn shows_dataframe(kind: FormatKind) -> bool {
    matches!(
        kind,
        FormatKind::Csv | FormatKind::Tsv | FormatKind::Dataframe
    )
}

/// Dataframe is hidden for JSON and XML even when the text also parses as a table.
pub fn shows_dataframe_button(kind: FormatKind, source: &str) -> bool {
    kind != FormatKind::Xml
        && kind != FormatKind::Html
        && kind != FormatKind::Json
        && kind != FormatKind::Markdown
        && !crate::format::looks_like_xml(source)
        && crate::format::detect(source) != FormatKind::Json
        && crate::format::detect(source) != FormatKind::Markdown
        && (shows_dataframe(kind) || crate::dataframe::is_table(source))
}

pub fn shows_decode(kind: FormatKind) -> bool {
    matches!(kind, FormatKind::Plain | FormatKind::Text | FormatKind::Url)
}

/// Convert: flat `key: value` text to JSON. JSON, CSV and TSV have no conversion
/// ([`crate::convert::try_convert`]); YAML opens formatted.
pub fn shows_convert(source: &str) -> bool {
    crate::format::detect(source) != FormatKind::Yaml
        && crate::convert::try_convert(source).is_some()
}

#[cfg(test)]
mod tests {
    use super::{
        shows_convert, shows_dataframe, shows_dataframe_button, shows_decode, shows_format,
    };
    use crate::format::FormatKind;

    #[test]
    fn code_hides_dataframe_and_decode() {
        for kind in [FormatKind::Rust, FormatKind::Java] {
            assert!(!shows_dataframe(kind), "{kind:?}");
        }
        assert!(!shows_decode(FormatKind::Rust));
        assert!(!shows_decode(FormatKind::Java));
    }

    #[test]
    fn yaml_and_url_hide_dataframe() {
        for kind in [FormatKind::Yaml, FormatKind::Url] {
            assert!(!shows_dataframe(kind), "{kind:?}");
        }
        assert!(!shows_decode(FormatKind::Yaml));
        assert!(shows_decode(FormatKind::Url));
    }

    #[test]
    fn json_hides_convert_and_dataframe() {
        let object = r#"{"email":"jan.devries@email.nl"}"#;
        let table = r#"[{"name":"Jan","age":30},{"name":"Anja","age":40}]"#;
        assert!(!shows_dataframe(FormatKind::Json));
        assert!(!shows_dataframe_button(FormatKind::Json, table));
        assert!(!shows_dataframe_button(FormatKind::Csv, table));
        assert!(!shows_convert(object));
        assert!(!shows_convert("{\n  \"name\": \"copycraft\"\n}"));
        assert!(!shows_decode(FormatKind::Json));
    }

    #[test]
    fn xml_hides_dataframe() {
        assert!(!shows_dataframe(FormatKind::Xml));
        let tabular = "\
<root>
  <person><name>Jan</name><salary>3450</salary></person>
  <person><name>Anja</name><salary>2900</salary></person>
</root>";
        assert!(!shows_dataframe_button(FormatKind::Xml, tabular));
        assert!(!shows_dataframe_button(FormatKind::Csv, tabular));
        assert!(shows_dataframe_button(
            FormatKind::Csv,
            "name,age\nalice,30\nbob,40"
        ));
        assert!(!shows_decode(FormatKind::Xml));
    }

    #[test]
    fn table_and_prose() {
        assert!(shows_dataframe(FormatKind::Dataframe));
        assert!(!shows_decode(FormatKind::Dataframe));
        assert!(!shows_dataframe(FormatKind::Text));
        assert!(shows_decode(FormatKind::Text));
        assert!(shows_decode(FormatKind::Plain));
        assert!(shows_dataframe(FormatKind::Csv));
        assert!(shows_dataframe(FormatKind::Tsv));
        assert!(!shows_decode(FormatKind::Csv));
        assert!(!shows_decode(FormatKind::Tsv));
    }

    #[test]
    fn markdown_hides_format_convert_and_dataframe() {
        let notes = "# Notes\n\nmail me at jan.devries@email.nl\n";
        assert_eq!(crate::format::detect(notes), FormatKind::Markdown);
        assert!(!shows_dataframe(FormatKind::Markdown));
        assert!(!shows_dataframe_button(FormatKind::Markdown, notes));
        assert!(!shows_decode(FormatKind::Markdown));
        assert!(!shows_convert(notes));
        let messy = "# Title\nSome text\n- item\n";
        assert!(shows_format(messy));
        let tidy = "# Title\n\nSome text\n\n- item\n";
        assert!(!shows_format(tidy));
    }

    #[test]
    fn format_hides_when_output_matches_source() {
        assert!(!shows_format("hello world"));
        assert!(!shows_format("line one\nline two"));
        assert!(!shows_format("https://example.com/path"));
        assert!(!shows_format("https://example.com/search?q=hello%20world"));
        assert!(shows_format("example.com/search?q=hello world"));
        assert!(shows_format("www.example.com/a b"));
        assert!(!shows_format("name,age\nalice,30\nbob,40"));
        assert!(!shows_format("name\tage\nalice\t30\nbob\t40"));
        let pretty = "{\n  \"name\": \"copycraft\"\n}";
        assert!(!shows_format(pretty));
    }

    #[test]
    fn format_shows_when_pretty_print_changes_text() {
        assert!(shows_format(r#"{"name":"copycraft"}"#));
        assert!(shows_format("<root><item/></root>"));
    }

    #[test]
    fn convert_hides_for_yaml_and_tables() {
        let pretty = "{\n  \"name\": \"copycraft\"\n}";
        assert!(!shows_convert(pretty));
        assert!(!shows_format(pretty));
        assert!(!shows_convert("name: copycraft\nitems:\n  - one\n"));
        assert!(!shows_convert("name:   copycraft\nitems:\n  - one\n"));
        assert!(shows_format("name:   copycraft\nitems:\n  - one\n"));
        assert!(!shows_convert("name,age\nalice,30\nbob,40"));
        assert!(!shows_convert("name\tage\nalice\t30\nbob\t40"));
        assert!(!shows_convert(
            "\
Id;Naam;Salaris
1;Jan;3450
2;Anja;2900"
        ));
        assert!(!shows_convert(
            "<root><person><name>Jan</name><age>30</age></person><person><name>Anja</name><age>40</age></person></root>"
        ));
    }

    #[test]
    fn convert_hides_for_code_prose_and_xml() {
        assert!(!shows_convert("hello world"));
        assert!(!shows_convert("line one\nline two"));
        assert!(!shows_convert("https://example.com/path"));
        assert!(!shows_convert("fn main() {}"));
        assert!(!shows_convert(
            "mod appearance;\nmod clipboard;\nfn main() {}"
        ));
        assert!(!shows_convert("<root><item/></root>"));
    }

    #[test]
    fn image_hides_text_transforms() {
        assert!(!shows_decode(FormatKind::Image));
        assert!(!shows_dataframe(FormatKind::Image));
        assert!(!shows_format(""));
        assert!(!shows_convert(""));
    }
}
