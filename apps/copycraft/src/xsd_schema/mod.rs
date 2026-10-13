const SCHEMA_NS: &str = "http://www.w3.org/2001/XMLSchema";
const SAMPLE_DEPTH: usize = 8;

/// Pretty-printed W3C XML Schema inferred from an XML document.
pub fn try_schema(text: &str) -> Option<String> {
    let root = parse_document(text.trim())?;
    Some(render(&infer_one(&root)))
}

/// Root element is `schema` and the document uses the XML Schema namespace.
pub fn is_xsd(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.contains(SCHEMA_NS) && parse_document(trimmed).is_some_and(|root| root.name == "schema")
}

/// One sample instance for the first global element of an XML Schema.
pub fn try_sample(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if !trimmed.contains(SCHEMA_NS) {
        return None;
    }
    let root = parse_document(trimmed)?;
    if root.name != "schema" {
        return None;
    }
    let schema = Schema::from_root(&root);
    let element = schema.elements.first()?;
    let name = element_name(element, &schema)?;
    let mut out = String::new();
    let mut stack = vec![name.clone()];
    write_instance(&mut out, &schema, &mut stack, &name, element, 0, 0);
    if out.is_empty() {
        None
    } else {
        Some(crate::format::pretty_xml(&out))
    }
}

mod infer;
mod parse;
mod sample;

use infer::*;
use parse::*;
use sample::*;

#[cfg(test)]
mod tests;
