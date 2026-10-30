//! A dropped or chosen file (⌘O) on its way: pure logic; the app ([`crate::menubar`]) and the
//! card draw it. The file is read on a thread; the card shows a spinner with "Opening file…"
//! once that takes longer than [`OPENING_SPINNER_DELAY_MS`] (a small file does not flash it).
//! Once read, the file is on the card and the opening is over, a table file (CSV, TSV, a JSON
//! array of objects) too: its card offers Open Table, and the table window opens only from
//! there. Another drop or a new copy meanwhile makes the file stale. Nothing here is logged.

use std::time::Duration;

/// An opening quicker than this shows no spinner on the card.
pub const OPENING_SPINNER_DELAY_MS: u64 = 150;

/// The file opening going on, if any: its token.
#[derive(Debug, Default)]
pub struct FileOpening {
    current: Option<u64>,
    tokens: u64,
}

impl FileOpening {
    /// A file was dropped or chosen: its read begins. Replaces an opening going on (its read is
    /// stale). Returns the opening's token.
    pub fn begin(&mut self) -> u64 {
        self.tokens = self.tokens.wrapping_add(1);
        self.current = Some(self.tokens);
        self.tokens
    }

    /// The token of the opening going on.
    pub fn token(&self) -> Option<u64> {
        self.current
    }

    pub fn active(&self) -> bool {
        self.current.is_some()
    }

    /// The file is read and on the card: the opening is over. Returns whether one was going on
    /// (not given up meanwhile).
    pub fn read(&mut self) -> bool {
        self.current.take().is_some()
    }

    /// The card shows its spinner now? Only for opening `token`, still going on, once
    /// `elapsed` reached [`OPENING_SPINNER_DELAY_MS`].
    pub fn spinner_due(&self, token: u64, elapsed: Duration) -> bool {
        self.token() == Some(token) && elapsed >= Duration::from_millis(OPENING_SPINNER_DELAY_MS)
    }

    /// A new copy came in, or the read could not start: the opening is over. Returns whether
    /// one was going on, so the caller drops its read.
    pub fn cancel(&mut self) -> bool {
        self.current.take().is_some()
    }
}

/// Fade the table window's grid in: a grid arrived where none of this table was on screen (the
/// window opens, another table comes in), and Reduce Motion is off.
pub fn grid_fades_in(had_grid: bool, has_grid: bool, reduce_motion: bool) -> bool {
    !had_grid && has_grid && !reduce_motion
}
