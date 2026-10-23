//! Whether a foreign pasteboard change is read. Pause, ignore-next and an ignored
//! app are decided here, before any clipboard bytes are touched. Nothing in this
//! module is stored in user defaults: a crash cannot leave recording off.

use std::sync::Mutex;

/// Bundle id of this app. An ignore list that names it is dropped, and it is never
/// remembered as the last foreign app.
pub const OWN_BUNDLE: &str = "nl.marcelkoopman.copycraft";

/// Why a foreign change was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Paused,
    IgnoreNext,
    IgnoredApp,
}

/// What to do with one foreign change. `consume_ignore_next` is set only on the
/// skip that uses the flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub skip: bool,
    pub reason: Option<Reason>,
    pub consume_ignore_next: bool,
}

/// Pure skip rule. `own` writes are never skipped. A change already skipped stays
/// skipped, and pause does not consume ignore-next.
pub fn decide(
    paused: bool,
    ignore_next: bool,
    own: bool,
    bundle_ignored: bool,
    change: isize,
    already_skipped: Option<isize>,
) -> Decision {
    if own {
        return Decision {
            skip: false,
            reason: None,
            consume_ignore_next: false,
        };
    }
    if already_skipped == Some(change) {
        return Decision {
            skip: true,
            reason: Some(if paused {
                Reason::Paused
            } else {
                Reason::IgnoreNext
            }),
            consume_ignore_next: false,
        };
    }
    if paused {
        return Decision {
            skip: true,
            reason: Some(Reason::Paused),
            consume_ignore_next: false,
        };
    }
    if ignore_next {
        return Decision {
            skip: true,
            reason: Some(Reason::IgnoreNext),
            consume_ignore_next: true,
        };
    }
    if bundle_ignored {
        return Decision {
            skip: true,
            reason: Some(Reason::IgnoredApp),
            consume_ignore_next: false,
        };
    }
    Decision {
        skip: false,
        reason: None,
        consume_ignore_next: false,
    }
}

#[derive(Clone, Copy)]
struct Decided {
    change: isize,
    reason: Option<Reason>,
}

struct Session {
    paused: bool,
    ignore_next: bool,
    skipped_change: Option<isize>,
    last_reason: Option<Reason>,
    last_foreign: Option<String>,
    /// The change already decided, so a later look in the same tick does not flip it
    /// or consume ignore-next again.
    decided: Option<Decided>,
}

impl Session {
    #[cfg(test)]
    fn new() -> Self {
        Self {
            paused: false,
            ignore_next: false,
            skipped_change: None,
            last_reason: None,
            last_foreign: None,
            decided: None,
        }
    }
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    paused: false,
    ignore_next: false,
    skipped_change: None,
    last_reason: None,
    last_foreign: None,
    decided: None,
});

fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(|err| err.into_inner())
}

pub fn paused() -> bool {
    session().paused
}

pub fn set_paused(on: bool) {
    session().paused = on;
}

pub fn ignore_next() -> bool {
    session().ignore_next
}

pub fn set_ignore_next(on: bool) {
    session().ignore_next = on;
}

/// Remember the frontmost bundle id. Our own id is ignored, so Settings (itself
/// frontmost) can still add the previous app. An empty id changes nothing.
pub fn observe_front(bundle: Option<&str>) {
    let Some(bundle) = bundle.map(str::trim).filter(|id| !id.is_empty()) else {
        return;
    };
    if bundle == OWN_BUNDLE {
        return;
    }
    session().last_foreign = Some(bundle.to_string());
}

/// History takes nothing new: recording is paused, or the frontmost app (the last foreign
/// one, as CopyCraft's own card is in front while it is used) is on the ignore list. The gate
/// of [`crate::clipboard::ClipboardHistory`].
pub fn history_held() -> bool {
    // Pause and the ignored apps belong to watching the clipboard: with history off they hold
    // nothing back, so a paste or a drop always goes in.
    if !crate::history_mode::capture_controls_active(crate::settings::clipboard_history()) {
        return false;
    }
    let (paused, front) = {
        let slot = session();
        (slot.paused, slot.last_foreign.clone())
    };
    held(paused, front.as_deref(), crate::settings::bundle_ignored)
}

/// Recording is paused (Pause, while history is kept). An Image ▾ step's own copy is held by
/// this alone, not by an ignored front app.
pub fn history_paused() -> bool {
    crate::history_mode::capture_controls_active(crate::settings::clipboard_history())
        && session().paused
}

/// [`history_held`] from its parts.
pub fn held(paused: bool, front: Option<&str>, ignored: impl Fn(&str) -> bool) -> bool {
    paused || front.is_some_and(ignored)
}

pub fn last_foreign() -> Option<String> {
    session().last_foreign.clone()
}

/// `true` while recording is paused, or this change was already skipped. Own writes
/// are not foreign: callers check that before they ask.
pub fn blocks(change: isize) -> bool {
    let slot = session();
    slot.paused || slot.skipped_change == Some(change)
}

/// Decide a foreign change once. `None` means read it. The same change asked again
/// returns the same answer and does not consume ignore-next a second time.
pub fn consider(change: isize, bundle_ignored: bool) -> Option<Reason> {
    let mut slot = session();
    if let Some(decided) = slot.decided
        && decided.change == change
    {
        if slot.paused {
            return Some(Reason::Paused);
        }
        return decided.reason;
    }
    let decision = decide(
        slot.paused,
        slot.ignore_next,
        false,
        bundle_ignored,
        change,
        slot.skipped_change,
    );
    let reason = decision.reason.filter(|_| decision.skip);
    if decision.skip {
        let first = slot.decided.is_none_or(|decided| decided.change != change);
        slot.skipped_change = Some(change);
        slot.last_reason = reason;
        if decision.consume_ignore_next {
            slot.ignore_next = false;
        }
        if first {
            log_skip(reason);
        }
    }
    slot.decided = Some(Decided { change, reason });
    paused_over(slot.paused, reason)
}

/// Lock, sleep and user switch forget the last foreign app. Pause, ignore-next and
/// a skipped change stay, so the unread copy is not pulled back in after unlock.
pub fn on_session_end() {
    let mut slot = session();
    slot.last_foreign = None;
    slot.decided = None;
}

fn paused_over(paused: bool, reason: Option<Reason>) -> Option<Reason> {
    match reason {
        Some(_) if paused => Some(Reason::Paused),
        other => other,
    }
}

fn log_skip(reason: Option<Reason>) {
    match reason {
        // Never the app's bundle id: which apps are ignored stays private too.
        Some(Reason::Paused) => crate::diag::log(crate::diag::LogEvent::CopySkipped(
            crate::diag::Skip::Paused,
        )),
        Some(Reason::IgnoreNext) => crate::diag::log(crate::diag::LogEvent::CopySkipped(
            crate::diag::Skip::IgnoreNext,
        )),
        Some(Reason::IgnoredApp) => crate::diag::log(crate::diag::LogEvent::CopySkipped(
            crate::diag::Skip::IgnoredApp,
        )),
        None => {}
    }
}

#[cfg(test)]
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|err| err.into_inner());
    *session() = Session::new();
    guard
}

#[cfg(test)]
mod tests {
    use super::{Decision, Reason, decide};

    fn skip(reason: Reason, consume: bool) -> Decision {
        Decision {
            skip: true,
            reason: Some(reason),
            consume_ignore_next: consume,
        }
    }

    fn read() -> Decision {
        Decision {
            skip: false,
            reason: None,
            consume_ignore_next: false,
        }
    }

    #[test]
    fn own_writes_are_read() {
        assert_eq!(decide(true, true, true, true, 1, Some(1)), read());
    }

    #[test]
    fn pause_does_not_consume_ignore_next() {
        assert_eq!(
            decide(true, true, false, false, 3, None),
            skip(Reason::Paused, false)
        );
    }

    #[test]
    fn ignore_next_is_consumed_once() {
        assert_eq!(
            decide(false, true, false, false, 4, None),
            skip(Reason::IgnoreNext, true)
        );
        assert_eq!(
            decide(false, false, false, false, 4, Some(4)),
            skip(Reason::IgnoreNext, false)
        );
    }

    #[test]
    fn a_skipped_change_stays_skipped_after_unpause() {
        assert_eq!(
            decide(false, false, false, false, 8, Some(8)),
            skip(Reason::IgnoreNext, false)
        );
        assert_eq!(decide(false, false, false, false, 9, Some(8)), read());
    }

    #[test]
    fn an_ignored_bundle_skips_without_consuming_ignore_next() {
        assert_eq!(
            decide(false, true, false, true, 2, None),
            skip(Reason::IgnoreNext, true)
        );
        assert_eq!(
            decide(false, false, false, true, 2, None),
            skip(Reason::IgnoredApp, false)
        );
    }

    #[test]
    fn session_pause_keeps_ignore_next_and_a_skipped_change() {
        let _guard = super::exclusive();
        super::set_ignore_next(true);
        super::set_paused(true);
        assert_eq!(super::consider(5, false), Some(Reason::Paused));
        assert!(super::ignore_next());
        assert!(super::blocks(5));
        super::set_paused(false);
        assert_eq!(super::consider(5, false), Some(Reason::Paused));
        // The stored decision was pause. The change stays unread; ignore-next is still armed
        // because pause did not consume it. A newer change uses the flag.
        assert!(super::ignore_next());
        assert_eq!(super::consider(6, false), Some(Reason::IgnoreNext));
        assert!(!super::ignore_next());
        assert_eq!(super::consider(6, false), Some(Reason::IgnoreNext));
        assert_eq!(super::consider(7, false), None);
        assert!(super::blocks(6));
        assert!(!super::blocks(7));
    }

    #[test]
    fn our_bundle_is_not_the_last_foreign_app() {
        let _guard = super::exclusive();
        super::observe_front(Some(super::OWN_BUNDLE));
        assert!(super::last_foreign().is_none());
        super::observe_front(Some("com.apple.Safari"));
        super::observe_front(Some(super::OWN_BUNDLE));
        assert_eq!(super::last_foreign().as_deref(), Some("com.apple.Safari"));
        super::on_session_end();
        assert!(super::last_foreign().is_none());
        super::set_paused(true);
        assert_eq!(super::consider(1, false), Some(Reason::Paused));
        super::on_session_end();
        assert!(super::paused());
        assert!(super::blocks(1));
    }

    #[test]
    fn an_ignored_app_is_logged_by_bundle_id_only() {
        let _guard = super::exclusive();
        assert_eq!(super::consider(4, true), Some(Reason::IgnoredApp));
        assert_eq!(super::consider(4, true), Some(Reason::IgnoredApp));
    }
}
