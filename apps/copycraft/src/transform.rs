use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

pub fn pretty_yaml(text: &str) -> Result<String, String> {
    let value = parse_yaml(text)?;
    serde_yaml::to_string(&value).map_err(|e| e.to_string())
}

pub fn yaml_to_json(text: &str) -> Option<String> {
    let value = parse_yaml(text).ok()?;
    serde_json::to_string_pretty(&yaml_value_to_json(value)?).ok()
}

fn yaml_value_to_json(value: YamlValue) -> Option<JsonValue> {
    Some(match value {
        YamlValue::Null => JsonValue::Null,
        YamlValue::Bool(flag) => JsonValue::Bool(flag),
        YamlValue::Number(number) => json_number(number)?,
        YamlValue::String(text) => JsonValue::String(text),
        YamlValue::Sequence(items) => JsonValue::Array(
            items
                .into_iter()
                .map(yaml_value_to_json)
                .collect::<Option<Vec<_>>>()?,
        ),
        YamlValue::Mapping(map) => {
            let mut object = serde_json::Map::new();
            for (key, nested) in map {
                object.insert(yaml_key(key)?, yaml_value_to_json(nested)?);
            }
            JsonValue::Object(object)
        }
        YamlValue::Tagged(tagged) => yaml_value_to_json(tagged.value)?,
    })
}

fn json_number(number: serde_yaml::Number) -> Option<JsonValue> {
    if let Some(value) = number.as_i64() {
        return Some(JsonValue::Number(value.into()));
    }
    if let Some(value) = number.as_u64() {
        return Some(JsonValue::Number(value.into()));
    }
    let value = number.as_f64()?;
    Some(JsonValue::Number(serde_json::Number::from_f64(value)?))
}

fn yaml_key(key: YamlValue) -> Option<String> {
    match key {
        YamlValue::String(text) => Some(text),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        YamlValue::Number(number) => Some(number.to_string()),
        YamlValue::Null => Some("null".into()),
        _ => None,
    }
}

pub fn looks_like_yaml(text: &str) -> bool {
    if looks_like_labeled_record(text) {
        return false;
    }
    parse_yaml(text).is_ok() && parse_json(text).is_err()
}

/// A flat contact card (`Naam: Jan`), not a YAML document.
/// A document marker, a list item, or an indented line is structure.
fn looks_like_labeled_record(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.len() < 2 || lines.iter().any(|line| yaml_structure_line(line)) {
        return false;
    }
    let labeled = lines
        .iter()
        .filter(|line| line_is_label_value(line.trim()))
        .count();
    labeled * 2 >= lines.len()
}

fn yaml_structure_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "---"
        || trimmed == "..."
        || trimmed.starts_with("- ")
        || line.starts_with([' ', '\t'])
}

fn line_is_label_value(line: &str) -> bool {
    let Some((label, value)) = line.split_once(':') else {
        return false;
    };
    let label = label.trim();
    let value = value.trim();
    !label.is_empty()
        && !value.is_empty()
        && !label.starts_with('-')
        && !label.contains('{')
        && label.chars().count() <= 40
        && !label.chars().any(|c| c == '[' || c == ']')
}

fn parse_json(text: &str) -> Result<JsonValue, String> {
    let value: JsonValue =
        serde_json::from_str(text.trim()).map_err(|e| format!("not JSON: {e}"))?;
    if !value.is_object() && !value.is_array() {
        return Err("JSON must be an object or array".into());
    }
    Ok(value)
}

fn parse_yaml(text: &str) -> Result<YamlValue, String> {
    let value: YamlValue =
        serde_yaml::from_str(text.trim()).map_err(|e| format!("not YAML: {e}"))?;
    match value {
        YamlValue::Mapping(_) | YamlValue::Sequence(_) => Ok(value),
        _ => Err("YAML must be a mapping or sequence".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{looks_like_yaml, pretty_yaml, yaml_to_json};

    #[test]
    fn detects_yaml_not_json() {
        assert!(looks_like_yaml("name: copycraft\nitems:\n  - one\n"));
        assert!(!looks_like_yaml("{\"a\":1}"));
    }

    #[test]
    fn pretty_yaml_keeps_keys() {
        let out = pretty_yaml("name:   copycraft").unwrap();
        assert!(out.contains("name"));
        assert!(out.contains("copycraft"));
    }

    #[test]
    fn labeled_personal_record_is_not_yaml() {
        let src = "Naam: [PERSON]
Adres: [LOCATION]
E-mailadres: [EMAIL_ADDRESS]
Telefoonnummer: [PHONE_NUMBER]
Geboortedatum: [DATE_TIME]
Salaris: [MONEY]";
        assert!(!looks_like_yaml(src));
    }

    #[test]
    fn yaml_to_json_pretty_prints_mapping() {
        let out = yaml_to_json("name: copycraft\ncount: 2\n").unwrap();
        assert!(out.contains("\"name\""));
        assert!(out.contains("copycraft"));
        assert!(out.contains('\n'));
    }

    #[test]
    fn song_document_is_yaml() {
        let src = "\
---
doe: \"a deer, a female deer\"
ray: \"a drop of golden sun\"
pi: 3.14159
xmas: true
french-hens: 3
calling-birds:
  - huey
  - dewey
  - louie
  - fred
xmas-fifth-day:
  calling-birds: four
  french-hens: 3
  golden-rings: 5
  partridges:
    count: 1
    location: \"a pear tree\"
  turtle-doves: two
";
        assert!(looks_like_yaml(src));
        let pretty = pretty_yaml(src).unwrap();
        assert!(pretty.contains("doe:"));
        assert!(pretty.contains("a deer, a female deer"));
        assert!(pretty.contains("huey"));
        assert!(pretty.contains("a pear tree"));

        let indented = "\
---
 doe: \"a deer, a female deer\"
 ray: \"a drop of golden sun\"
 pi: 3.14159
 xmas: true
 french-hens: 3
 calling-birds:
   - huey
   - dewey
   - louie
   - fred
 xmas-fifth-day:
   calling-birds: four
   french-hens: 3
   golden-rings: 5
   partridges:
     count: 1
     location: \"a pear tree\"
   turtle-doves: two
";
        assert!(looks_like_yaml(indented), "indented song");
    }

    #[test]
    fn blank_line_labeled_record_is_not_yaml() {
        let src = "Naam: Jan de Vries

Adres: Hoofdstraat 45, 9711 AB Groningen

E-mailadres: jan.devries@email.nl

Telefoonnummer: 06-12345678

Geboortedatum: 12 mei 1984

Salaris: € 3.450";
        assert!(!looks_like_yaml(src));
    }
}
