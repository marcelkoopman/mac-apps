use std::time::Duration;

use crate::commands::{
    Arrival, CommandId, SWITCH_GIVE_UP_MS, SWITCH_SPINNER_DELAY_MS, VersionBar, VersionSwitch,
    switch_fades, switch_target,
};

fn bar(current: usize, len: usize) -> VersionBar {
    VersionBar {
        labels: (0..len).map(|i| format!("step {i}")).collect(),
        current,
    }
}

#[test]
fn only_a_different_existing_version_is_a_switch() {
    let shown = bar(1, 3);
    assert_eq!(switch_target(&CommandId::TableVersion(2), &shown), Some(2));
    assert_eq!(switch_target(&CommandId::TableVersion(1), &shown), None);
    assert_eq!(switch_target(&CommandId::TableVersion(3), &shown), None);
    assert_eq!(switch_target(&CommandId::StepBack, &shown), Some(0));
    assert_eq!(switch_target(&CommandId::StepForward, &shown), Some(2));
    assert_eq!(switch_target(&CommandId::StepBack, &bar(0, 3)), None);
    assert_eq!(switch_target(&CommandId::StepForward, &bar(2, 3)), None);
    assert_eq!(switch_target(&CommandId::Copy, &shown), None);
}

#[test]
fn the_card_asked_for_stays_framed_until_its_version_arrives() {
    let mut switch = VersionSwitch::default();
    assert!(!switch.busy());
    switch.begin(2);
    assert!(switch.busy());
    assert_eq!(switch.target(), Some(2));
    // The app works on it: still on its way.
    assert_eq!(switch.arrive(0, true), (Arrival::Pending, false));
    assert_eq!(switch.target(), Some(2));
    // Done.
    assert_eq!(switch.arrive(2, false), (Arrival::Settled, false));
    assert!(!switch.busy());
    assert_eq!(switch.target(), None);
    // Nothing going on: views just show.
    assert_eq!(switch.arrive(2, false), (Arrival::Idle, false));
}

#[test]
fn a_result_from_before_another_click_is_stale() {
    let mut switch = VersionSwitch::default();
    let first = switch.begin(1);
    let second = switch.begin(3);
    assert_ne!(first, second);
    assert_eq!(switch.token(), Some(second));
    // A finished view of the first click (or of before both): not the answer yet.
    assert_eq!(switch.arrive(1, false), (Arrival::Pending, false));
    assert_eq!(switch.target(), Some(3));
    // Ticks of the first click do nothing.
    let late = Duration::from_millis(SWITCH_GIVE_UP_MS + 1);
    assert!(!switch.spinner_due(first, late));
    assert!(!switch.give_up(first, late));
    assert!(switch.busy());
    assert_eq!(switch.arrive(3, false), (Arrival::Settled, false));
}

#[test]
fn work_seen_then_finished_settles_whatever_version_it_shows() {
    // A step can fail or land elsewhere: the end of the work ends the switch.
    let mut switch = VersionSwitch::default();
    switch.begin(2);
    assert_eq!(switch.arrive(1, true), (Arrival::Pending, false));
    assert_eq!(switch.arrive(1, false), (Arrival::Settled, false));
}

#[test]
fn work_started_elsewhere_is_a_switch_too() {
    let mut switch = VersionSwitch::default();
    assert_eq!(switch.arrive(0, true), (Arrival::Pending, true));
    assert!(switch.busy());
    // No card asked for: the strip frames the version shown.
    assert_eq!(switch.target(), None);
    assert_eq!(switch.arrive(0, true), (Arrival::Pending, false));
    assert_eq!(switch.arrive(1, false), (Arrival::Settled, false));
}

#[test]
fn the_spinner_waits_so_a_quick_switch_does_not_flash() {
    let mut switch = VersionSwitch::default();
    let token = switch.begin(1);
    let early = Duration::from_millis(SWITCH_SPINNER_DELAY_MS - 1);
    let due = Duration::from_millis(SWITCH_SPINNER_DELAY_MS);
    assert!(!switch.spinner_due(token, early));
    assert!(switch.spinner_due(token, due));
    // Settled before the tick: no spinner.
    switch.arrive(1, false);
    assert!(!switch.spinner_due(token, due));
}

#[test]
fn a_click_the_app_never_takes_is_given_up() {
    let mut switch = VersionSwitch::default();
    let token = switch.begin(1);
    assert!(!switch.give_up(token, Duration::from_millis(SWITCH_GIVE_UP_MS - 1)));
    assert!(switch.give_up(token, Duration::from_millis(SWITCH_GIVE_UP_MS)));
    assert!(!switch.busy());
    // Work seen: a slow switch is never given up.
    let token = switch.begin(1);
    switch.arrive(0, true);
    assert!(!switch.give_up(token, Duration::from_secs(60)));
    assert!(switch.busy());
    switch.reset();
    assert!(!switch.busy());
}

#[test]
fn only_another_version_drawn_fades_and_not_with_reduce_motion() {
    assert!(switch_fades(Arrival::Settled, Some(0), 1, true, false));
    assert!(switch_fades(Arrival::Idle, Some(0), 1, true, false));
    assert!(!switch_fades(Arrival::Pending, Some(0), 1, true, false));
    assert!(!switch_fades(Arrival::Settled, Some(1), 1, true, false));
    assert!(!switch_fades(Arrival::Settled, None, 1, true, false));
    assert!(!switch_fades(Arrival::Settled, Some(0), 1, false, false));
    assert!(!switch_fades(Arrival::Settled, Some(0), 1, true, true));
}
