//! SVG: an XML document whose root element is `<svg>`. Shown as a picture (AppKit draws SVG
//! itself) with the code a toggle away. Nothing is ever fetched: a document that points
//! outside itself is not drawn at all.

/// The root element is `<svg …>`, after an optional `<?xml …?>`, doctype and comments.
pub fn is_svg(text: &str) -> bool {
    root_tag(text).is_some()
}

/// The root `<svg …>` start tag (its attributes), when the document is an SVG.
fn root_tag(text: &str) -> Option<&str> {
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("<?") {
            rest = after.split_once("?>")?.1.trim_start();
        } else if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.split_once("-->")?.1.trim_start();
        } else if rest.starts_with("<!") {
            rest = skip_doctype(rest)?.trim_start();
        } else {
            break;
        }
    }
    let after = rest.strip_prefix("<")?;
    let name_end = after
        .find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')
        .unwrap_or(after.len());
    let name = &after[..name_end];
    let local = name.rsplit(':').next().unwrap_or(name);
    if local != "svg" {
        return None;
    }
    let end = after.find('>')?;
    Some(&after[name_end..end])
}

/// Past a `<!DOCTYPE …>`, internal subset (`[ … ]`) included.
fn skip_doctype(text: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '>' if depth == 0 => return Some(&text[at + 1..]),
            _ => {}
        }
    }
    None
}

/// A root attribute's value (`name="…"` or `name='…'`).
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let after = rest[at + name.len()..].trim_start();
        if before_ok && let Some(value) = after.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &value[1..];
                return inner.find(quote).map(|end| inner[..end].trim());
            }
        }
        rest = &rest[at + name.len()..];
    }
    None
}

/// "48 × 24" from the root's `width`/`height`, else from its `viewBox`. Units stay as written
/// (`10cm`); a bare number or `px` shows as the number.
pub fn dimensions(text: &str) -> Option<String> {
    let tag = root_tag(text)?;
    let clean = |value: &str| value.strip_suffix("px").unwrap_or(value).trim().to_string();
    let width = attribute(tag, "width").filter(|v| !v.is_empty() && !v.ends_with('%'));
    let height = attribute(tag, "height").filter(|v| !v.is_empty() && !v.ends_with('%'));
    if let (Some(width), Some(height)) = (width, height) {
        return Some(format!("{} × {}", clean(width), clean(height)));
    }
    let view_box = attribute(tag, "viewBox")?;
    let parts: Vec<&str> = view_box
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .collect();
    let [_, _, width, height] = parts.as_slice() else {
        return None;
    };
    let number = |value: &str| value.parse::<f64>().ok().filter(|n| *n > 0.0);
    number(width)?;
    number(height)?;
    Some(format!("{width} × {height}"))
}

/// The document points outside itself: an `href` / `xlink:href` / `src` or a CSS `url(…)` /
/// `@import` that is not a `#fragment` or a `data:` URI. Such an SVG is not drawn, so no
/// renderer ever tries to load it.
pub fn has_external_refs(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("@import") || lower.contains("<!entity") {
        return true;
    }
    let local = |value: &str| {
        let value = value.trim_start_matches(['"', '\'', ' ', '\t', '\n']);
        value.is_empty() || value.starts_with('#') || value.starts_with("data:")
    };
    for key in ["href", "src"] {
        let mut rest = lower.as_str();
        while let Some(at) = rest.find(key) {
            let after = rest[at + key.len()..].trim_start();
            if let Some(value) = after.strip_prefix('=')
                && !local(value.trim_start())
            {
                return true;
            }
            rest = &rest[at + key.len()..];
        }
    }
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("url(") {
        if !local(&rest[at + 4..]) {
            return true;
        }
        rest = &rest[at + 4..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_svg_root_is_svg_after_prolog_doctype_and_comments() {
        assert!(is_svg(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20"/>"#
        ));
        assert!(is_svg(
            "<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\">\n<svg viewBox=\"0 0 4 4\"><rect/></svg>"
        ));
        assert!(is_svg(
            "\u{feff}  <svg:svg xmlns:svg=\"http://www.w3.org/2000/svg\"></svg:svg>"
        ));
        assert!(!is_svg("<html><svg></svg></html>"));
        assert!(!is_svg("<svgx/>"));
        assert!(!is_svg("<root><svg/></root>"));
        assert!(!is_svg("svg text"));
        assert_eq!(
            super::super::detect("<?xml version=\"1.0\"?><svg width=\"1\" height=\"1\"/>"),
            super::super::FormatKind::Svg
        );
        assert_eq!(
            super::super::detect("<root/>"),
            super::super::FormatKind::Xml
        );
    }

    #[test]
    fn an_svg_file_name_makes_it_svg() {
        let text = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n<!-- svg file -->";
        super::super::hint_by_name("Logo.SVG", text);
        assert_eq!(super::super::detect(text), super::super::FormatKind::Svg);
    }

    #[test]
    fn dimensions_from_width_height_else_view_box() {
        assert_eq!(
            dimensions(r#"<svg width="48px" height="24" viewBox="0 0 1 1"/>"#).as_deref(),
            Some("48 × 24")
        );
        assert_eq!(
            dimensions(r#"<svg viewBox="0 0 100 50.5"/>"#).as_deref(),
            Some("100 × 50.5")
        );
        assert_eq!(
            dimensions(r#"<svg width="100%" viewBox="0,0,8,8"/>"#).as_deref(),
            Some("8 × 8")
        );
        assert_eq!(dimensions(r#"<svg stroke-width="2"/>"#), None);
    }

    #[test]
    fn outside_references_are_found() {
        assert!(!has_external_refs(
            r##"<svg><use href="#a"/><image href="data:image/png;base64,AA"/></svg>"##
        ));
        assert!(!has_external_refs(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect fill="url(#g)"/></svg>"#
        ));
        assert!(has_external_refs(
            r#"<svg><image xlink:href="https://x.example/a.png"/></svg>"#
        ));
        assert!(has_external_refs(
            r#"<svg><image href='file:///etc/a.png'/></svg>"#
        ));
        assert!(has_external_refs(
            r#"<svg><style>@import url(a.css);</style></svg>"#
        ));
        assert!(has_external_refs(
            r#"<svg><rect style="fill:url(http://x/p#g)"/></svg>"#
        ));
    }
}
