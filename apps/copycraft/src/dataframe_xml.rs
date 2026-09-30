fn xml_rows(text: &str) -> Option<Vec<Vec<String>>> {
    let body = text.trim();
    let children = top_level_children(body)?;
    if children.len() < 2 {
        return None;
    }
    let row_tag = most_common_tag(&children)?;
    let row_blocks: Vec<&str> = children
        .into_iter()
        .filter(|(tag, _)| *tag == row_tag)
        .map(|(_, block)| block)
        .collect();
    if row_blocks.len() < 2 {
        return None;
    }
    let mut headers: Vec<String> = Vec::new();
    let mut values: Vec<Vec<String>> = Vec::new();
    for block in row_blocks {
        let leaves = leaf_fields(block);
        if leaves.is_empty() {
            return None;
        }
        if headers.is_empty() {
            headers = leaves.iter().map(|(k, _)| k.clone()).collect();
        }
        let row: Vec<String> = headers
            .iter()
            .map(|header| {
                leaves
                    .iter()
                    .find(|(k, _)| k == header)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            })
            .collect();
        values.push(row);
    }
    let mut rows = vec![headers];
    rows.extend(values);
    Some(rows)
}

fn top_level_children(text: &str) -> Option<Vec<(&str, &str)>> {
    let start = text.find('<')?;
    let mut i = start;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if text[i..].starts_with("<?") || text[i..].starts_with("<!" ) {
            i = text[i..]
                .find('>')
                .map(|n| i + n + 1)
                .unwrap_or(bytes.len());
            continue;
        }
        break;
    }
    let open_end = text[i..].find('>')? + i;
    let open = &text[i + 1..open_end];
    if open.starts_with('/') || open.ends_with('/') {
        return None;
    }
    let root_name = open.split_whitespace().next()?.trim_end_matches('/');
    let close = format!("</{root_name}>");
    let close_at = text.rfind(&close)?;
    let inner = &text[open_end + 1..close_at];
    extract_direct_children(inner)
}

fn extract_direct_children(inner: &str) -> Option<Vec<(&str, &str)>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let bytes = inner.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] != b'<' {
            return None;
        }
        if inner[i..].starts_with("</") {
            break;
        }
        let tag_end = inner[i + 1..].find([' ', '>', '/'])? + i + 1;
        let tag = &inner[i + 1..tag_end];
        let open_close = inner[i..].find('>')? + i;
        if inner[..open_close].ends_with('/') {
            out.push((tag, &inner[i..open_close + 1]));
            i = open_close + 1;
            continue;
        }
        let close = format!("</{tag}>");
        let rel = inner[open_close + 1..].find(&close)?;
        let end = open_close + 1 + rel + close.len();
        out.push((tag, &inner[i..end]));
        i = end;
    }
    if out.is_empty() { None } else { Some(out) }
}

fn most_common_tag<'a>(children: &[(&'a str, &str)]) -> Option<&'a str> {
    let mut best: Option<(&str, usize)> = None;
    for (tag, _) in children {
        let count = children.iter().filter(|(t, _)| t == tag).count();
        if best.map(|(_, c)| count > c).unwrap_or(true) {
            best = Some((*tag, count));
        }
    }
    best.map(|(tag, _)| tag)
}

fn leaf_fields(block: &str) -> Vec<(String, String)> {
    let Some(children) = extract_direct_children(inner_of_element(block).unwrap_or("")) else {
        return Vec::new();
    };
    children
        .into_iter()
        .filter_map(|(tag, elem)| {
            let inner = inner_of_element(elem)?;
            if inner.contains('<') {
                return None;
            }
            Some((tag.to_string(), decode_basic_entities(inner.trim())))
        })
        .collect()
}

fn inner_of_element(elem: &str) -> Option<&str> {
    let open_end = elem.find('>')?;
    if elem[..open_end].ends_with('/') {
        return Some("");
    }
    let close_start = elem.rfind("</")?;
    Some(&elem[open_end + 1..close_start])
}

fn decode_basic_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn rows_to_csv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| escape_csv_cell(cell))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_csv_cell(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

include!("dataframe_util.rs");
