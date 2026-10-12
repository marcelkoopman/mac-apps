//! Paged "Show all" for a long text, like `more` ([`crate::text_ops::pager`]). The line index
//! arrives worked out by the show-all thread; here a page is only a slice of the excerpt.
//! Space: next page, b: previous, g / G: first / last, Esc or q: the whole text again.

use std::cell::RefCell;
use std::sync::Arc;

use crate::text_ops::pager::{self, LineIndex};

use super::*;

struct Pager {
    key: u64,
    index: Arc<LineIndex>,
    page: usize,
    per_page: usize,
    /// Esc or q turned paging off for this item: it shows whole until another item.
    off: bool,
}

thread_local! {
    static PAGER: RefCell<Option<Pager>> = const { RefCell::new(None) };
}

/// Page the card for `key` with `index`, or stop paging (`None`). The same item keeps its page.
pub(crate) fn set_pages(key: u64, index: Option<Arc<LineIndex>>) {
    PAGER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(index) = index else {
            *slot = None;
            return;
        };
        let (page, per_page, off) = match slot.as_ref() {
            Some(old) if old.key == key && *old.index == *index => {
                (old.page, old.per_page, old.off)
            }
            _ => (0, 1, false),
        };
        *slot = Some(Pager {
            key,
            index,
            page,
            per_page,
            off,
        });
    });
}

pub(crate) fn paging() -> bool {
    PAGER.with(|slot| slot.borrow().as_ref().is_some_and(|pager| !pager.off))
}

/// The page to paint in a well `height` points tall, with the `--More--` status line under it.
/// A resize keeps the first line shown on screen.
pub(crate) fn page_body(excerpt: &str, height: f64) -> Option<String> {
    PAGER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let pager = slot.as_mut().filter(|pager| !pager.off)?;
        // The editor font is 12 pt; a line is about 15 pt. Two lines go to the status.
        let line_h = 15.0;
        let per_page = pager::per_page(height - WELL_INSET * 2.0 - line_h * 2.0, line_h);
        if per_page != pager.per_page {
            pager.page = pager::page_after_resize(pager.page, pager.per_page, per_page);
            pager.per_page = per_page;
        }
        let pages = pager.index.pages(per_page);
        pager.page = pager.page.min(pages - 1);
        let text = pager::page_text(excerpt, pager.index.page(pager.page, per_page));
        let status = pager::status(pager.page, pages, pager.index.percent(pager.page, per_page));
        Some(format!("{text}\n\n{status}"))
    })
}

/// Space, b, g, G, q and Esc while a long text is paged and no search is being typed. True
/// when the key was the pager's.
pub(crate) fn pager_key(event: &NSEvent) -> bool {
    if !paging() || SEARCHING.with(Cell::get) {
        return false;
    }
    let flags = event.modifierFlags();
    if flags.intersects(
        NSEventModifierFlags::Command
            | NSEventModifierFlags::Control
            | NSEventModifierFlags::Option,
    ) {
        return false;
    }
    let escape = Key::from_key_code(event.keyCode()) == Some(Key::Escape);
    let Some(chars) = event.characters().map(|chars| chars.to_string()) else {
        return false;
    };
    let handled = PAGER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(pager) = slot.as_mut() else {
            return false;
        };
        let last = pager.index.pages(pager.per_page) - 1;
        match chars.as_str() {
            _ if escape => pager.off = true,
            "q" => pager.off = true,
            " " => pager.page = (pager.page + 1).min(last),
            "b" => pager.page = pager.page.saturating_sub(1),
            "g" => pager.page = 0,
            "G" => pager.page = last,
            _ => return false,
        }
        true
    });
    if handled {
        layout(false);
    }
    handled
}
