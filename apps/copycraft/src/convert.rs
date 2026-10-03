use crate::format::FormatKind;

/// The Convert chip's result: flat `key: value` text (and YAML) as pretty JSON. JSON, CSV and
/// TSV have no conversion: they open formatted, or as a table (Dataframe; Save writes CSV).
pub fn try_convert(text: &str) -> Option<String> {
    let converted = match crate::format::detect(text) {
        FormatKind::Yaml => crate::transform::yaml_to_json(text)?,
        // Flat YAML mappings are detected as text so they are not pretty-printed
        // as YAML (Dutch labeled records). Convert still maps them to JSON.
        FormatKind::Text | FormatKind::Plain => crate::transform::yaml_to_json(text)?,
        FormatKind::Json
        | FormatKind::Csv
        | FormatKind::Tsv
        | FormatKind::Dataframe
        | FormatKind::Xml
        | FormatKind::Html
        | FormatKind::Rust
        | FormatKind::Java
        | FormatKind::Python
        | FormatKind::Markdown
        | FormatKind::Url
        | FormatKind::Image => {
            return None;
        }
    };
    if converted == text {
        None
    } else {
        Some(converted)
    }
}

#[cfg(test)]
mod tests {
    use super::try_convert;
    use serde_json::Value as JsonValue;

    #[test]
    fn yaml_converts_to_pretty_json() {
        let src = "name: copycraft\nitems:\n  - one\n  - two\n";
        let out = try_convert(src).expect("json");
        let value: JsonValue = serde_json::from_str(&out).expect("parse json");
        assert_eq!(value["name"], "copycraft");
        assert_eq!(value["items"][0], "one");
        assert!(out.contains('\n'));
        assert_eq!(crate::format::detect(&out), crate::format::FormatKind::Json);
    }

    #[test]
    fn json_csv_and_tsv_do_not_convert() {
        assert!(try_convert(r#"{"name":"copycraft","n":3}"#).is_none());
        assert!(try_convert(r#"[{"name":"a"},{"name":"b"}]"#).is_none());
        assert!(try_convert("name,age\nalice,30\nbob,40").is_none());
        assert!(try_convert("Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900").is_none());
        assert!(try_convert("name\tage\nalice\t30\nbob\t40").is_none());
    }

    #[test]
    fn plain_text_and_code_do_not_convert() {
        assert!(try_convert("hello world").is_none());
        assert!(try_convert("fn main() {}").is_none());
        assert!(try_convert("https://example.com/x").is_none());
        assert!(try_convert("<root><item/></root>").is_none());
        let tabular_xml = "\
<root>
  <person><name>Jan</name><salary>3450</salary></person>
  <person><name>Anja</name><salary>2900</salary></person>
</root>";
        assert!(try_convert(tabular_xml).is_none());
        assert!(try_convert("mod appearance;\nmod clipboard;\nmod compress;").is_none());
    }

    #[test]
    fn labeled_record_converts_to_json() {
        let src = "\
Naam: Jan de Vries
Adres: Hoofdstraat 45, 9711 AB Groningen
E-mailadres: jan.devries@email.nl";
        let out = try_convert(src).expect("json");
        let value: JsonValue = serde_json::from_str(&out).expect("parse json");
        assert_eq!(value["Naam"], "Jan de Vries");
        assert_eq!(value["E-mailadres"], "jan.devries@email.nl");
        assert_eq!(crate::format::detect(&out), crate::format::FormatKind::Json);
    }
}
