use crate::format::FormatKind;

pub fn try_convert(text: &str) -> Option<String> {
    let converted = match crate::format::detect(text) {
        FormatKind::Json => crate::transform::json_to_yaml(text)?,
        FormatKind::Yaml => crate::transform::yaml_to_json(text)?,
        FormatKind::Csv => crate::dataframe::try_json_text(text)?,
        FormatKind::Tsv | FormatKind::Dataframe => crate::dataframe::try_csv_text(text)?,
        // Flat YAML mappings are detected as text so they are not pretty-printed
        // as YAML (Dutch labeled records). Convert still maps them to JSON.
        FormatKind::Text | FormatKind::Plain => crate::transform::yaml_to_json(text)?,
        // XML keeps its own views (Original, Schema); flattening it into CSV made no sense.
        FormatKind::Xml
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
    fn json_object_converts_to_yaml() {
        let out = try_convert(r#"{"name":"copycraft","n":3}"#).expect("yaml");
        assert!(out.contains("name:"));
        assert!(out.contains("copycraft"));
        assert!(!out.trim_start().starts_with('{'));
        let back = try_convert(&out).expect("json");
        let value: JsonValue = serde_json::from_str(&back).expect("parse json");
        assert_eq!(value["name"], "copycraft");
        assert_eq!(value["n"], 3);
    }

    #[test]
    fn json_array_converts_to_yaml() {
        let out = try_convert(r#"[{"name":"a"},{"name":"b"}]"#).expect("yaml");
        assert!(out.contains("name:"));
        assert!(out.contains('a'));
        assert_eq!(crate::format::detect(&out), crate::format::FormatKind::Yaml);
    }

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
    fn json_yaml_roundtrip_preserves_object() {
        let src = r#"{"name":"copycraft","ok":true,"n":3}"#;
        let yaml = try_convert(src).expect("yaml");
        let json = try_convert(&yaml).expect("json");
        let a: JsonValue = serde_json::from_str(src).unwrap();
        let b: JsonValue = serde_json::from_str(&json).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn csv_converts_to_json_rows() {
        let out = try_convert("name,age\nalice,30\nbob,40").expect("json");
        let value: JsonValue = serde_json::from_str(&out).expect("parse json");
        let rows = value.as_array().expect("array");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "alice");
        assert_eq!(crate::format::detect(&out), crate::format::FormatKind::Json);
    }

    #[test]
    fn semicolon_csv_converts_to_json_rows() {
        let src = "\
Id;Naam;Salaris
1;Jan;3450
2;Anja;2900";
        let out = try_convert(src).expect("json");
        let value: JsonValue = serde_json::from_str(&out).expect("parse json");
        assert_eq!(value[0]["Naam"], "Jan");
        assert_eq!(value[1]["Salaris"], 2900);
    }

    #[test]
    fn tsv_converts_to_csv() {
        let out = try_convert("name\tage\nalice\t30\nbob\t40").expect("csv");
        assert!(out.contains("name"));
        assert!(out.contains("alice"));
        assert!(out.contains(','));
        assert!(!out.contains('\t'));
        assert_eq!(crate::format::detect(&out), crate::format::FormatKind::Csv);
    }

    #[test]
    fn quoted_csv_converts_address_cells() {
        let src = "\
Id,Naam,Adres
1,Jan de Vries,\"Hoofdstraat 45, Groningen\"";
        let out = try_convert(src).expect("json");
        let value: JsonValue = serde_json::from_str(&out).expect("parse json");
        assert_eq!(value[0]["Naam"], "Jan de Vries");
        assert_eq!(value[0]["Adres"], "Hoofdstraat 45, Groningen");
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
