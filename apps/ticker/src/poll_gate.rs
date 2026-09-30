//! Bookkeeping for the background price fetch: at most one fetch in flight, and results from a
//! fetch that was started before the asset config changed are dropped.
//!
//! Pure state (no threads or I/O) so the coalescing rules are unit-testable on any platform.

/// Generation tag handed to a fetch when it starts and returned with its result.
pub type Generation = u64;

#[derive(Debug, Default)]
pub struct PollGate {
    in_flight: bool,
    /// A fresh fetch must start as soon as the running one finishes (config changed mid-fetch).
    rerun_queued: bool,
    generation: Generation,
}

/// What to do with a finished fetch.
#[derive(Debug, PartialEq, Eq)]
pub struct Finished {
    /// The result belongs to the current config and should be applied.
    pub apply: bool,
    /// Start another fetch right away with this generation.
    pub restart: Option<Generation>,
}

impl PollGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Regular poll (timer or "Poll now"). Returns the generation to fetch with, or `None` when a
    /// fetch is already running; that one is just as fresh, so the request is skipped.
    pub fn try_start(&mut self) -> Option<Generation> {
        if self.in_flight {
            None
        } else {
            self.in_flight = true;
            Some(self.generation)
        }
    }

    /// The asset config changed (edit/reset): results of any running fetch are now stale. Starts a
    /// fetch now if idle, otherwise queues one to start when the running fetch finishes.
    pub fn invalidate(&mut self) -> Option<Generation> {
        self.generation = self.generation.wrapping_add(1);
        if self.in_flight {
            self.rerun_queued = true;
            None
        } else {
            self.in_flight = true;
            Some(self.generation)
        }
    }

    /// Call when the fetch tagged `generation` has finished (successfully or not).
    pub fn finish(&mut self, generation: Generation) -> Finished {
        self.in_flight = false;
        let apply = generation == self.generation;
        let restart = if std::mem::take(&mut self.rerun_queued) {
            self.in_flight = true;
            Some(self.generation)
        } else {
            None
        };
        Finished { apply, restart }
    }

    /// The fetch could not be started (thread spawn failed): release the slot.
    pub fn abort(&mut self) {
        self.in_flight = false;
        self.rerun_queued = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_poll_is_skipped_while_one_is_in_flight() {
        let mut gate = PollGate::new();
        let g = gate.try_start().expect("idle gate starts");
        assert!(gate.in_flight);
        assert_eq!(gate.try_start(), None);
        assert_eq!(gate.try_start(), None);
        assert_eq!(
            gate.finish(g),
            Finished {
                apply: true,
                restart: None
            }
        );
        assert!(!gate.in_flight);
        assert!(gate.try_start().is_some());
    }

    #[test]
    fn invalidate_when_idle_starts_immediately_with_new_generation() {
        let mut gate = PollGate::new();
        let first = gate.try_start().unwrap();
        gate.finish(first);
        let g = gate.invalidate().expect("idle gate starts");
        assert_ne!(g, first);
        assert!(gate.in_flight);
        assert_eq!(
            gate.finish(g),
            Finished {
                apply: true,
                restart: None
            }
        );
    }

    #[test]
    fn invalidate_mid_fetch_drops_stale_result_and_reruns_once() {
        let mut gate = PollGate::new();
        let stale = gate.try_start().unwrap();
        assert_eq!(gate.invalidate(), None);
        assert_eq!(gate.invalidate(), None); // several edits coalesce into one rerun
        assert_eq!(gate.try_start(), None);
        let done = gate.finish(stale);
        assert!(!done.apply);
        let fresh = done.restart.expect("queued rerun starts");
        assert_ne!(fresh, stale);
        assert!(gate.in_flight);
        assert_eq!(
            gate.finish(fresh),
            Finished {
                apply: true,
                restart: None
            }
        );
        assert!(!gate.in_flight);
    }

    #[test]
    fn abort_releases_the_slot() {
        let mut gate = PollGate::new();
        gate.try_start().unwrap();
        gate.invalidate();
        gate.abort();
        assert!(!gate.in_flight);
        assert!(gate.try_start().is_some());
    }
}
