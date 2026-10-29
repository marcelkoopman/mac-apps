//! A version switch in the table window, drawn ([`commands::VersionSwitch`] decides). The card
//! asked for is framed at once; the grid stops taking clicks. After
//! [`commands::SWITCH_SPINNER_DELAY_MS`] the grid dims and a spinner shows over it; when the
//! version is there it fades in ([`commands::SWITCH_FADE_SECS`]), or just shows with Reduce
//! Motion (the spinner still shows). A switch started elsewhere (the card's capsule, a step)
//! shows the same once the app is seen working. Nothing is logged.

use std::time::Instant;

use mac_ui::objc2_app_kit::NSAnimatablePropertyContainer;
use mac_ui::progress::{Spinner, SpinnerSize};

use super::*;

thread_local! {
    static SWITCH: RefCell<commands::VersionSwitch> = RefCell::new(commands::VersionSwitch::default());
    /// When the switch going on began.
    static STARTED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The spinner over the grid, made once.
    static SPINNER: RefCell<Option<Spinner>> = const { RefCell::new(None) };
    /// The grid is dimmed under the spinner.
    static DIMMED: Cell<bool> = const { Cell::new(false) };
}

/// A switch is going on: the grid takes no clicks.
pub(super) fn busy() -> bool {
    SWITCH.with(|switch| switch.borrow().busy())
}

/// The window runs `id`: when it shows another version, frame its card now and start the
/// switch.
pub(super) fn asked(id: &CommandId) {
    let target = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|view| !view.grid.is_empty())
            .and_then(|view| commands::switch_target(id, &view.versions))
    });
    let Some(to) = target else {
        return;
    };
    SWITCH.with(|switch| switch.borrow_mut().begin(to));
    started();
    repaint_strip();
}

/// A view arrived showing `version` (`working`: the app still works on the table).
pub(super) fn arrive(version: usize, working: bool) -> commands::Arrival {
    let (arrival, began) = SWITCH.with(|switch| switch.borrow_mut().arrive(version, working));
    if began {
        started();
    }
    arrival
}

/// After the view was shown: end a settled switch, fading the new version in when it is
/// another one than `before` (the version on screen before).
pub(super) fn after_show(arrival: commands::Arrival, before: Option<usize>, after: usize) {
    if arrival == commands::Arrival::Pending {
        return;
    }
    let was_dimmed = DIMMED.get();
    finish();
    let Some(body) = grid_body() else {
        return;
    };
    let fades = commands::switch_fades(
        arrival,
        before,
        after,
        PAINTED.get(),
        mac_ui::motion::reduce_motion(),
    );
    if !fades {
        body.setAlphaValue(1.0);
        return;
    }
    // From the dimmed grid, or from a dip as the card's crossfade, back to full.
    body.setAlphaValue(if was_dimmed {
        commands::SWITCH_DIM_ALPHA
    } else {
        commands::SWITCH_FADE_FROM
    });
    let target = body.clone();
    mac_ui::motion::animate(
        commands::SWITCH_FADE_SECS,
        move || target.animator().setAlphaValue(1.0),
        None,
    );
}

/// The delayed check of the switch going on ([`started`]).
pub(super) fn tick() {
    let Some(began) = STARTED.get() else {
        return;
    };
    let elapsed = began.elapsed();
    let (due, gave_up) = SWITCH.with(|switch| {
        let mut switch = switch.borrow_mut();
        let Some(token) = switch.token() else {
            return (false, false);
        };
        if switch.give_up(token, elapsed) {
            (false, true)
        } else {
            (switch.spinner_due(token, elapsed), false)
        }
    });
    if gave_up {
        // The app never took the click: show what the window has again.
        finish();
        if let Some(body) = grid_body() {
            body.setAlphaValue(1.0);
        }
        paint();
    } else if due {
        show_busy();
    }
}

/// The window was wiped or closed: no switch, no spinner, the grid at full again.
pub(super) fn reset() {
    SWITCH.with(|switch| switch.borrow_mut().reset());
    finish();
    if let Some(body) = grid_body() {
        body.setAlphaValue(1.0);
    }
}

/// The strip again, framing the card asked for.
fn repaint_strip() {
    let masked = masked();
    VIEWS.with(|slot| {
        let Ok(borrowed) = slot.try_borrow() else {
            return;
        };
        let Some(views) = borrowed.as_ref() else {
            return;
        };
        SHOWN.with(|shown| {
            if let Some(view) = shown.borrow().as_ref() {
                strip::paint(views, view, masked);
            }
        });
    });
}

/// The version the strip frames: the one asked for while a switch runs.
pub(super) fn framed() -> Option<usize> {
    SWITCH.with(|switch| switch.borrow().target())
}

/// A switch began now: the grid takes no clicks, and the delayed checks are set.
fn started() {
    STARTED.set(Some(Instant::now()));
    set_grid_selectable(false);
    let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) else {
        return;
    };
    mac_ui::actions::cancel_perform(&delegate, sel!(tableWindowSwitchTick:));
    let ms = |ms: u64| ms as f64 / 1000.0;
    mac_ui::actions::perform_after(
        &delegate,
        sel!(tableWindowSwitchTick:),
        ms(commands::SWITCH_SPINNER_DELAY_MS),
    );
    mac_ui::actions::perform_after(
        &delegate,
        sel!(tableWindowSwitchTick:),
        ms(commands::SWITCH_GIVE_UP_MS),
    );
}

/// The switch ended: no spinner, no pending checks, the grid takes clicks again (its alpha is
/// the caller's).
fn finish() {
    STARTED.set(None);
    DIMMED.set(false);
    if let Some(delegate) = DELEGATE.with(|slot| slot.borrow().clone()) {
        mac_ui::actions::cancel_perform(&delegate, sel!(tableWindowSwitchTick:));
    }
    SPINNER.with(|slot| {
        if let Some(spinner) = slot.borrow().as_ref() {
            spinner.remove();
        }
    });
    set_grid_selectable(true);
}

/// The switch takes a while: dim the grid and spin over it.
fn show_busy() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let placed = VIEWS.with(|slot| {
        let borrowed = slot.borrow();
        let views = borrowed.as_ref()?;
        let parent = mac_ui::widgets::superview(&views.well)?;
        Some((parent, views.well.frame()))
    });
    let Some((parent, area)) = placed else {
        return;
    };
    if let Some(body) = grid_body() {
        body.setAlphaValue(commands::SWITCH_DIM_ALPHA);
    }
    DIMMED.set(true);
    SPINNER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let spinner = slot.get_or_insert_with(|| {
            let spinner = Spinner::new(mtm, SpinnerSize::Regular);
            spinner.set_accessibility_label(crate::locale::t("table_version_loading"));
            spinner
        });
        spinner.add_centered(&parent, area);
        spinner.start();
    });
}

/// The view around the grid's scroller and its header: what dims and fades.
fn grid_body() -> Option<Retained<NSView>> {
    VIEWS.with(|slot| {
        let borrowed = slot.try_borrow().ok()?;
        let views = borrowed.as_ref()?;
        let scroll = views.grid.enclosingScrollView()?;
        mac_ui::widgets::superview(&scroll)
    })
}

fn set_grid_selectable(selectable: bool) {
    VIEWS.with(|slot| {
        if let Ok(borrowed) = slot.try_borrow()
            && let Some(views) = borrowed.as_ref()
        {
            views.grid.setSelectable(selectable);
        }
    });
}
