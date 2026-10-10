use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

/// YAML re-indented. A copy of several documents (`---`) stays several documents.
pub fn pretty_yaml(text: &str) -> Result<String, String> {
    let documents = parse_yaml_documents(text)?;
    let mut out = String::new();
    for (index, value) in documents.iter().enumerate() {
        if index > 0 || documents.len() > 1 {
            out.push_str("---\n");
        }
        out.push_str(&serde_yaml::to_string(value).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// YAML as pretty JSON (To JSON, and Convert on `key: value` text): anchors and aliases
/// expanded (`<<` merge keys too), keys that are numbers, booleans or null become strings,
/// dates stay the text they were copied as, and several documents (`---`) become an array.
pub fn yaml_to_json(text: &str) -> Option<String> {
    let mut documents = parse_yaml_documents(text).ok()?;
    for document in &mut documents {
        document.apply_merge().ok()?;
    }
    let json = if documents.len() == 1 {
        yaml_value_to_json(documents.pop()?)?
    } else {
        JsonValue::Array(
            documents
                .into_iter()
                .map(yaml_value_to_json)
                .collect::<Option<Vec<_>>>()?,
        )
    };
    serde_json::to_string_pretty(&json).ok()
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
    // `.inf` and `.nan` have no JSON number: they stay text.
    Some(
        serde_json::Number::from_f64(value)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(number.to_string())),
    )
}

fn yaml_key(key: YamlValue) -> Option<String> {
    match key {
        YamlValue::String(text) => Some(text),
        YamlValue::Bool(flag) => Some(flag.to_string()),
        YamlValue::Number(number) => Some(number.to_string()),
        YamlValue::Null => Some("null".into()),
        YamlValue::Tagged(tagged) => yaml_key(tagged.value),
        // A sequence or mapping as a key: its JSON text.
        complex => serde_json::to_string(&yaml_value_to_json(complex)?).ok(),
    }
}

pub fn looks_like_yaml(text: &str) -> bool {
    if looks_like_labeled_record(text) || looks_like_prose_pair(text) {
        return false;
    }
    parse_yaml_documents(text).is_ok() && parse_json(text).is_err()
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

/// One line of prose with a single colon (`Let op: dit werkt niet.`), not a one-pair mapping.
/// Only a lone `key: value` line is judged; anything longer is left to the YAML parser. A
/// pair is YAML when its key is a plain token (letters, digits, `_`, `-`, `.`) and its value
/// does not read as a sentence: three or more words, two or more words with a comma or a
/// closing `.`/`!`/`?`, or a clock time (`Time: 10:30`). Quoted values and flow collections
/// (`{...}`, `[...]`) are kept as YAML.
fn looks_like_prose_pair(text: &str) -> bool {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let (Some(line), None) = (lines.next(), lines.next()) else {
        return false;
    };
    let line = line.trim();
    let Some((key, value)) = line.split_once(':') else {
        return false;
    };
    if line.starts_with(['-', '{', '[', '#', '"', '\'', '!', '&', '*', '?']) {
        return false;
    }
    let value = value.trim();
    // `key:value` and `key:` are not the `key: value` shape of prose.
    if value.is_empty()
        || !key.is_empty() && !line[key.len() + 1..].starts_with(char::is_whitespace)
    {
        return false;
    }
    let plain_key = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !plain_key {
        return true;
    }
    if value.starts_with(['"', '\'', '[', '{', '|', '>', '&', '*', '!']) {
        return false;
    }
    let words = value.split_whitespace().count();
    let sentence_end = value.ends_with(['.', '!', '?']);
    let clock = value.split(':').all(|part| {
        !part.is_empty() && part.len() <= 2 && part.chars().all(|c| c.is_ascii_digit())
    }) && value.contains(':');
    words >= 3 || (words >= 2 && (sentence_end || value.contains(','))) || clock
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

/// The documents of a YAML copy (one, or several split by `---`), empty ones left out. Each
/// must be a mapping or a sequence.
fn parse_yaml_documents(text: &str) -> Result<Vec<YamlValue>, String> {
    let mut documents = Vec::new();
    for part in yaml_document_texts(text.trim()) {
        if part.trim().is_empty() {
            continue;
        }
        let value: YamlValue = serde_yaml::from_str(part).map_err(|e| format!("not YAML: {e}"))?;
        match value {
            YamlValue::Null => {}
            YamlValue::Mapping(_) | YamlValue::Sequence(_) | YamlValue::Tagged(_) => {
                documents.push(value)
            }
            _ => return Err("YAML must be a mapping or sequence".into()),
        }
    }
    if documents.is_empty() {
        return Err("no YAML document".into());
    }
    Ok(documents)
}

/// `text` split at its document markers: a line `---` (a comment may follow) or `...` at the
/// start of a line. Content on a `---` line (`--- !tag`, `--- value`) starts the document.
fn yaml_document_texts(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end();
        let marker = bare == "---"
            || bare == "..."
            || bare.starts_with("--- ")
            || bare.starts_with("---\t")
            || bare.starts_with("... ");
        if marker {
            parts.push(&text[start..offset]);
            // `--- {a: 1}`: what follows the marker belongs to the next document.
            let rest = if bare.starts_with("---") {
                3
            } else {
                line.len()
            };
            start = offset + rest.min(line.len());
        }
        offset += line.len();
    }
    parts.push(&text[start..]);
    parts
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
    fn single_pairs_are_yaml_but_prose_is_not() {
        for yaml in [
            "name: copycraft",
            "key: value",
            "version: 1.2",
            "port: 8080",
            "enabled: true",
            "image: nginx:1.25",
            "a.b-c_d: x",
            "title: \"Hello big world.\"",
            "tags: [a, b, c]",
            "- one\n- two\n",
            "items:\n  - one\n",
            "---\nname: x\n",
            "name: copycraft\nitems:\n  - one\n",
        ] {
            assert!(looks_like_yaml(yaml), "{yaml:?}");
        }
        for prose in [
            "Base64-afbeeldingen: die worden beschreven, niet getoond.",
            "Note: this is a test",
            "Let op: dit werkt niet.",
            "Time: 10:30",
            "Let op: klaar.",
            "Hi there: how are you?",
            "Note: yes, no",
            "Note: dit werkt niet",
        ] {
            assert!(!looks_like_yaml(prose), "{prose:?}");
        }
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

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(&yaml_to_json(text).expect("converts")).expect("JSON")
    }

    #[test]
    fn yaml_to_json_expands_anchors_aliases_and_merge_keys() {
        let src = "\
base: &base
  host: db.example.com
  port: 5432
dev:
  <<: *base
  name: dev
copy: *base
";
        let value = json(src);
        assert_eq!(value["dev"]["host"], "db.example.com");
        assert_eq!(value["dev"]["port"], 5432);
        assert_eq!(value["dev"]["name"], "dev");
        assert!(value["dev"].get("<<").is_none());
        assert_eq!(value["copy"]["port"], 5432);
    }

    #[test]
    fn yaml_to_json_keys_become_strings_and_dates_stay_text() {
        let value = json("1: one\ntrue: yes\nnull: nothing\n2.5: half\nwhen: 2026-01-02\n");
        assert_eq!(value["1"], "one");
        assert_eq!(value["true"], "yes");
        assert_eq!(value["null"], "nothing");
        assert_eq!(value["2.5"], "half");
        assert_eq!(value["when"], "2026-01-02");
        assert_eq!(json("[1, .inf]\n")[1], ".inf");
    }

    #[test]
    fn yaml_to_json_turns_documents_into_an_array() {
        let value = json("---\nname: a\n---\nname: b\n...\n");
        assert_eq!(value, serde_json::json!([{"name": "a"}, {"name": "b"}]));
        // One document, with or without a marker, stays itself.
        assert_eq!(json("---\nname: a\n"), serde_json::json!({"name": "a"}));
        assert!(looks_like_yaml("a: 1\n---\nb: 2\n"));
        let pretty = pretty_yaml("a:   1\n---\nb:   2\n").expect("pretty");
        assert_eq!(pretty, "---\na: 1\n---\nb: 2\n");
    }

    #[test]
    fn invalid_yaml_does_not_convert() {
        assert!(yaml_to_json("a: [1, 2\nb: 3\n").is_none());
        assert!(yaml_to_json("a: 1\n  b: 2\n").is_none());
        assert!(yaml_to_json("just words").is_none());
        assert!(!looks_like_yaml("a: [1, 2\nb: 3\n"));
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
