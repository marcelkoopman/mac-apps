//! Paged view, like `more`: a long text one page at a time. The line index is worked out once
//! (off the main thread); a page is then a slice of the text.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::ops::Range;

/// Where each line starts, for paging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::new();
        if !text.is_empty() {
            starts.push(0);
            starts.extend(
                text.match_indices('\n')
                    .map(|(at, _)| at + 1)
                    .filter(|&at| at < text.len()),
            );
        }
        Self {
            starts,
            len: text.len(),
        }
    }

    pub fn lines(&self) -> usize {
        self.starts.len()
    }

    /// Pages of `per_page` lines; an empty text is one (empty) page.
    pub fn pages(&self, per_page: usize) -> usize {
        self.lines().div_ceil(per_page.max(1)).max(1)
    }

    /// Byte range of page `page` (0-based, clamped to the last page), without the final line
    /// break.
    pub fn page(&self, page: usize, per_page: usize) -> Range<usize> {
        let per_page = per_page.max(1);
        let page = page.min(self.pages(per_page) - 1);
        let first = page * per_page;
        let Some(&start) = self.starts.get(first) else {
            return self.len..self.len;
        };
        let end = self
            .starts
            .get(first + per_page)
            .map_or(self.len, |&next| next);
        start..end
    }

    /// How far the end of page `page` is through the text, in whole percent of lines.
    pub fn percent(&self, page: usize, per_page: usize) -> usize {
        let per_page = per_page.max(1);
        let lines = self.lines();
        if lines == 0 {
            return 100;
        }
        let page = page.min(self.pages(per_page) - 1);
        ((page + 1) * per_page).min(lines) * 100 / lines
    }
}

/// The page text without its last line break.
pub fn page_text(text: &str, range: Range<usize>) -> &str {
    let page = text.get(range).unwrap_or("");
    page.strip_suffix('\n')
        .map(|page| page.strip_suffix('\r').unwrap_or(page))
        .unwrap_or(page)
}

/// Lines that fit in `height` points at `line_height` each (at least one).
pub fn per_page(height: f64, line_height: f64) -> usize {
    if line_height <= 0.0 || !height.is_finite() {
        return 1;
    }
    ((height / line_height).floor() as usize).max(1)
}

/// After a resize to `new_per_page`, the page that holds the first line page `page` showed.
pub fn page_after_resize(page: usize, old_per_page: usize, new_per_page: usize) -> usize {
    page * old_per_page.max(1) / new_per_page.max(1)
}

/// "--More-- page 3 of 12 (23%)".
pub fn status(page: usize, pages: usize, percent: usize) -> String {
    crate::locale::tf("pager_status", &[&(page + 1), &pages, &percent])
}
