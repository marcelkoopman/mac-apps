//! The Preview button's busy state. A click writes the picture on a thread, then asks
//! `NSWorkspace` to open it in Preview, which can take a few seconds while Preview starts. Until
//! Preview has it (or that failed, a newer preview or Wipe/lock replaced it, or
//! [`PREVIEW_WAIT_MAX`] passed) the button shows a small spinner and takes no second click.
//! Pure logic, so it is tested on every platform; the spinner itself is
//! `macos_launcher::set_preview_busy`.

use std::time::{Duration, Instant};

/// After this long the spinner goes even without an answer from Preview.
pub(crate) const PREVIEW_WAIT_MAX: Duration = Duration::from_secs(15);

/// One Preview click being opened: its generation (`macos_open::begin_preview`) and start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreviewWait {
    pub(crate) generation: u64,
    pub(crate) started: Instant,
}

/// What [`PreviewWait::check`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitCheck {
    /// Still opening: look again by `due` at the latest.
    Waiting { due: Instant },
    /// Done waiting: take the spinner away.
    Over,
}

impl PreviewWait {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn new(generation: u64, now: Instant) -> Self {
        Self {
            generation,
            started: now,
        }
    }

    /// A new click starts a wait only when none runs (the button is dimmed meanwhile; command
    /// search can still ask).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn may_start(wait: Option<&Self>) -> bool {
        wait.is_none()
    }

    /// Preview's answer (or the write's failure) for `generation` ends this wait; an answer
    /// for an older click does not.
    pub(crate) fn ends_with(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// `current` is false once Wipe, lock, sleep or quit moved the preview generation on.
    pub(crate) fn check(&self, current: bool, now: Instant) -> WaitCheck {
        let due = self.started + PREVIEW_WAIT_MAX;
        if !current || now >= due {
            WaitCheck::Over
        } else {
            WaitCheck::Waiting { due }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_click_waits_for_the_first() {
        let now = Instant::now();
        assert!(PreviewWait::may_start(None));
        let wait = PreviewWait::new(3, now);
        assert!(!PreviewWait::may_start(Some(&wait)));
    }

    #[test]
    fn only_its_own_answer_ends_the_wait() {
        let wait = PreviewWait::new(3, Instant::now());
        assert!(wait.ends_with(3));
        assert!(!wait.ends_with(2));
        assert!(!wait.ends_with(4));
    }

    #[test]
    fn waits_until_the_limit_then_gives_up() {
        let now = Instant::now();
        let wait = PreviewWait::new(1, now);
        assert_eq!(
            wait.check(true, now + Duration::from_secs(1)),
            WaitCheck::Waiting {
                due: now + PREVIEW_WAIT_MAX
            }
        );
        assert_eq!(wait.check(true, now + PREVIEW_WAIT_MAX), WaitCheck::Over);
    }

    #[test]
    fn wipe_or_lock_ends_the_wait_at_once() {
        let now = Instant::now();
        assert_eq!(PreviewWait::new(1, now).check(false, now), WaitCheck::Over);
    }
}
