use std::collections::HashSet;

use serde_json::{Map, Value};

/// Pretty-printed Apache Avro schema inferred from a JSON object or array.
pub fn try_schema(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    if !value.is_object() && !value.is_array() {
        return None;
    }
    let rendered = render(infer(&value, "document"), &mut HashSet::new());
    serde_json::to_string_pretty(&rendered).ok()
}

#[derive(Debug, PartialEq, Eq)]
enum Schema {
    /// Item type of an empty array: no values have been seen yet.
    Unknown,
    Null,
    Boolean,
    Int,
    Long,
    Double,
    String,
    Array(Box<Schema>),
    Record(Record),
    Union(Vec<Schema>),
}

#[derive(Debug, PartialEq, Eq)]
struct Record {
    name: String,
    fields: Vec<Field>,
    /// Objects merged into this record. A field seen fewer times is optional.
    samples: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct Field {
    name: String,
    ty: Schema,
    present: u32,
}

fn infer(value: &Value, record_name: &str) -> Schema {
    match value {
        Value::Null => Schema::Null,
        Value::Bool(_) => Schema::Boolean,
        Value::Number(number) => number_ty(number),
        Value::String(_) => Schema::String,
        Value::Array(items) => infer_array(items, record_name),
        Value::Object(map) => infer_object(map, record_name),
    }
}

fn number_ty(number: &serde_json::Number) -> Schema {
    if number.is_f64() {
        return Schema::Double;
    }
    match number.as_i64() {
        Some(value) if i32::try_from(value).is_ok() => Schema::Int,
        Some(_) => Schema::Long,
        None => Schema::Double,
    }
}

fn infer_array(items: &[Value], record_name: &str) -> Schema {
    let Some(first) = items.first() else {
        return Schema::Array(Box::new(Schema::Unknown));
    };
    let mut acc = infer(first, record_name);
    for item in items.iter().skip(1) {
        acc = merge(acc, infer(item, record_name));
    }
    Schema::Array(Box::new(acc))
}

fn infer_object(map: &Map<String, Value>, record_name: &str) -> Schema {
    let mut used = HashSet::new();
    let mut fields = Vec::new();
    for (key, value) in map {
        let name = unique_name(&sanitize(key), &mut used);
        let nested = format!("{record_name}_{name}");
        fields.push(Field {
            name,
            ty: infer(value, &nested),
            present: 1,
        });
    }
    Schema::Record(Record {
        name: record_name.to_string(),
        fields,
        samples: 1,
    })
}

/// Avro names are `[A-Za-z_][A-Za-z0-9_]*`. Anything else becomes `_`.
fn sanitize(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "field".to_string();
    }
    let first = out.chars().next().unwrap_or('_');
    if first.is_ascii_alphabetic() || first == '_' {
        out
    } else {
        format!("_{out}")
    }
}

fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n = n.saturating_add(1);
    }
}

fn merge(left: Schema, right: Schema) -> Schema {
    match (left, right) {
        (Schema::Unknown, other) | (other, Schema::Unknown) => other,
        (Schema::Int, Schema::Long) | (Schema::Long, Schema::Int) => Schema::Long,
        (Schema::Int | Schema::Long, Schema::Double)
        | (Schema::Double, Schema::Int | Schema::Long) => Schema::Double,
        (Schema::Array(left), Schema::Array(right)) => {
            Schema::Array(Box::new(merge(*left, *right)))
        }
        (Schema::Record(left), Schema::Record(right)) if left.name == right.name => {
            Schema::Record(merge_records(left, right))
        }
        (Schema::Union(items), other) => fold_into_union(items, other),
        (other, Schema::Union(items)) => fold_into_union(items, other),
        (left, right) if left == right => left,
        (left, right) => union_of(left, right),
    }
}

fn merge_records(mut left: Record, right: Record) -> Record {
    for field in right.fields {
        if let Some(existing) = left.fields.iter_mut().find(|item| item.name == field.name) {
            let current = std::mem::replace(&mut existing.ty, Schema::Unknown);
            existing.ty = merge(current, field.ty);
            existing.present = existing.present.saturating_add(field.present);
        } else {
            left.fields.push(field);
        }
    }
    left.samples = left.samples.saturating_add(right.samples);
    left
}

fn union_of(left: Schema, right: Schema) -> Schema {
    let mut items = Vec::new();
    push_branch(&mut items, left);
    push_branch(&mut items, right);
    finish_union(items)
}

fn fold_into_union(items: Vec<Schema>, extra: Schema) -> Schema {
    let mut acc = Vec::new();
    for item in items {
        push_branch(&mut acc, item);
    }
    push_branch(&mut acc, extra);
    finish_union(acc)
}

fn push_branch(items: &mut Vec<Schema>, ty: Schema) {
    match ty {
        Schema::Unknown => {}
        Schema::Union(branches) => {
            for branch in branches {
                push_branch(items, branch);
            }
        }
        other => {
            if let Some(index) = items.iter().position(|item| folds(item, &other)) {
                let current = items.remove(index);
                push_branch(items, merge(current, other));
            } else {
                items.push(other);
            }
        }
    }
}

fn folds(left: &Schema, right: &Schema) -> bool {
    match (left, right) {
        (Schema::Int | Schema::Long, Schema::Int | Schema::Long | Schema::Double) => true,
        (Schema::Double, Schema::Int | Schema::Long | Schema::Double) => true,
        (Schema::Array(_), Schema::Array(_)) => true,
        (Schema::Record(left), Schema::Record(right)) => left.name == right.name,
        (Schema::Null, Schema::Null)
        | (Schema::Boolean, Schema::Boolean)
        | (Schema::String, Schema::String) => true,
        _ => false,
    }
}

fn finish_union(mut items: Vec<Schema>) -> Schema {
    items.sort_by_key(union_rank);
    if items.len() == 1 {
        return items.pop().unwrap_or(Schema::Null);
    }
    if items.is_empty() {
        Schema::Null
    } else {
        Schema::Union(items)
    }
}

fn union_rank(ty: &Schema) -> u8 {
    match ty {
        Schema::Null | Schema::Unknown => 0,
        Schema::Boolean => 1,
        Schema::Int => 2,
        Schema::Long => 3,
        Schema::Double => 4,
        Schema::String => 5,
        Schema::Array(_) => 6,
        Schema::Record(_) => 7,
        Schema::Union(_) => 8,
    }
}

fn render(ty: Schema, used: &mut HashSet<String>) -> Value {
    match ty {
        Schema::Null | Schema::Unknown => Value::String("null".into()),
        Schema::Boolean => Value::String("boolean".into()),
        Schema::Int => Value::String("int".into()),
        Schema::Long => Value::String("long".into()),
        Schema::Double => Value::String("double".into()),
        Schema::String => Value::String("string".into()),
        Schema::Array(items) => {
            let mut obj = Map::new();
            obj.insert("type".into(), Value::String("array".into()));
            obj.insert("items".into(), render(*items, used));
            Value::Object(obj)
        }
        Schema::Record(record) => render_record(record, used),
        Schema::Union(items) => {
            let mut branches = Vec::with_capacity(items.len());
            for item in items {
                branches.push(render(item, used));
            }
            Value::Array(branches)
        }
    }
}

fn render_record(record: Record, used: &mut HashSet<String>) -> Value {
    let name = unique_name(&record.name, used);
    let samples = record.samples;
    let mut fields = Vec::with_capacity(record.fields.len());
    for field in record.fields {
        fields.push(render_field(field, samples, used));
    }
    let mut obj = Map::new();
    obj.insert("type".into(), Value::String("record".into()));
    obj.insert("name".into(), Value::String(name));
    obj.insert("fields".into(), Value::Array(fields));
    Value::Object(obj)
}

fn render_field(field: Field, samples: u32, used: &mut HashSet<String>) -> Value {
    let missing = field.present < samples;
    let ty = if missing {
        merge(Schema::Null, field.ty)
    } else {
        field.ty
    };
    // A union default must match the first branch. Null is kept first.
    let add_default = match &ty {
        Schema::Union(items) => matches!(items.first(), Some(Schema::Null)),
        Schema::Null => missing,
        _ => false,
    };
    let mut obj = Map::new();
    obj.insert("name".into(), Value::String(field.name));
    obj.insert("type".into(), render(ty, used));
    if add_default {
        obj.insert("default".into(), Value::Null);
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::try_schema;
    use serde_json::Value;

    fn expect(src: &str, expected: Value) {
        let text = try_schema(src).unwrap_or_else(|| panic!("no schema for {src}"));
        assert_valid(&text);
        let pretty = serde_json::to_string_pretty(&expected).unwrap();
        assert_eq!(text, pretty);
    }

    fn assert_valid(text: &str) {
        let value: Value = serde_json::from_str(text).unwrap();
        let mut names = HashSet::new();
        walk(&value, &mut names);
    }

    fn walk(value: &Value, names: &mut HashSet<String>) {
        match value {
            Value::String(primitive) => {
                assert!(
                    matches!(
                        primitive.as_str(),
                        "null" | "boolean" | "int" | "long" | "double" | "string"
                    ),
                    "{primitive}"
                );
            }
            Value::Array(branches) => {
                assert!(branches.len() >= 2, "union needs two branches");
                let mut seen = HashSet::new();
                for branch in branches {
                    assert!(!branch.is_array(), "nested union");
                    let kind = union_kind(branch);
                    if kind != "record" {
                        assert!(seen.insert(kind), "duplicate union branch");
                    }
                    walk(branch, names);
                }
            }
            Value::Object(map) => match map.get("type").and_then(Value::as_str) {
                Some("record") => {
                    let name = map.get("name").and_then(Value::as_str).unwrap();
                    assert!(is_avro_name(name), "{name}");
                    assert!(names.insert(name.to_string()), "duplicate record {name}");
                    let fields = map.get("fields").and_then(Value::as_array).unwrap();
                    let mut field_names = HashSet::new();
                    for field in fields {
                        let field = field.as_object().unwrap();
                        let fname = field.get("name").and_then(Value::as_str).unwrap();
                        assert!(is_avro_name(fname), "{fname}");
                        assert!(field_names.insert(fname.to_string()), "duplicate {fname}");
                        let fty = field.get("type").unwrap();
                        if fty.as_array().is_some_and(|list| {
                            list.first().and_then(Value::as_str) == Some("null")
                        }) {
                            assert!(field.get("default").is_some_and(Value::is_null));
                        } else if fty.as_str() != Some("null") {
                            assert!(field.get("default").is_none());
                        }
                        walk(fty, names);
                    }
                }
                Some("array") => walk(map.get("items").unwrap(), names),
                other => panic!("unexpected type {other:?}"),
            },
            other => panic!("unexpected schema node {other}"),
        }
    }

    fn union_kind(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            Value::Object(map) => map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("object")
                .to_string(),
            _ => "other".to_string(),
        }
    }

    fn is_avro_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }

    #[test]
    fn object_becomes_a_record() {
        expect(
            r#"{"name":"copycraft","version":1,"ok":true,"tags":["rust","macos"],"repo":{"owner":"marcelkoopman","stars":2}}"#,
            serde_json::json!({
                "type": "record",
                "name": "document",
                "fields": [
                    {"name": "name", "type": "string"},
                    {"name": "version", "type": "int"},
                    {"name": "ok", "type": "boolean"},
                    {"name": "tags", "type": {"type": "array", "items": "string"}},
                    {"name": "repo", "type": {
                        "type": "record",
                        "name": "document_repo",
                        "fields": [
                            {"name": "owner", "type": "string"},
                            {"name": "stars", "type": "int"}
                        ]
                    }}
                ]
            }),
        );
    }

    #[test]
    fn array_of_objects_makes_missing_fields_nullable() {
        expect(
            r#"[{"id":1,"name":"a"},{"id":2},{"id":3,"name":"c"}]"#,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "record",
                    "name": "document",
                    "fields": [
                        {"name": "id", "type": "int"},
                        {
                            "name": "name",
                            "type": ["null", "string"],
                            "default": null
                        }
                    ]
                }
            }),
        );
    }

    #[test]
    fn repeated_records_keep_shared_fields_required() {
        expect(
            r#"[{"a":1},{"a":1},{"b":2}]"#,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "record",
                    "name": "document",
                    "fields": [
                        {"name": "a", "type": ["null", "int"], "default": null},
                        {"name": "b", "type": ["null", "int"], "default": null}
                    ]
                }
            }),
        );
    }

    #[test]
    fn numbers_widen_and_decimals_are_doubles() {
        expect(r#"{"n":2147483647}"#, field("n", serde_json::json!("int")));
        expect(r#"{"n":2147483648}"#, field("n", serde_json::json!("long")));
        expect(
            r#"{"n":-2147483649}"#,
            field("n", serde_json::json!("long")),
        );
        expect(r#"{"n":1.0}"#, field("n", serde_json::json!("double")));
        expect(
            r#"[{"n":1},{"n":2147483648},{"n":1.5}]"#,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "record",
                    "name": "document",
                    "fields": [{"name": "n", "type": "double"}]
                }
            }),
        );
    }

    #[test]
    fn empty_array_adopts_a_later_item_type() {
        expect("[]", serde_json::json!({"type": "array", "items": "null"}));
        expect(
            r#"[{"tags":[]},{"tags":["a"]}]"#,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "record",
                    "name": "document",
                    "fields": [{
                        "name": "tags",
                        "type": {"type": "array", "items": "string"}
                    }]
                }
            }),
        );
        expect(
            r#"{"tags":[null,"a"]}"#,
            field(
                "tags",
                serde_json::json!({"type": "array", "items": ["null", "string"]}),
            ),
        );
    }

    #[test]
    fn mixed_values_become_a_union() {
        expect(
            r#"[1,"a",null]"#,
            serde_json::json!({
                "type": "array",
                "items": ["null", "int", "string"]
            }),
        );
        expect(
            r#"[{"v":{"a":1}},{"v":[1,2]}]"#,
            serde_json::json!({
                "type": "array",
                "items": {
                    "type": "record",
                    "name": "document",
                    "fields": [{
                        "name": "v",
                        "type": [
                            {"type": "array", "items": "int"},
                            {
                                "type": "record",
                                "name": "document_v",
                                "fields": [{"name": "a", "type": "int"}]
                            }
                        ]
                    }]
                }
            }),
        );
    }

    #[test]
    fn field_names_are_avro_names_and_record_names_stay_unique() {
        expect(
            r#"{"café":1,"a-b":2,"a b":3,"1x":true,"":null}"#,
            serde_json::json!({
                "type": "record",
                "name": "document",
                "fields": [
                    {"name": "caf_", "type": "int"},
                    {"name": "a_b", "type": "int"},
                    {"name": "a_b_2", "type": "int"},
                    {"name": "_1x", "type": "boolean"},
                    {"name": "field", "type": "null"}
                ]
            }),
        );
        let text = try_schema(r#"{"a_b":{"c":1},"a":{"b":{"c":2}}}"#).unwrap();
        assert_valid(&text);
        assert!(text.contains("\"name\": \"document_a_b\""));
        assert!(text.contains("\"name\": \"document_a_b_2\""));
    }

    #[test]
    fn non_json_has_no_schema() {
        assert!(try_schema("hello").is_none());
        assert!(try_schema("\"hi\"").is_none());
        assert!(try_schema("null").is_none());
        assert!(try_schema("name: copycraft").is_none());
    }

    fn field(name: &str, ty: Value) -> Value {
        serde_json::json!({
            "type": "record",
            "name": "document",
            "fields": [{"name": name, "type": ty}]
        })
    }
}
