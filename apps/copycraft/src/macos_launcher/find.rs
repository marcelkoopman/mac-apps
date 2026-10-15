//! Find inside the shown item.

use super::*;

pub(crate) fn item_find_on() -> bool {
    FIND_ON.get()
}

pub(crate) fn set_item_find(on: bool) {
    if !on {
        clear_item_query();
    }
    FIND_ON.set(on);
    apply_item_find_visible(on);
}

pub(crate) fn apply_item_find_visible(on: bool) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            field.setHidden(!on);
        }
    });
    ITEM_COUNT.with(|slot| {
        if let Some(label) = slot.borrow().as_ref() {
            label.setHidden(!on);
        }
    });
}

pub(crate) fn focus_item_field() {
    if !item_find_on() {
        return;
    }
    let field = ITEM_FIELD.with(|slot| slot.borrow().clone());
    let window = WINDOW.with(|slot| slot.borrow().clone());
    if let (Some(field), Some(window)) = (field, window) {
        let _ = window.makeFirstResponder(Some(&*field));
    }
}

pub(crate) fn is_item_find_chord(event: &NSEvent) -> bool {
    mac_ui::keys::is_command_chord(event, "f")
}

pub(crate) fn note_is_item_field(note: &NSNotification) -> bool {
    let Some(object) = note.object() else {
        return false;
    };
    ITEM_FIELD.with(|slot| {
        slot.borrow().as_ref().is_some_and(|field| {
            std::ptr::eq(
                field.as_ref() as *const NSSearchField as *const AnyObject,
                &*object as *const AnyObject,
            )
        })
    })
}

pub(crate) fn control_is_item_field(control: &NSControl) -> bool {
    ITEM_FIELD.with(|slot| {
        slot.borrow().as_ref().is_some_and(|field| {
            std::ptr::eq(
                field.as_ref() as *const NSSearchField as *const NSControl,
                control as *const NSControl,
            )
        })
    })
}

pub(crate) fn item_field_command(command: Sel) -> bool {
    if command == sel!(insertNewline:) || command == sel!(insertNewlineIgnoringFieldEditor:) {
        step_item_match(!mac_ui::keys::shift_held());
        true
    } else if (command == sel!(moveLeft:) || command == sel!(moveRight:)) && item_query().is_empty()
    {
        // An empty find field has no caret to move: ← → step through history.
        arrow_command(command).is_some_and(|key| on_arrow(key, false, commands::ArrowFocus::Card))
    } else if command == sel!(cancelOperation:) {
        if item_query().is_empty() {
            if SEARCHING.with(Cell::get) {
                close_search();
            } else {
                hide();
            }
        } else {
            clear_item_query();
        }
        true
    } else {
        false
    }
}

pub(crate) fn search_body(
    shows_image: bool,
    linked: bool,
    excerpt: &str,
    placeholder: &str,
) -> String {
    if shows_image || linked {
        String::new()
    } else if !excerpt.is_empty() {
        excerpt.to_string()
    } else {
        placeholder.to_string()
    }
}

pub(crate) fn publish_search_text() {
    let shows_image = SHOWS_IMAGE.with(Cell::get);
    let linked = LINK_PAGE.with(|slot| slot.borrow().is_some());
    let mut excerpt = CARD_EXCERPT.with(|slot| slot.borrow().clone());
    let mut placeholder = CARD_PLACEHOLDER.with(|slot| slot.borrow().clone());
    let text = search_body(shows_image, linked, &excerpt, &placeholder);
    excerpt.zeroize();
    placeholder.zeroize();
    set_item_text(text);
}

pub(crate) fn set_item_text(mut next: String) {
    let changed = ITEM_TEXT.with(|slot| {
        let mut text = slot.borrow_mut();
        if *text == next {
            next.zeroize();
            return false;
        }
        text.zeroize();
        *text = next;
        true
    });
    if changed {
        ITEM_GEN.set(ITEM_GEN.get().wrapping_add(1));
        FIND_CACHE.with(|slot| slot.borrow_mut().take());
    }
}

#[cfg(test)]
pub(crate) fn current_item_text() -> String {
    ITEM_TEXT.with(|slot| slot.borrow().clone())
}

/// [`ITEM_TEXT`] folded for find ([`item_find::fold`]) and the matches of `query` in it.
pub(crate) struct FindCache {
    pub(crate) item_gen: u64,
    pub(crate) folded: Zeroizing<Vec<u8>>,
    pub(crate) query: Zeroizing<String>,
    pub(crate) matches: item_find::Matches,
}

/// The matches of `query` in the current item, folding the item only when it changed and
/// searching only when the query did. The item's text is borrowed, not copied.
pub(crate) fn with_item_matches<R>(query: &str, f: impl FnOnce(&FindCache) -> R) -> R {
    FIND_CACHE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let item_gen = ITEM_GEN.get();
        if slot.as_ref().is_none_or(|cache| cache.item_gen != item_gen) {
            let folded = ITEM_TEXT.with(|text| item_find::fold(&text.borrow()));
            *slot = Some(FindCache {
                item_gen,
                folded,
                query: Zeroizing::new(String::new()),
                matches: item_find::Matches::default(),
            });
        }
        let cache = slot.as_mut().expect("find cache was just filled");
        if cache.query.as_str() != query {
            cache.matches = item_find::find(&cache.folded, query);
            cache.query.zeroize();
            cache.query.push_str(query);
        }
        f(cache)
    })
}

pub(crate) fn item_query() -> String {
    ITEM_QUERY.with(|slot| slot.borrow().clone())
}

pub(crate) fn set_item_query(text: &str) {
    ITEM_QUERY.with(|slot| {
        let mut query = slot.borrow_mut();
        query.zeroize();
        query.push_str(text);
    });
    FIND_INDEX.set(0);
    if text.is_empty() {
        wipe_item_field();
    } else if MainThreadMarker::new().is_some() {
        ITEM_FIELD.with(|slot| {
            if let Some(field) = slot.borrow().as_ref() {
                field.setStringValue(&NSString::from_str(text));
            }
        });
    }
    refresh_item_marks(true);
}

pub(crate) fn clear_item_query() {
    set_item_query("");
}

pub(crate) fn take_item_query_from_field() {
    let mut next = ITEM_FIELD.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|field| field.stringValue().to_string())
            .unwrap_or_default()
    });
    ITEM_QUERY.with(|slot| {
        let mut query = slot.borrow_mut();
        query.zeroize();
        query.push_str(&next);
    });
    FIND_INDEX.set(0);
    next.zeroize();
    refresh_item_marks(true);
}

pub(crate) fn step_item_match(forward: bool) {
    let mut query = item_query();
    let total = if query.is_empty() {
        0
    } else {
        with_item_matches(&query, |cache| cache.matches.total())
    };
    query.zeroize();
    if total == 0 {
        return;
    }
    let index = FIND_INDEX.get().min(total - 1);
    FIND_INDEX.set(step_match(index, total, forward));
    refresh_item_marks(true);
}

pub(crate) fn refresh_item_marks(scroll: bool) {
    let mut query = item_query();
    if query.is_empty() {
        // Nothing to find: no fold of the item, no scan, just clear the marks.
        FIND_INDEX.set(0);
        if MainThreadMarker::new().is_some() {
            paint_match_marks(&[], None, false);
            set_match_label("");
        }
        return;
    }
    let shown = PAINTED_ITEM.get() == Some(ITEM_GEN.get());
    let (total, painted) = with_item_matches(&query, |cache| {
        let total = cache.matches.total();
        if total == 0 || FIND_INDEX.get() >= total {
            FIND_INDEX.set(0);
        }
        let painted =
            shown.then(|| item_find::painted(&cache.folded, &cache.matches, FIND_INDEX.get()));
        (total, painted)
    });
    let index = FIND_INDEX.get();
    let label = match_label(&query, index, total);
    query.zeroize();
    if MainThreadMarker::new().is_some() {
        match painted {
            Some((ranges, current)) => {
                paint_match_marks(&ranges, Some(current), scroll && total > 0);
            }
            None => paint_match_marks(&[], None, false),
        }
        set_match_label(&label);
    }
}

/// The item field changed. A long item is searched once typing pauses for
/// [`item_find::DEBOUNCE`]; a short one right away.
pub(crate) fn item_query_changed(delegate: &LauncherDelegate) {
    let long = ITEM_TEXT.with(|text| text.borrow().len() >= item_find::DEBOUNCE_FROM);
    if !long {
        take_item_query_from_field();
        return;
    }
    mac_ui::actions::cancel_perform(delegate, sel!(findAfterPause:));
    mac_ui::actions::perform_after(
        delegate,
        sel!(findAfterPause:),
        item_find::DEBOUNCE.as_secs_f64(),
    );
}

pub(crate) fn set_match_label(label: &str) {
    ITEM_COUNT.with(|slot| {
        if let Some(count) = slot.borrow().as_ref() {
            count.setStringValue(&NSString::from_str(label));
        }
    });
}

pub(crate) fn paint_match_marks(ranges: &[(usize, usize)], current: Option<usize>, scroll: bool) {
    let view = PREVIEW_TEXT.with(|slot| slot.borrow().clone());
    let Some(view) = view else {
        return;
    };
    // Line numbers sit in the text, so the body's offsets move.
    let moved: Vec<(usize, usize)> = PAINTED.with(|slot| match slot.borrow().as_ref() {
        Some(painted) if painted.numbers => ranges
            .iter()
            .map(|&(location, length)| commands::gutter_range(&painted.text, location, length))
            .collect(),
        _ => ranges.to_vec(),
    });
    widgets::mark_ranges(&view, &moved, current, scroll);
}
