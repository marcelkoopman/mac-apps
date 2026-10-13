//! Inferring an XSD from sample XML, and writing it out.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Simple {
    Boolean,
    Int,
    Long,
    Double,
    String,
}

#[derive(Clone)]
pub(super) struct Node {
    pub(super) name: String,
    pub(super) attrs: Vec<(String, String)>,
    pub(super) children: Vec<Node>,
    /// Collapsed character data. Empty when the element has no text.
    pub(super) text: String,
}

pub(super) struct Elem {
    pub(super) name: String,
    pub(super) attrs: Vec<Attr>,
    pub(super) children: Vec<Child>,
    pub(super) simple: Option<Simple>,
    pub(super) mixed: bool,
    pub(super) samples: u32,
}

pub(super) struct Attr {
    pub(super) name: String,
    pub(super) ty: Simple,
    pub(super) present: u32,
}

pub(super) struct Child {
    pub(super) elem: Elem,
    /// Parent samples that contained this child at least once.
    pub(super) present: u32,
    pub(super) repeating: bool,
}

pub(super) fn infer_one(node: &Node) -> Elem {
    let mut groups: Vec<(&str, Vec<&Node>)> = Vec::new();
    for child in &node.children {
        if let Some((_, group)) = groups.iter_mut().find(|(name, _)| *name == child.name) {
            group.push(child);
        } else {
            groups.push((child.name.as_str(), vec![child]));
        }
    }
    let children = groups
        .into_iter()
        .map(|(_, group)| Child {
            repeating: group.len() > 1,
            present: 1,
            elem: infer_group(&group),
        })
        .collect();
    let simple = if node.children.is_empty() {
        text_simple(&node.text)
    } else {
        None
    };
    Elem {
        name: node.name.clone(),
        attrs: node
            .attrs
            .iter()
            .map(|(name, value)| Attr {
                name: name.clone(),
                ty: value_ty(value),
                present: 1,
            })
            .collect(),
        children,
        simple,
        mixed: !node.children.is_empty() && !node.text.is_empty(),
        samples: 1,
    }
}

pub(super) fn infer_group(nodes: &[&Node]) -> Elem {
    let mut acc = infer_one(nodes[0]);
    for node in nodes.iter().skip(1) {
        acc = merge(acc, infer_one(node));
    }
    acc
}

pub(super) fn merge(left: Elem, right: Elem) -> Elem {
    let mixed = left.mixed
        || right.mixed
        || (left.simple.is_some() && !right.children.is_empty())
        || (right.simple.is_some() && !left.children.is_empty());
    let simple = if left.children.is_empty() && right.children.is_empty() {
        merge_simple(left.simple, right.simple)
    } else {
        None
    };
    let mut children = left.children;
    for child in right.children {
        if let Some(existing) = children
            .iter_mut()
            .find(|item| item.elem.name == child.elem.name)
        {
            existing.elem = merge(
                std::mem::replace(&mut existing.elem, empty_elem()),
                child.elem,
            );
            existing.present = existing.present.saturating_add(child.present);
            existing.repeating = existing.repeating || child.repeating;
        } else {
            children.push(child);
        }
    }
    let mut attrs = left.attrs;
    for attr in right.attrs {
        if let Some(existing) = attrs.iter_mut().find(|item| item.name == attr.name) {
            existing.ty = widen(existing.ty, attr.ty);
            existing.present = existing.present.saturating_add(attr.present);
        } else {
            attrs.push(attr);
        }
    }
    Elem {
        name: left.name,
        attrs,
        children,
        simple,
        mixed,
        samples: left.samples.saturating_add(right.samples),
    }
}

pub(super) fn empty_elem() -> Elem {
    Elem {
        name: String::new(),
        attrs: Vec::new(),
        children: Vec::new(),
        simple: None,
        mixed: false,
        samples: 0,
    }
}

pub(super) fn merge_simple(left: Option<Simple>, right: Option<Simple>) -> Option<Simple> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(widen(left, right)),
    }
}

pub(super) fn widen(left: Simple, right: Simple) -> Simple {
    match (left, right) {
        (Simple::Int, Simple::Long) | (Simple::Long, Simple::Int) => Simple::Long,
        (Simple::Int | Simple::Long, Simple::Double)
        | (Simple::Double, Simple::Int | Simple::Long) => Simple::Double,
        (left, right) if left == right => left,
        _ => Simple::String,
    }
}

pub(super) fn text_simple(text: &str) -> Option<Simple> {
    if text.is_empty() {
        None
    } else {
        Some(value_ty(text))
    }
}

pub(super) fn value_ty(text: &str) -> Simple {
    if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        return Simple::Boolean;
    }
    if let Some(kind) = int_ty(text) {
        return kind;
    }
    if double_ty(text) {
        return Simple::Double;
    }
    Simple::String
}

pub(super) fn int_ty(text: &str) -> Option<Simple> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    if i32::try_from(value).is_ok() {
        Some(Simple::Int)
    } else {
        Some(Simple::Long)
    }
}

pub(super) fn double_ty(text: &str) -> bool {
    if !text.contains(['.', 'e', 'E']) {
        return false;
    }
    text.parse::<f64>()
        .ok()
        .is_some_and(|value| value.is_finite())
}

pub(super) fn type_name(kind: Simple) -> &'static str {
    match kind {
        Simple::Boolean => "xs:boolean",
        Simple::Int => "xs:int",
        Simple::Long => "xs:long",
        Simple::Double => "xs:double",
        Simple::String => "xs:string",
    }
}

pub(super) fn render(elem: &Elem) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n",
    );
    render_element(&mut out, elem, 1, false, false);
    out.push_str("</xs:schema>");
    out
}

pub(super) fn render_element(
    out: &mut String,
    elem: &Elem,
    indent: usize,
    optional: bool,
    repeating: bool,
) {
    let mut open = format!("<xs:element name=\"{}\"", escape(&elem.name));
    let leaf = elem.children.is_empty() && elem.attrs.is_empty() && !elem.mixed;
    if let Some(kind) = elem.simple
        && leaf
    {
        open.push_str(&format!(" type=\"{}\"", type_name(kind)));
        push_occurs(&mut open, optional, repeating);
        open.push_str("/>");
        line(out, indent, &open);
        return;
    }
    push_occurs(&mut open, optional, repeating);
    open.push('>');
    line(out, indent, &open);
    if leaf {
        line(out, indent + 1, "<xs:complexType/>");
        line(out, indent, "</xs:element>");
        return;
    }
    if elem.mixed {
        line(out, indent + 1, "<xs:complexType mixed=\"true\">");
    } else {
        line(out, indent + 1, "<xs:complexType>");
    }
    if let Some(kind) = elem.simple
        && elem.children.is_empty()
    {
        line(out, indent + 2, "<xs:simpleContent>");
        line(
            out,
            indent + 3,
            &format!("<xs:extension base=\"{}\">", type_name(kind)),
        );
        for attr in &elem.attrs {
            render_attr(out, attr, elem.samples, indent + 4);
        }
        line(out, indent + 3, "</xs:extension>");
        line(out, indent + 2, "</xs:simpleContent>");
    } else {
        if !elem.children.is_empty() {
            line(out, indent + 2, "<xs:sequence>");
            for child in &elem.children {
                let optional = child.present < elem.samples;
                render_element(out, &child.elem, indent + 3, optional, child.repeating);
            }
            line(out, indent + 2, "</xs:sequence>");
        }
        for attr in &elem.attrs {
            render_attr(out, attr, elem.samples, indent + 2);
        }
    }
    line(out, indent + 1, "</xs:complexType>");
    line(out, indent, "</xs:element>");
}

pub(super) fn render_attr(out: &mut String, attr: &Attr, samples: u32, indent: usize) {
    let mut text = format!(
        "<xs:attribute name=\"{}\" type=\"{}\"",
        escape(&attr.name),
        type_name(attr.ty)
    );
    if attr.present >= samples && samples > 0 {
        text.push_str(" use=\"required\"");
    }
    text.push_str("/>");
    line(out, indent, &text);
}

pub(super) fn push_occurs(open: &mut String, optional: bool, repeating: bool) {
    if optional {
        open.push_str(" minOccurs=\"0\"");
    }
    if repeating {
        open.push_str(" maxOccurs=\"unbounded\"");
    }
}

pub(super) fn line(out: &mut String, indent: usize, text: &str) {
    out.push_str(&pad(indent));
    out.push_str(text);
    out.push('\n');
}

pub(super) fn pad(indent: usize) -> String {
    "    ".repeat(indent)
}

pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}
