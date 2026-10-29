use std::time::Duration;

use crate::commands::{
    FileOpening, OPENING_SPINNER_DELAY_MS, OpeningPhase, WindowView, grid_fades_in,
};

#[test]
fn a_file_that_is_not_a_table_ends_the_opening_once_it_is_read() {
    let mut opening = FileOpening::default();
    opening.begin();
    assert_eq!(opening.phase(), Some(OpeningPhase::Reading));
    assert!(!opening.read(false));
    assert!(!opening.active());
    // Views of a window opened another way are shown as always.
    assert_eq!(opening.window_view(true, false), WindowView::Show);
}

#[test]
fn the_card_waits_for_the_table_windows_grid() {
    let mut opening = FileOpening::default();
    opening.begin();
    assert!(opening.read(true));
    assert!(opening.awaits_window());
    // The table is still being read: no empty window, the card keeps its spinner.
    assert_eq!(opening.window_view(true, false), WindowView::Hold);
    assert!(opening.awaits_window());
    // The grid is there: the window shows and the card goes.
    assert_eq!(opening.window_view(false, false), WindowView::Reveal);
    assert!(!opening.active());
    assert_eq!(opening.window_view(false, false), WindowView::Show);
}

#[test]
fn a_table_that_cannot_be_read_still_shows_the_window_with_its_error() {
    let mut opening = FileOpening::default();
    opening.begin();
    opening.read(true);
    assert_eq!(opening.window_view(true, true), WindowView::Reveal);
    assert!(!opening.active());
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
    // Also while the card waits for the window.
    opening.read(true);
    assert!(opening.spinner_due(second, due));
    opening.window_view(false, false);
    assert!(!opening.spinner_due(second, due));
}

#[test]
fn a_new_copy_cancels_the_opening_in_either_phase() {
    let mut opening = FileOpening::default();
    opening.begin();
    assert_eq!(opening.cancel(), Some(OpeningPhase::Reading));
    assert_eq!(opening.cancel(), None);
    // A read that arrives after the cancel opens no window and waits for nothing.
    assert!(!opening.read(true));
    opening.begin();
    opening.read(true);
    assert_eq!(opening.cancel(), Some(OpeningPhase::Window));
    assert_eq!(opening.window_view(true, false), WindowView::Show);
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
