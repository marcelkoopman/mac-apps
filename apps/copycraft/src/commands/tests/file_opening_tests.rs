use std::time::Duration;

use crate::commands::{FileOpening, OPENING_SPINNER_DELAY_MS, grid_fades_in};

#[test]
fn a_read_file_ends_the_opening_table_or_not() {
    // A CSV, TSV or JSON table too: it is on the card with Open Table, no window waits.
    let mut opening = FileOpening::default();
    opening.begin();
    assert!(opening.active());
    assert!(opening.read());
    assert!(!opening.active());
    assert_eq!(opening.token(), None);
}

#[test]
fn the_spinner_shows_only_for_a_slow_opening_still_going_on() {
    let mut opening = FileOpening::default();
    let first = opening.begin();
    let early = Duration::from_millis(OPENING_SPINNER_DELAY_MS - 1);
    let due = Duration::from_millis(OPENING_SPINNER_DELAY_MS);
    assert!(!opening.spinner_due(first, early));
    assert!(opening.spinner_due(first, due));
    // Another drop: the first one's delayed check is stale.
    let second = opening.begin();
    assert!(!opening.spinner_due(first, due));
    assert!(opening.spinner_due(second, due));
    // Read: the spinner does not come (back) late, so "Opening file…" cannot stay up.
    opening.read();
    assert!(!opening.spinner_due(second, due));
}

#[test]
fn a_new_copy_cancels_the_opening() {
    let mut opening = FileOpening::default();
    opening.begin();
    assert!(opening.cancel());
    assert!(!opening.cancel());
    // A read that arrives after the cancel finds nothing going on.
    assert!(!opening.read());
    assert!(!opening.active());
}

#[test]
fn the_grid_fades_in_when_it_first_arrives_unless_reduce_motion() {
    assert!(grid_fades_in(false, true, false));
    assert!(!grid_fades_in(false, true, true));
    // The same table again (a repaint, a version switch has its own fade): no fade.
    assert!(!grid_fades_in(true, true, false));
    // Still no grid (Working…): nothing to fade.
    assert!(!grid_fades_in(false, false, false));
}
