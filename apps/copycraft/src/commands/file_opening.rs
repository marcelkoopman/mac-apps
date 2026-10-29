//! A dropped or chosen file (⌘O) on its way: pure logic; the app ([`crate::menubar`]) and the
//! card draw it. The file is read on a thread; the card shows a spinner with "Opening file…"
//! once that takes longer than [`OPENING_SPINNER_DELAY_MS`] (a small file does not flash it).
//! A table file (CSV, TSV, a JSON array of objects) then opens in the table window: the card
//! keeps the spinner until the window has the grid, and only then gives way to it. Another drop
//! or a new copy meanwhile makes the file stale. Nothing here is logged.

use std::time::Duration;

/// An opening quicker than this shows no spinner on the card.
pub const OPENING_SPINNER_DELAY_MS: u64 = 150;

/// Where an opening is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningPhase {
    /// The file is being read (and its type worked out) on a thread.
    Reading,
    /// A table file: the table window is building its grid; the card waits for it.
    Window,
}

/// What the app does with a table window view that is ready ([`FileOpening::window_view`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowView {
    /// No opening waits for it: show it, as always.
    Show,
    /// The opened table is still worked on: keep the window hidden and the card's spinner
    /// (no empty "Working…" window between the card and the table).
    Hold,
    /// The table is there (or could not be read): show the window and let the card go; the
    /// opening is over.
    Reveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Opening {
    phase: OpeningPhase,
    token: u64,
}

/// The file opening going on, if any.
#[derive(Debug, Default)]
pub struct FileOpening {
    current: Option<Opening>,
    tokens: u64,
}

impl FileOpening {
    /// A file was dropped or chosen: its read begins. Replaces an opening going on (its read is
    /// stale). Returns the opening's token.
    pub fn begin(&mut self) -> u64 {
        self.tokens = self.tokens.wrapping_add(1);
        self.current = Some(Opening {
            phase: OpeningPhase::Reading,
            token: self.tokens,
        });
        self.tokens
    }

    /// The token of the opening going on.
    pub fn token(&self) -> Option<u64> {
        self.current.map(|opening| opening.token)
    }

    pub fn phase(&self) -> Option<OpeningPhase> {
        self.current.map(|opening| opening.phase)
    }

    pub fn active(&self) -> bool {
        self.current.is_some()
    }

    /// The card waits for the table window's grid.
    pub fn awaits_window(&self) -> bool {
        self.phase() == Some(OpeningPhase::Window)
    }

    /// The file is read and on the card. `table_window`: it opens in the table window, so the
    /// card keeps waiting (`true`); else the opening is over (`false`).
    pub fn read(&mut self, table_window: bool) -> bool {
        match self.current.as_mut() {
            Some(opening) if table_window && opening.phase == OpeningPhase::Reading => {
                opening.phase = OpeningPhase::Window;
                true
            }
            _ => {
                self.current = None;
                false
            }
        }
    }

    /// A table window view is ready. `working`: the table is still worked on; `error`: it
    /// could not be read or a step failed.
    pub fn window_view(&mut self, working: bool, error: bool) -> WindowView {
        if !self.awaits_window() {
            return WindowView::Show;
        }
        if working && !error {
            return WindowView::Hold;
        }
        self.current = None;
        WindowView::Reveal
    }

    /// The card shows its spinner now? Only for opening `token`, still going on, once
    /// `elapsed` reached [`OPENING_SPINNER_DELAY_MS`].
    pub fn spinner_due(&self, token: u64, elapsed: Duration) -> bool {
        self.token() == Some(token) && elapsed >= Duration::from_millis(OPENING_SPINNER_DELAY_MS)
    }

    /// A new copy came in, or the window went away first: the opening is over. Returns the
    /// phase it was in, so the caller drops the read (`Reading`) or the hidden window
    /// (`Window`).
    pub fn cancel(&mut self) -> Option<OpeningPhase> {
        self.current.take().map(|opening| opening.phase)
    }
}

/// Fade the table window's grid in: a grid arrived where none of this table was on screen (the
/// window opens, a dropped table comes in), and Reduce Motion is off.
pub fn grid_fades_in(had_grid: bool, has_grid: bool, reduce_motion: bool) -> bool {
    !had_grid && has_grid && !reduce_motion
}
