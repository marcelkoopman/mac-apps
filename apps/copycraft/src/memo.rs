//! Remembered results of pure functions over clipboard text.
//!
//! Detecting the format or the sensitive data of a large copy takes milliseconds to hundreds of
//! milliseconds, and the card asks for both many times per redraw. A [`Memo`] keeps the last
//! few results keyed by the text's length and a 64-bit hash. The text itself is never stored;
//! [`forget_all`] drops the keys too (Wipe).

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

/// Shorter texts are cheap to scan, so they are not remembered.
const MIN_LEN: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    len: usize,
    hash: u64,
}

impl Key {
    fn of(text: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            len: text.len(),
            hash: hasher.finish(),
        }
    }
}

/// The last `capacity` results of one function, newest first.
pub struct Memo<V> {
    slots: Mutex<VecDeque<(Key, V)>>,
    capacity: usize,
    min_len: usize,
}

impl<V: Clone> Memo<V> {
    pub const fn new(capacity: usize) -> Self {
        Self::with_min_len(capacity, MIN_LEN)
    }

    /// Like [`new`](Self::new), also remembering texts shorter than [`MIN_LEN`] bytes (from
    /// `min_len` up), for a function that is slow whatever the length.
    pub const fn with_min_len(capacity: usize, min_len: usize) -> Self {
        Self {
            slots: Mutex::new(VecDeque::new()),
            capacity,
            min_len,
        }
    }

    /// The remembered result for `text`, else `compute(text)` (run without holding the lock,
    /// so another thread can compute meanwhile), remembered for next time.
    pub fn get_or_compute(&self, text: &str, compute: impl FnOnce(&str) -> V) -> V {
        if text.len() < self.min_len {
            return compute(text);
        }
        let key = Key::of(text);
        if let Some(value) = self.lookup(key) {
            return value;
        }
        let value = compute(text);
        self.remember(key, value.clone());
        value
    }

    fn lookup(&self, key: Key) -> Option<V> {
        let mut slots = self.slots.lock().ok()?;
        let index = slots.iter().position(|(k, _)| *k == key)?;
        let entry = slots.remove(index)?;
        let value = entry.1.clone();
        slots.push_front(entry);
        Some(value)
    }

    fn remember(&self, key: Key, value: V) {
        let Ok(mut slots) = self.slots.lock() else {
            return;
        };
        slots.retain(|(k, _)| *k != key);
        slots.push_front((key, value));
        slots.truncate(self.capacity);
    }

    pub fn clear(&self) {
        if let Ok(mut slots) = self.slots.lock() {
            slots.clear();
        }
    }
}

/// Drop every remembered result (Wipe), so not even a hash of the copy stays behind.
pub fn forget_all() {
    crate::format::forget_detected();
    crate::sensitivity::forget_labels();
    crate::commands::forget_chips();
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::Memo;

    fn big(fill: char) -> String {
        std::iter::repeat_n(fill, super::MIN_LEN + 10).collect()
    }

    #[test]
    fn large_text_is_computed_once() {
        let memo = Memo::new(2);
        let calls = Cell::new(0);
        let text = big('a');
        for _ in 0..3 {
            let value = memo.get_or_compute(&text, |t| {
                calls.set(calls.get() + 1);
                t.len()
            });
            assert_eq!(value, text.len());
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn short_text_is_not_remembered() {
        let memo = Memo::new(2);
        let calls = Cell::new(0);
        for _ in 0..2 {
            memo.get_or_compute("short", |_| calls.set(calls.get() + 1));
        }
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn oldest_result_is_dropped_past_capacity() {
        let memo = Memo::new(2);
        let calls = Cell::new(0);
        let count = |memo: &Memo<usize>, text: &str| {
            memo.get_or_compute(text, |t| {
                calls.set(calls.get() + 1);
                t.len()
            })
        };
        let (a, b, c) = (big('a'), big('b'), big('c'));
        count(&memo, &a);
        count(&memo, &b);
        count(&memo, &c);
        assert_eq!(calls.get(), 3);
        count(&memo, &c);
        count(&memo, &b);
        assert_eq!(calls.get(), 3);
        count(&memo, &a);
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn different_texts_of_the_same_length_do_not_share() {
        let memo = Memo::new(4);
        let a = big('a');
        let b = big('b');
        assert!(memo.get_or_compute(&a, |t| t.starts_with('a')));
        assert!(!memo.get_or_compute(&b, |t| t.starts_with('a')));
    }

    #[test]
    fn a_lower_min_len_remembers_short_text() {
        let memo = Memo::with_min_len(2, 0);
        let calls = Cell::new(0);
        for _ in 0..2 {
            memo.get_or_compute("short", |_| calls.set(calls.get() + 1));
        }
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn clear_forgets() {
        let memo = Memo::new(2);
        let calls = Cell::new(0);
        let text = big('x');
        memo.get_or_compute(&text, |_| calls.set(calls.get() + 1));
        memo.clear();
        memo.get_or_compute(&text, |_| calls.set(calls.get() + 1));
        assert_eq!(calls.get(), 2);
    }
}
