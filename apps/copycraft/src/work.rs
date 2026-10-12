//! Hand a background result to the main thread. The main thread may wait a moment for a quick
//! job (so a fast card shows at once, as before); when the job is slower it gives up, and the
//! worker then sends the result on as an event instead. Exactly one side ends up with it.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

enum Slot<T> {
    Waiting,
    Done(T),
    /// The main thread stopped waiting; the worker keeps its result.
    Gone,
}

/// One job's result, shared by the worker ([`Handoff::give`]) and the main thread
/// ([`Handoff::take_within`]).
pub struct Handoff<T>(Arc<(Mutex<Slot<T>>, Condvar)>);

impl<T> Clone for Handoff<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Default for Handoff<T> {
    fn default() -> Self {
        Self(Arc::new((Mutex::new(Slot::Waiting), Condvar::new())))
    }
}

impl<T> Handoff<T> {
    /// Worker side: leave `value` for the main thread. Returns it back when the main thread
    /// already stopped waiting, so the worker sends it on itself.
    pub fn give(&self, value: T) -> Option<T> {
        let (lock, ready) = &*self.0;
        let mut slot = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*slot, Slot::Gone) {
            return Some(value);
        }
        *slot = Slot::Done(value);
        ready.notify_one();
        None
    }

    /// Main side: the result if it is there within `wait`; otherwise `None`, and the worker
    /// keeps it ([`Handoff::give`] returns it).
    pub fn take_within(&self, wait: Duration) -> Option<T> {
        let (lock, ready) = &*self.0;
        let slot = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut slot, _) = ready
            .wait_timeout_while(slot, wait, |slot| matches!(slot, Slot::Waiting))
            .unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(&mut *slot, Slot::Gone) {
            Slot::Done(value) => Some(value),
            _ => None,
        }
    }
}

/// Newest-wins sequence numbers for one kind of job: a result whose number is not the newest
/// is stale and dropped.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Sequence(u64);

impl Sequence {
    /// The number for a new job; every older job is now stale.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }

    pub fn is_current(self, number: u64) -> bool {
        self.0 == number
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_result_is_taken_by_the_main_thread() {
        let handoff = Handoff::default();
        let worker = handoff.clone();
        let thread = std::thread::spawn(move || worker.give(7));
        assert_eq!(handoff.take_within(Duration::from_secs(5)), Some(7));
        assert_eq!(thread.join().unwrap(), None);
    }

    #[test]
    fn a_slow_result_stays_with_the_worker() {
        let handoff: Handoff<String> = Handoff::default();
        assert_eq!(handoff.take_within(Duration::from_millis(1)), None);
        assert_eq!(handoff.give("late".into()), Some("late".into()));
    }

    #[test]
    fn a_zero_wait_still_takes_a_finished_result() {
        let handoff = Handoff::default();
        assert_eq!(handoff.give(1), None);
        assert_eq!(handoff.take_within(Duration::ZERO), Some(1));
    }

    #[test]
    fn exactly_one_side_gets_the_value_under_races() {
        for round in 0..200u64 {
            let handoff = Handoff::default();
            let worker = handoff.clone();
            let thread = std::thread::spawn(move || worker.give(round));
            let main = handoff.take_within(Duration::from_micros(round % 7));
            let back = thread.join().unwrap();
            assert!(main.is_some() != back.is_some(), "round {round}");
        }
    }

    #[test]
    fn only_the_newest_number_is_current() {
        let mut sequence = Sequence::default();
        let first = sequence.next();
        let second = sequence.next();
        assert!(!sequence.is_current(first));
        assert!(sequence.is_current(second));
    }
}
