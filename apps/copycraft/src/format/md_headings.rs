//! Heading counts for a Markdown card's meta line.

/// Level-1 and level-2 ATX headings: `#` / `##` at the start of a line (up to 3 spaces before),
/// then a space, a tab or the end of the line. Not inside ``` or ~~~ fences, and not a
/// `#hashtag`.
pub fn count(text: &str) -> (usize, usize) {
    let mut fence: Option<(u8, usize)> = None;
    let (mut h1, mut h2) = (0, 0);
    for line in text.lines() {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let rest = &line[indent..];
        if let Some((mark, len)) = fence_of(rest) {
            match fence {
                None => fence = Some((mark, len)),
                Some((open, open_len))
                    if open == mark && len >= open_len && rest[len..].trim().is_empty() =>
                {
                    fence = None;
                }
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let level = rest.bytes().take_while(|&c| c == b'#').count();
        let after = rest.as_bytes().get(level);
        if matches!(after, None | Some(b' ' | b'\t')) {
            match level {
                1 => h1 += 1,
                2 => h2 += 1,
                _ => {}
            }
        }
    }
    (h1, h2)
}

/// A fence line: 3 or more backticks or tildes.
fn fence_of(line: &str) -> Option<(u8, usize)> {
    let mark = *line.as_bytes().first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let len = line.bytes().take_while(|&c| c == mark).count();
    (len >= 3).then_some((mark, len))
}

#[cfg(test)]
mod tests {
    use super::count;

    const DOC: &str = "# Title\n\nIntro with a #hashtag.\n\n## One\n\n   ## Indented two\n\n    # four spaces is code\n\n```bash\n# a shell comment\n## not a heading\n```\n\n~~~\n# also code\n~~~\n\n##\n\n### Three\n#nospace\n## Two again\n";

    #[test]
    fn counts_atx_headings_outside_fences() {
        assert_eq!(count(DOC), (1, 4));
        assert_eq!(count("no headings\njust text\n"), (0, 0));
    }

    #[test]
    fn markdown_meta_line_shows_heading_counts() {
        let meta = crate::commands::text_meta(DOC);
        assert!(meta.contains(" · 1 # · 4 ## · "), "{meta}");
        // A level without headings is left out.
        let only_h2 = "## A\n\nSome **bold** text and a [link](https://example.com).\n\n## B\n\n- item\n- item\n";
        assert_eq!(
            crate::format::detect(only_h2),
            crate::format::FormatKind::Markdown
        );
        let meta = crate::commands::text_meta(only_h2);
        assert!(
            meta.contains(" · 2 ## · ") && !meta.contains(" # ·"),
            "{meta}"
        );
    }

    #[test]
    fn an_md_file_name_makes_it_markdown() {
        let text = "plain words in notes.md\nsecond line\n";
        crate::format::hint_by_name("notes.MD", text);
        assert_eq!(
            crate::format::detect(text),
            crate::format::FormatKind::Markdown
        );
    }
}
