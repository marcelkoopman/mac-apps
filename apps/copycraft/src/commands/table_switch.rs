//! A version switch in the table window (a card in the version strip, ⌘Z / ⇧⌘Z, the card's
//! version capsule, a step): pure logic; `macos_table_window/switching.rs` draws it. The
//! version is worked out on a thread and its grid laid out on another, so the window stays
//! responsive. While that runs the card asked for is framed at once; after
//! [`SWITCH_SPINNER_DELAY_MS`] the grid dims and a spinner shows; when the version is there it
//! fades in (no fade with Reduce Motion). Nothing here is logged.

use std::time::Duration;

use super::{CommandId, VersionBar};

/// A switch quicker than this shows no spinner and does not dim the grid (no flash).
pub const SWITCH_SPINNER_DELAY_MS: u64 = 150;
/// A switch that the app never started (nothing worked on within this) is given up: the
/// window shows what it has again.
pub const SWITCH_GIVE_UP_MS: u64 = 2_000;
/// How far the grid dims while a slow switch runs.
pub const SWITCH_DIM_ALPHA: f64 = 0.45;
/// The new version fades in over this (from the dimmed grid, or from [`SWITCH_FADE_FROM`]).
pub const SWITCH_FADE_SECS: f64 = 0.2;
/// Where the fade of a quick switch starts: the card's crossfade dip
/// (`mac_ui::motion::crossfade`).
pub const SWITCH_FADE_FROM: f64 = 0.35;

/// The version `id` asks the window for, from the version `bar` shows: `None` when it is no
/// version switch, or shows the version already on screen.
pub fn switch_target(id: &CommandId, bar: &VersionBar) -> Option<usize> {
    let len = bar.labels.len();
    let to = match id {
        CommandId::TableVersion(index) => *index,
        CommandId::StepBack => bar.current.checked_sub(1)?,
        CommandId::StepForward => bar.current + 1,
        // The version shown, or one before it, taken out: the one before the version shown
        // (the same version, one place to the left, without the step).
        CommandId::RemoveTableVersion(index) if *index > 0 && *index <= bar.current => {
            bar.current - 1
        }
        _ => return None,
    };
    (to < len && to != bar.current).then_some(to)
}

/// What the window does with a view that arrived ([`VersionSwitch::arrive`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// No switch was going on: show it as before.
    Idle,
    /// The switch is done: show it, undim, fade in (when it shows another version).
    Settled,
    /// Still on its way (the app is working, or this is an older result than the one asked
    /// for): show what it says, but keep the strip on the card asked for and the spinner.
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pending {
    /// The version asked for in the window; `None` for a switch started elsewhere (the card,
    /// a step) that the window only sees by the app working.
    to: Option<usize>,
    /// The app was seen working since the switch began: its next finished view is the answer.
    saw_working: bool,
    /// Bumped by every [`VersionSwitch::begin`]: a delayed tick for an older switch is dropped.
    token: u64,
}

/// The state of the window's version switch.
#[derive(Debug, Default)]
pub struct VersionSwitch {
    pending: Option<Pending>,
    tokens: u64,
}

impl VersionSwitch {
    /// The window asked for version `to`. Returns the switch's token. A switch already on its
    /// way is replaced: a finished view from before this click is stale until the app was seen
    /// working on it (or [`give_up`](Self::give_up)).
    pub fn begin(&mut self, to: usize) -> u64 {
        self.tokens = self.tokens.wrapping_add(1);
        self.pending = Some(Pending {
            to: Some(to),
            saw_working: false,
            token: self.tokens,
        });
        self.tokens
    }

    /// The version the strip frames while a switch runs (the one asked for).
    pub fn target(&self) -> Option<usize> {
        self.pending.and_then(|pending| pending.to)
    }

    /// A switch is going on.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// The token of the switch going on.
    pub fn token(&self) -> Option<u64> {
        self.pending.map(|pending| pending.token)
    }

    /// A view arrived showing `version`; `working`: the app is still working on the table.
    /// Returns what to do with it and whether a switch began just now (started elsewhere).
    pub fn arrive(&mut self, version: usize, working: bool) -> (Arrival, bool) {
        match self.pending.as_mut() {
            None if working => {
                self.tokens = self.tokens.wrapping_add(1);
                self.pending = Some(Pending {
                    to: None,
                    saw_working: true,
                    token: self.tokens,
                });
                (Arrival::Pending, true)
            }
            None => (Arrival::Idle, false),
            Some(pending) if working => {
                pending.saw_working = true;
                (Arrival::Pending, false)
            }
            // The version asked for, or the end of the work the app was seen doing (also when
            // it ended elsewhere, with an error, or another click took over).
            Some(pending) if pending.to == Some(version) || pending.saw_working => {
                self.pending = None;
                (Arrival::Settled, false)
            }
            // A result from before the click (the app has not taken the click yet): stale.
            Some(_) => (Arrival::Pending, false),
        }
    }

    /// A delayed tick of switch `token`, `elapsed` after it began: show the spinner (and dim)
    /// now? Only for the switch still going on.
    pub fn spinner_due(&self, token: u64, elapsed: Duration) -> bool {
        self.token() == Some(token) && elapsed >= Duration::from_millis(SWITCH_SPINNER_DELAY_MS)
    }

    /// Give a switch up that the app never started working on (it ignored the click): `true`
    /// when it was given up.
    pub fn give_up(&mut self, token: u64, elapsed: Duration) -> bool {
        let stuck = self.pending.is_some_and(|pending| {
            pending.token == token
                && !pending.saw_working
                && elapsed >= Duration::from_millis(SWITCH_GIVE_UP_MS)
        });
        if stuck {
            self.pending = None;
        }
        stuck
    }

    /// The window was wiped or closed.
    pub fn reset(&mut self) {
        self.pending = None;
    }
}

/// Fade the new version in: only when the switch settled on another version than the one on
/// screen before, the grid is drawn, and Reduce Motion is off.
pub fn switch_fades(
    arrival: Arrival,
    before: Option<usize>,
    after: usize,
    drawn: bool,
    reduce_motion: bool,
) -> bool {
    matches!(arrival, Arrival::Settled | Arrival::Idle)
        && before.is_some_and(|before| before != after)
        && drawn
        && !reduce_motion
}
