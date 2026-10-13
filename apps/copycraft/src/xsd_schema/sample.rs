//! A sample XML instance from an XSD.

use super::*;

pub(super) struct Schema {
    pub(super) elements: Vec<Node>,
    pub(super) complex_types: Vec<Node>,
    pub(super) simple_types: Vec<Node>,
}

impl Schema {
    pub(super) fn from_root(root: &Node) -> Self {
        Self {
            elements: named_top(root, "element"),
            complex_types: named_top(root, "complexType"),
            simple_types: named_top(root, "simpleType"),
        }
    }
}

pub(super) fn named_top(root: &Node, kind: &str) -> Vec<Node> {
    root.children
        .iter()
        .filter(|child| {
            child.name == kind && attr(child, "name").is_some_and(|name| !name.is_empty())
        })
        .filter(|child| kind != "element" || attr(child, "abstract") != Some("true"))
        .cloned()
        .collect()
}

pub(super) enum TypeRef<'a> {
    Complex(&'a Node),
    Simple(&'a Node),
    Builtin(String),
}

pub(super) fn lookup_type<'a>(qname: &str, schema: &'a Schema) -> TypeRef<'a> {
    let (prefix, local) = split_qname(qname);
    if !matches!(prefix, Some("xs" | "xsd")) {
        if let Some(node) = schema
            .complex_types
            .iter()
            .find(|node| attr(node, "name") == Some(local))
        {
            return TypeRef::Complex(node);
        }
        if let Some(node) = schema
            .simple_types
            .iter()
            .find(|node| attr(node, "name") == Some(local))
        {
            return TypeRef::Simple(node);
        }
    }
    TypeRef::Builtin(local.to_string())
}

pub(super) fn split_qname(qname: &str) -> (Option<&str>, &str) {
    match qname.rsplit_once(':') {
        Some((prefix, local)) if !local.is_empty() => (Some(prefix), local),
        _ => (None, qname),
    }
}

pub(super) fn element_name(decl: &Node, schema: &Schema) -> Option<String> {
    if let Some(name) = attr(decl, "name")
        && !name.is_empty()
    {
        return Some(name.to_string());
    }
    let reference = attr(decl, "ref")?;
    let local = local_name(reference);
    schema
        .elements
        .iter()
        .any(|element| attr(element, "name") == Some(local.as_str()))
        .then_some(local)
}

pub(super) fn content_node<'a>(decl: &'a Node, schema: &'a Schema) -> &'a Node {
    let mut current = decl;
    for _ in 0..SAMPLE_DEPTH {
        let Some(reference) = attr(current, "ref") else {
            break;
        };
        let local = local_name(reference);
        let Some(global) = schema
            .elements
            .iter()
            .find(|element| attr(element, "name") == Some(local.as_str()))
        else {
            break;
        };
        if std::ptr::eq(global, current) {
            break;
        }
        current = global;
    }
    current
}

pub(super) fn child_named<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children.iter().find(|child| child.name == name)
}

pub(super) fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

pub(super) fn repeat_count(node: &Node) -> u32 {
    match attr(node, "maxOccurs") {
        Some("unbounded") => 2,
        Some(raw) => raw.parse::<u32>().unwrap_or(1).min(2),
        None => 1,
    }
}

pub(super) fn write_instance(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    name: &str,
    decl: &Node,
    depth: usize,
    index: u32,
) {
    let content = content_node(decl, schema);
    if let Some(complex) = child_named(content, "complexType") {
        write_complex(out, schema, stack, name, complex, depth, index);
        return;
    }
    if let Some(simple) = child_named(content, "simpleType") {
        let value = simple_type_value(simple, schema, index, 0);
        push_leaf(out, name, &[], &value);
        return;
    }
    if let Some(ty) = attr(content, "type") {
        match lookup_type(ty, schema) {
            TypeRef::Complex(complex) => {
                write_complex(out, schema, stack, name, complex, depth, index);
            }
            TypeRef::Simple(simple) => {
                let value = simple_type_value(simple, schema, index, 0);
                push_leaf(out, name, &[], &value);
            }
            TypeRef::Builtin(local) => {
                let value = builtin_value(&local, index);
                push_leaf(out, name, &[], &value);
            }
        }
        return;
    }
    push_leaf(out, name, &[], &builtin_value("string", index));
}

pub(super) fn write_complex(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    name: &str,
    complex: &Node,
    depth: usize,
    index: u32,
) {
    if let Some(simple) = child_named(complex, "simpleContent") {
        let ext = child_named(simple, "extension").or_else(|| child_named(simple, "restriction"));
        let base = ext
            .and_then(|node| attr(node, "base"))
            .unwrap_or("xs:string");
        let value = value_of_type(base, schema, index, 0);
        let attrs = ext
            .map(|node| direct_attrs(node, schema, index))
            .unwrap_or_default();
        push_leaf(out, name, &attrs, &value);
        return;
    }
    let mut seen = Vec::new();
    let attrs = gather_attrs(complex, schema, index, &mut seen, 0);
    let mut inner = String::new();
    let mut seen = Vec::new();
    write_particles(
        &mut inner,
        schema,
        stack,
        complex,
        depth + 1,
        index,
        &mut seen,
    );
    if attr(complex, "mixed") == Some("true") {
        push_mixed(out, name, &attrs, &builtin_value("string", index), &inner);
    } else if inner.is_empty() {
        push_leaf(out, name, &attrs, "");
    } else {
        push_parent(out, name, &attrs, &inner);
    }
}

pub(super) fn write_particles(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    node: &Node,
    depth: usize,
    index: u32,
    seen: &mut Vec<String>,
) {
    if node.name == "complexType"
        && let Some(type_name) = attr(node, "name")
    {
        if seen.iter().any(|item| item == type_name) {
            return;
        }
        seen.push(type_name.to_string());
        write_particles_body(out, schema, stack, node, depth, index, seen);
        seen.pop();
        return;
    }
    write_particles_body(out, schema, stack, node, depth, index, seen);
}

pub(super) fn write_particles_body(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    node: &Node,
    depth: usize,
    index: u32,
    seen: &mut Vec<String>,
) {
    if let Some(content) = child_named(node, "complexContent") {
        if let Some(ext) = child_named(content, "extension") {
            if let Some(base) = attr(ext, "base")
                && let TypeRef::Complex(base_type) = lookup_type(base, schema)
            {
                write_particles(out, schema, stack, base_type, depth, index, seen);
            }
            write_compositors(out, schema, stack, ext, depth, index, seen);
            return;
        }
        if let Some(restriction) = child_named(content, "restriction") {
            write_compositors(out, schema, stack, restriction, depth, index, seen);
            return;
        }
    }
    write_compositors(out, schema, stack, node, depth, index, seen);
}

pub(super) fn write_compositors(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    node: &Node,
    depth: usize,
    index: u32,
    seen: &mut Vec<String>,
) {
    for child in &node.children {
        match child.name.as_str() {
            "sequence" | "all" => {
                write_compositors(out, schema, stack, child, depth, index, seen);
            }
            "choice" => write_first_choice(out, schema, stack, child, depth, index, seen),
            "element" => write_particle(out, schema, stack, child, depth, index),
            _ => {}
        }
    }
}

pub(super) fn write_first_choice(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    choice: &Node,
    depth: usize,
    index: u32,
    seen: &mut Vec<String>,
) {
    for child in &choice.children {
        match child.name.as_str() {
            "element" => {
                write_particle(out, schema, stack, child, depth, index);
                return;
            }
            "sequence" | "all" => {
                write_compositors(out, schema, stack, child, depth, index, seen);
                return;
            }
            "choice" => {
                write_first_choice(out, schema, stack, child, depth, index, seen);
                return;
            }
            _ => {}
        }
    }
}

pub(super) fn write_particle(
    out: &mut String,
    schema: &Schema,
    stack: &mut Vec<String>,
    decl: &Node,
    depth: usize,
    inherited: u32,
) {
    let Some(name) = element_name(decl, schema) else {
        return;
    };
    if depth >= SAMPLE_DEPTH || stack.iter().any(|seen| seen == &name) {
        push_leaf(out, &name, &[], "");
        return;
    }
    let times = repeat_count(decl);
    if times == 0 {
        return;
    }
    let repeating = times > 1;
    for step in 0..times {
        let index = if repeating { step } else { inherited };
        stack.push(name.clone());
        write_instance(out, schema, stack, &name, decl, depth, index);
        stack.pop();
    }
}

pub(super) fn gather_attrs(
    node: &Node,
    schema: &Schema,
    index: u32,
    seen: &mut Vec<String>,
    depth: usize,
) -> Vec<(String, String)> {
    if depth > SAMPLE_DEPTH {
        return Vec::new();
    }
    if node.name == "complexType"
        && let Some(type_name) = attr(node, "name")
    {
        if seen.iter().any(|item| item == type_name) {
            return Vec::new();
        }
        seen.push(type_name.to_string());
        let attrs = gather_attrs_body(node, schema, index, seen, depth);
        seen.pop();
        return attrs;
    }
    gather_attrs_body(node, schema, index, seen, depth)
}

pub(super) fn gather_attrs_body(
    node: &Node,
    schema: &Schema,
    index: u32,
    seen: &mut Vec<String>,
    depth: usize,
) -> Vec<(String, String)> {
    if let Some(content) = child_named(node, "complexContent") {
        if let Some(ext) = child_named(content, "extension") {
            let mut attrs = Vec::new();
            if let Some(base) = attr(ext, "base")
                && let TypeRef::Complex(base_type) = lookup_type(base, schema)
            {
                attrs.extend(gather_attrs(base_type, schema, index, seen, depth + 1));
            }
            attrs.extend(direct_attrs(ext, schema, index));
            return attrs;
        }
        if let Some(restriction) = child_named(content, "restriction") {
            return direct_attrs(restriction, schema, index);
        }
    }
    direct_attrs(node, schema, index)
}

pub(super) fn direct_attrs(node: &Node, schema: &Schema, index: u32) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    for child in &node.children {
        if child.name != "attribute" || attr(child, "use") == Some("prohibited") {
            continue;
        }
        let Some(name) = attr(child, "name").filter(|name| !name.is_empty()) else {
            continue;
        };
        let value = if let Some(simple) = child_named(child, "simpleType") {
            simple_type_value(simple, schema, index, 0)
        } else {
            value_of_type(attr(child, "type").unwrap_or("xs:string"), schema, index, 0)
        };
        attrs.push((name.to_string(), value));
    }
    attrs
}

pub(super) fn value_of_type(qname: &str, schema: &Schema, index: u32, depth: usize) -> String {
    if depth > SAMPLE_DEPTH {
        return builtin_value("string", index);
    }
    match lookup_type(qname, schema) {
        TypeRef::Builtin(local) => builtin_value(&local, index),
        TypeRef::Complex(_) => builtin_value("string", index),
        TypeRef::Simple(simple) => simple_type_value(simple, schema, index, depth + 1),
    }
}

pub(super) fn simple_type_value(node: &Node, schema: &Schema, index: u32, depth: usize) -> String {
    if let Some(restriction) = child_named(node, "restriction")
        && let Some(base) = attr(restriction, "base")
    {
        return value_of_type(base, schema, index, depth);
    }
    if let Some(list) = child_named(node, "list")
        && let Some(item) = attr(list, "itemType")
    {
        return value_of_type(item, schema, index, depth);
    }
    builtin_value("string", index)
}

pub(super) fn builtin_value(local: &str, index: u32) -> String {
    let n = index + 1;
    match local {
        "boolean" => "true".to_string(),
        "byte" | "int" | "integer" | "long" | "negativeInteger" | "nonNegativeInteger"
        | "nonPositiveInteger" | "positiveInteger" | "short" | "unsignedByte" | "unsignedInt"
        | "unsignedLong" | "unsignedShort" => n.to_string(),
        "decimal" | "double" | "float" => format!("{n}.0"),
        "date" => "2026-01-01".to_string(),
        "dateTime" => "2026-01-01T00:00:00".to_string(),
        "time" => "00:00:00".to_string(),
        "anyURI" => "https://example.com".to_string(),
        _ if index == 0 => "sample".to_string(),
        _ => format!("sample {}", index + 1),
    }
}

pub(super) fn push_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(&xml_escape(value, true));
        out.push('"');
    }
}

pub(super) fn push_leaf(out: &mut String, name: &str, attrs: &[(String, String)], text: &str) {
    out.push('<');
    out.push_str(name);
    push_attrs(out, attrs);
    if text.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    out.push_str(&xml_escape(text, false));
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

pub(super) fn push_parent(out: &mut String, name: &str, attrs: &[(String, String)], inner: &str) {
    out.push('<');
    out.push_str(name);
    push_attrs(out, attrs);
    out.push('>');
    out.push_str(inner);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

pub(super) fn push_mixed(
    out: &mut String,
    name: &str,
    attrs: &[(String, String)],
    text: &str,
    inner: &str,
) {
    out.push('<');
    out.push_str(name);
    push_attrs(out, attrs);
    out.push('>');
    out.push_str(&xml_escape(text, false));
    out.push_str(inner);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

pub(super) fn xml_escape(value: &str, attr_value: bool) -> String {
    let mut out = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attr_value {
        out = out.replace('"', "&quot;");
    }
    out
}

pub(super) fn decode_entity(name: &str) -> Option<char> {
    if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return char::from_u32(u32::from_str_radix(hex, 16).ok()?);
    }
    if let Some(digits) = name.strip_prefix('#') {
        return char::from_u32(digits.parse().ok()?);
    }
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}
