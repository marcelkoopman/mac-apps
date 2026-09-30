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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Simple {
    Boolean,
    Int,
    Long,
    Double,
    String,
}

#[derive(Clone)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    /// Collapsed character data. Empty when the element has no text.
    text: String,
}

struct Elem {
    name: String,
    attrs: Vec<Attr>,
    children: Vec<Child>,
    simple: Option<Simple>,
    mixed: bool,
    samples: u32,
}

struct Attr {
    name: String,
    ty: Simple,
    present: u32,
}

struct Child {
    elem: Elem,
    /// Parent samples that contained this child at least once.
    present: u32,
    repeating: bool,
}

fn infer_one(node: &Node) -> Elem {
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

fn infer_group(nodes: &[&Node]) -> Elem {
    let mut acc = infer_one(nodes[0]);
    for node in nodes.iter().skip(1) {
        acc = merge(acc, infer_one(node));
    }
    acc
}

fn merge(left: Elem, right: Elem) -> Elem {
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

fn empty_elem() -> Elem {
    Elem {
        name: String::new(),
        attrs: Vec::new(),
        children: Vec::new(),
        simple: None,
        mixed: false,
        samples: 0,
    }
}

fn merge_simple(left: Option<Simple>, right: Option<Simple>) -> Option<Simple> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) => Some(widen(left, right)),
    }
}

fn widen(left: Simple, right: Simple) -> Simple {
    match (left, right) {
        (Simple::Int, Simple::Long) | (Simple::Long, Simple::Int) => Simple::Long,
        (Simple::Int | Simple::Long, Simple::Double)
        | (Simple::Double, Simple::Int | Simple::Long) => Simple::Double,
        (left, right) if left == right => left,
        _ => Simple::String,
    }
}

fn text_simple(text: &str) -> Option<Simple> {
    if text.is_empty() {
        None
    } else {
        Some(value_ty(text))
    }
}

fn value_ty(text: &str) -> Simple {
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

fn int_ty(text: &str) -> Option<Simple> {
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

fn double_ty(text: &str) -> bool {
    if !text.contains(['.', 'e', 'E']) {
        return false;
    }
    text.parse::<f64>()
        .ok()
        .is_some_and(|value| value.is_finite())
}

fn type_name(kind: Simple) -> &'static str {
    match kind {
        Simple::Boolean => "xs:boolean",
        Simple::Int => "xs:int",
        Simple::Long => "xs:long",
        Simple::Double => "xs:double",
        Simple::String => "xs:string",
    }
}

fn render(elem: &Elem) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n",
    );
    render_element(&mut out, elem, 1, false, false);
    out.push_str("</xs:schema>");
    out
}

fn render_element(out: &mut String, elem: &Elem, indent: usize, optional: bool, repeating: bool) {
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

fn render_attr(out: &mut String, attr: &Attr, samples: u32, indent: usize) {
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

fn push_occurs(open: &mut String, optional: bool, repeating: bool) {
    if optional {
        open.push_str(" minOccurs=\"0\"");
    }
    if repeating {
        open.push_str(" maxOccurs=\"unbounded\"");
    }
}

fn line(out: &mut String, indent: usize, text: &str) {
    out.push_str(&pad(indent));
    out.push_str(text);
    out.push('\n');
}

fn pad(indent: usize) -> String {
    "    ".repeat(indent)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

fn parse_document(text: &str) -> Option<Node> {
    let mut parser = Parser { s: text, i: 0 };
    parser.skip_misc()?;
    let node = parser.parse_element()?;
    parser.skip_misc()?;
    parser.eof().then_some(node)
}

impl<'a> Parser<'a> {
    fn eof(&self) -> bool {
        self.i >= self.s.len()
    }

    fn starts(&self, prefix: &str) -> bool {
        self.s[self.i..].starts_with(prefix)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.s[self.i..].chars().next()?;
        self.i += ch.len_utf8();
        Some(ch)
    }

    fn expect(&mut self, want: char) -> Option<()> {
        (self.bump()? == want).then_some(())
    }

    fn consume(&mut self, prefix: &str) -> bool {
        if self.starts(prefix) {
            self.i += prefix.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.s[self.i..].chars().next() {
            if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn skip_misc(&mut self) -> Option<()> {
        loop {
            self.skip_ws();
            if self.starts("<!--") {
                self.skip_comment()?;
            } else if self.starts("<?") {
                self.skip_until("?>")?;
            } else if self.starts("<!") {
                self.skip_declaration()?;
            } else {
                break;
            }
        }
        Some(())
    }

    fn skip_comment(&mut self) -> Option<()> {
        self.skip_until("-->")
    }

    fn skip_until(&mut self, marker: &str) -> Option<()> {
        let rest = self.s[self.i..].find(marker)?;
        self.i += rest + marker.len();
        Some(())
    }

    fn skip_declaration(&mut self) -> Option<()> {
        self.expect('<')?;
        let mut brackets = 0i32;
        while let Some(ch) = self.bump() {
            match ch {
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '>' if brackets == 0 => return Some(()),
                _ => {}
            }
        }
        None
    }

    fn parse_element(&mut self) -> Option<Node> {
        self.expect('<')?;
        if self.starts("/") || self.starts("!") || self.starts("?") {
            return None;
        }
        let raw = self.read_name()?;
        let mut attrs = Vec::new();
        let closed = loop {
            self.skip_ws();
            if self.consume("/>") {
                break true;
            }
            if self.consume(">") {
                break false;
            }
            let attr_name = self.read_name()?;
            self.skip_ws();
            self.expect('=')?;
            self.skip_ws();
            let value = self.read_quoted()?;
            if is_xmlns(&attr_name) {
                continue;
            }
            let name = local_name(&attr_name);
            if !attrs.iter().any(|(existing, _)| existing == &name) {
                attrs.push((name, value));
            }
        };
        let (children, text) = if closed {
            (Vec::new(), String::new())
        } else {
            self.parse_content(&raw)?
        };
        Some(Node {
            name: local_name(&raw),
            attrs,
            children,
            text,
        })
    }

    fn parse_content(&mut self, raw_name: &str) -> Option<(Vec<Node>, String)> {
        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            if self.eof() {
                return None;
            }
            if self.starts("</") {
                self.consume("</");
                let close = self.read_name()?;
                self.skip_ws();
                self.expect('>')?;
                if close != raw_name {
                    return None;
                }
                break;
            }
            if self.starts("<!--") {
                self.skip_comment()?;
            } else if self.starts("<![CDATA[") {
                push_text(&mut text, &self.read_cdata()?);
            } else if self.starts("<?") {
                self.skip_until("?>")?;
            } else if self.starts("<!") {
                self.skip_declaration()?;
            } else if self.starts("<") {
                children.push(self.parse_element()?);
            } else {
                push_text(&mut text, &self.read_text()?);
            }
        }
        Some((children, text))
    }

    fn read_name(&mut self) -> Option<String> {
        let start = self.i;
        let first = self.s[self.i..].chars().next()?;
        if !is_name_start(first) {
            return None;
        }
        self.i += first.len_utf8();
        while let Some(ch) = self.s[self.i..].chars().next() {
            if is_name_char(ch) {
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }
        Some(self.s[start..self.i].to_string())
    }

    fn read_quoted(&mut self) -> Option<String> {
        let quote = self.bump()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let start = self.i;
        while let Some(ch) = self.s[self.i..].chars().next() {
            if ch == quote {
                let raw = &self.s[start..self.i];
                self.i += ch.len_utf8();
                return decode_entities(raw);
            }
            self.i += ch.len_utf8();
        }
        None
    }

    fn read_text(&mut self) -> Option<String> {
        let start = self.i;
        while let Some(ch) = self.s[self.i..].chars().next() {
            if ch == '<' {
                break;
            }
            self.i += ch.len_utf8();
        }
        decode_entities(&self.s[start..self.i])
    }

    fn read_cdata(&mut self) -> Option<String> {
        self.consume("<![CDATA[");
        let rest = self.s[self.i..].find("]]>")?;
        let text = self.s[self.i..self.i + rest].to_string();
        self.i += rest + 3;
        Some(text)
    }
}

fn push_text(text: &mut String, chunk: &str) {
    let trimmed = chunk.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(&trimmed);
}

fn is_xmlns(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

fn local_name(qname: &str) -> String {
    qname
        .rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(qname)
        .to_string()
}

fn is_name_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == ':' || (!ch.is_ascii() && ch.is_alphabetic())
}

fn is_name_char(ch: char) -> bool {
    is_name_start(ch) || ch.is_ascii_digit() || ch == '-' || ch == '.'
}

fn decode_entities(raw: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        let end = rest.find(';')?;
        let name = &rest[..end];
        rest = &rest[end + 1..];
        out.push(decode_entity(name)?);
    }
    out.push_str(rest);
    Some(out)
}

struct Schema {
    elements: Vec<Node>,
    complex_types: Vec<Node>,
    simple_types: Vec<Node>,
}

impl Schema {
    fn from_root(root: &Node) -> Self {
        Self {
            elements: named_top(root, "element"),
            complex_types: named_top(root, "complexType"),
            simple_types: named_top(root, "simpleType"),
        }
    }
}

fn named_top(root: &Node, kind: &str) -> Vec<Node> {
    root.children
        .iter()
        .filter(|child| {
            child.name == kind && attr(child, "name").is_some_and(|name| !name.is_empty())
        })
        .filter(|child| kind != "element" || attr(child, "abstract") != Some("true"))
        .cloned()
        .collect()
}

enum TypeRef<'a> {
    Complex(&'a Node),
    Simple(&'a Node),
    Builtin(String),
}

fn lookup_type<'a>(qname: &str, schema: &'a Schema) -> TypeRef<'a> {
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

fn split_qname(qname: &str) -> (Option<&str>, &str) {
    match qname.rsplit_once(':') {
        Some((prefix, local)) if !local.is_empty() => (Some(prefix), local),
        _ => (None, qname),
    }
}

fn element_name(decl: &Node, schema: &Schema) -> Option<String> {
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

fn content_node<'a>(decl: &'a Node, schema: &'a Schema) -> &'a Node {
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

fn child_named<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    node.children.iter().find(|child| child.name == name)
}

fn attr<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn repeat_count(node: &Node) -> u32 {
    match attr(node, "maxOccurs") {
        Some("unbounded") => 2,
        Some(raw) => raw.parse::<u32>().unwrap_or(1).min(2),
        None => 1,
    }
}

fn write_instance(
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

fn write_complex(
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

fn write_particles(
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

fn write_particles_body(
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

fn write_compositors(
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

fn write_first_choice(
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

fn write_particle(
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

fn gather_attrs(
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

fn gather_attrs_body(
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

fn direct_attrs(node: &Node, schema: &Schema, index: u32) -> Vec<(String, String)> {
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

fn value_of_type(qname: &str, schema: &Schema, index: u32, depth: usize) -> String {
    if depth > SAMPLE_DEPTH {
        return builtin_value("string", index);
    }
    match lookup_type(qname, schema) {
        TypeRef::Builtin(local) => builtin_value(&local, index),
        TypeRef::Complex(_) => builtin_value("string", index),
        TypeRef::Simple(simple) => simple_type_value(simple, schema, index, depth + 1),
    }
}

fn simple_type_value(node: &Node, schema: &Schema, index: u32, depth: usize) -> String {
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

fn builtin_value(local: &str, index: u32) -> String {
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

fn push_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(&xml_escape(value, true));
        out.push('"');
    }
}

fn push_leaf(out: &mut String, name: &str, attrs: &[(String, String)], text: &str) {
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

fn push_parent(out: &mut String, name: &str, attrs: &[(String, String)], inner: &str) {
    out.push('<');
    out.push_str(name);
    push_attrs(out, attrs);
    out.push('>');
    out.push_str(inner);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

fn push_mixed(out: &mut String, name: &str, attrs: &[(String, String)], text: &str, inner: &str) {
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

fn xml_escape(value: &str, attr_value: bool) -> String {
    let mut out = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attr_value {
        out = out.replace('"', "&quot;");
    }
    out
}

fn decode_entity(name: &str) -> Option<char> {
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

#[cfg(test)]
mod tests {
    use super::{is_xsd, try_sample, try_schema};

    #[test]
    fn person_becomes_an_xsd() {
        let src = r#"<person id="1"><name>Ada</name><age>36</age></person>"#;
        assert_eq!(
            try_schema(src).as_deref(),
            Some(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
    <xs:element name="person">
        <xs:complexType>
            <xs:sequence>
                <xs:element name="name" type="xs:string"/>
                <xs:element name="age" type="xs:int"/>
            </xs:sequence>
            <xs:attribute name="id" type="xs:int" use="required"/>
        </xs:complexType>
    </xs:element>
</xs:schema>"#
            )
        );
    }

    #[test]
    fn repeated_children_are_optional_when_missing() {
        let src = "\
<list>
  <item><id>1</id><name>a</name></item>
  <item><id>2</id></item>
</list>";
        let schema = try_schema(src).unwrap();
        assert!(schema.contains("<xs:element name=\"item\" maxOccurs=\"unbounded\">"));
        assert!(schema.contains("<xs:element name=\"id\" type=\"xs:int\"/>"));
        assert!(schema.contains("<xs:element name=\"name\" type=\"xs:string\" minOccurs=\"0\"/>"));
        assert!(!schema.contains("copycraft"));
    }

    #[test]
    fn scalars_widen_like_numbers() {
        let src =
            "<list><n>1</n><n>3000000000</n><n>1.5</n><flag>true</flag><flag>false</flag></list>";
        let schema = try_schema(src).unwrap();
        assert!(
            schema.contains("<xs:element name=\"n\" type=\"xs:double\" maxOccurs=\"unbounded\"/>")
        );
        assert!(
            schema.contains(
                "<xs:element name=\"flag\" type=\"xs:boolean\" maxOccurs=\"unbounded\"/>"
            )
        );
    }

    #[test]
    fn conflicting_text_becomes_string() {
        let schema = try_schema("<list><a>1</a><a>no</a></list>").unwrap();
        assert!(
            schema.contains("<xs:element name=\"a\" type=\"xs:string\" maxOccurs=\"unbounded\"/>")
        );
    }

    #[test]
    fn empty_element_is_an_empty_complex_type() {
        let schema = try_schema("<root><item/></root>").unwrap();
        assert!(schema.contains("<xs:element name=\"item\">"));
        assert!(schema.contains("<xs:complexType/>"));
    }

    #[test]
    fn text_with_an_attribute_uses_simple_content() {
        let schema = try_schema(r#"<title lang="en">Hello</title>"#).unwrap();
        assert!(schema.contains("<xs:simpleContent>"));
        assert!(schema.contains("base=\"xs:string\""));
        assert!(schema.contains("name=\"lang\" type=\"xs:string\" use=\"required\""));
    }

    #[test]
    fn declaration_comment_and_entities_are_read() {
        let src = r#"<?xml version="1.0"?><!-- note --><root><name>A &amp; B</name></root>"#;
        let schema = try_schema(src).unwrap();
        assert!(schema.contains("name=\"root\""));
        assert!(schema.contains("name=\"name\" type=\"xs:string\""));
    }

    #[test]
    fn prefixed_names_use_the_local_name() {
        let schema = try_schema(r#"<h:table xmlns:h="urn:example"><h:tr/></h:table>"#).unwrap();
        assert!(schema.contains("name=\"table\""));
        assert!(schema.contains("name=\"tr\""));
        assert!(!schema.contains("urn:example"));
    }

    #[test]
    fn non_xml_and_broken_markup_have_no_schema() {
        assert!(try_schema("hello").is_none());
        assert!(try_schema("<a></b>").is_none());
        assert!(try_schema("<a></a><b></b>").is_none());
        assert!(try_schema("<root><item></item>").is_none());
    }

    #[test]
    fn person_xsd_sample_round_trips() {
        let src = r#"<person id="1"><name>Ada</name><age>36</age></person>"#;
        let schema = try_schema(src).unwrap();
        assert!(is_xsd(&schema));
        assert_eq!(
            try_sample(&schema).as_deref(),
            Some("<person id=\"1\">\n    <name>sample</name>\n    <age>1</age>\n</person>")
        );
    }

    #[test]
    fn repeated_optional_children_are_filled_twice() {
        let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="list">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="item" maxOccurs="unbounded">
          <xs:complexType>
            <xs:sequence>
              <xs:element name="id" type="xs:int"/>
              <xs:element name="name" type="xs:string" minOccurs="0"/>
            </xs:sequence>
          </xs:complexType>
        </xs:element>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
        assert_eq!(
            try_sample(schema).as_deref(),
            Some(
                "<list>\n    <item>\n        <id>1</id>\n        <name>sample</name>\n    </item>\n    <item>\n        <id>2</id>\n        <name>sample 2</name>\n    </item>\n</list>"
            )
        );
    }

    #[test]
    fn simple_content_includes_the_attribute() {
        let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="title">
    <xs:complexType>
      <xs:simpleContent>
        <xs:extension base="xs:string">
          <xs:attribute name="lang" type="xs:string"/>
          <xs:attribute name="gone" type="xs:string" use="prohibited"/>
        </xs:extension>
      </xs:simpleContent>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
        assert_eq!(
            try_sample(schema).as_deref(),
            Some("<title lang=\"sample\">sample</title>")
        );
    }

    #[test]
    fn named_complex_type_is_preferred_over_a_builtin() {
        let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="person" type="Person"/>
  <xs:complexType name="Person">
    <xs:sequence>
      <xs:element name="name" type="xs:string"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:int" use="required"/>
  </xs:complexType>
  <xs:element name="box" type="string"/>
  <xs:complexType name="string">
    <xs:sequence>
      <xs:element name="value" type="xs:int"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#;
        assert_eq!(
            try_sample(schema).as_deref(),
            Some("<person id=\"1\">\n    <name>sample</name>\n</person>")
        );
        let box_schema = schema.replace(r#"<xs:element name="person" type="Person"/>"#, "");
        assert_eq!(
            try_sample(&box_schema).as_deref(),
            Some("<box>\n    <value>1</value>\n</box>")
        );
    }

    #[test]
    fn choice_ref_extension_and_recursion_stay_bounded() {
        let schema = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="employee" type="Employee"/>
  <xs:element name="name" type="xs:string"/>
  <xs:complexType name="Named">
    <xs:sequence>
      <xs:element ref="name"/>
    </xs:sequence>
    <xs:attribute name="id" type="xs:int"/>
  </xs:complexType>
  <xs:complexType name="Employee">
    <xs:complexContent>
      <xs:extension base="Named">
        <xs:sequence>
          <xs:choice>
            <xs:element name="role" type="xs:string"/>
            <xs:element name="title" type="xs:string"/>
          </xs:choice>
        </xs:sequence>
        <xs:attribute name="dept" type="xs:string"/>
      </xs:extension>
    </xs:complexContent>
  </xs:complexType>
</xs:schema>"#;
        assert_eq!(
            try_sample(schema).as_deref(),
            Some(
                "<employee id=\"1\" dept=\"sample\">\n    <name>sample</name>\n    <role>sample</role>\n</employee>"
            )
        );
        let recursive = r#"
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="node">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="node" minOccurs="0" type="xs:string"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>"#;
        assert_eq!(
            try_sample(recursive).as_deref(),
            Some("<node>\n    <node/>\n</node>")
        );
    }

    #[test]
    fn non_xsd_and_empty_schema_have_no_sample() {
        assert!(!is_xsd("hello"));
        assert!(try_sample("hello").is_none());
        assert!(!is_xsd(r#"<person id="1"><name>Ada</name></person>"#));
        assert!(try_sample(r#"<person id="1"><name>Ada</name></person>"#).is_none());
        let empty = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"/>"#;
        assert!(is_xsd(empty));
        assert!(try_sample(empty).is_none());
    }
}
