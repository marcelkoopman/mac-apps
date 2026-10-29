//! Take items off their owner for an edit and put the same items back after it, such as the
//! layout managers of a text storage while its text is wiped ([`crate::widgets`]).
//!
//! The items come as a snapshot taken before anything is removed: an owner may hand out its
//! own live list, which empties as the items come off, and putting back "whatever the list
//! holds now" would then put back nothing (a text view with no layout manager draws nothing).

/// Remove every item of `snapshot` with `remove`, run `edit`, then `add` each item back, in
/// the same order.
pub fn while_detached<M>(
    snapshot: &[M],
    mut remove: impl FnMut(&M),
    edit: impl FnOnce(),
    mut add: impl FnMut(&M),
) {
    for item in snapshot {
        remove(item);
    }
    edit();
    for item in snapshot {
        add(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn every_item_comes_back_after_the_edit() {
        // The owner's live list, emptied by `remove` like a text storage's layout managers.
        let owner = RefCell::new(vec![1, 2]);
        let edited = RefCell::new(None);
        let snapshot = owner.borrow().clone();
        while_detached(
            &snapshot,
            |item| owner.borrow_mut().retain(|kept| kept != item),
            || *edited.borrow_mut() = Some(owner.borrow().len()),
            |item| owner.borrow_mut().push(*item),
        );
        assert_eq!(*edited.borrow(), Some(0), "detached during the edit");
        assert_eq!(*owner.borrow(), vec![1, 2], "all back, in order");
    }

    #[test]
    fn nothing_to_detach_still_edits() {
        let mut edited = false;
        while_detached(&[] as &[u8], |_| {}, || edited = true, |_| {});
        assert!(edited);
    }
}
