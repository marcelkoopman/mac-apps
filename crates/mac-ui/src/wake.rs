//! When the event loop should wake up next, from several optional deadlines.

use std::time::Instant;

use winit::event_loop::ControlFlow;

/// The earliest of `times`, ignoring `None`s; `None` when there is none.
pub fn earliest(times: impl IntoIterator<Item = Option<Instant>>) -> Option<Instant> {
    times.into_iter().flatten().min()
}

/// [`ControlFlow::WaitUntil`] the [`earliest`] of `times`, or [`ControlFlow::Wait`] without any.
pub fn control_flow(times: impl IntoIterator<Item = Option<Instant>>) -> ControlFlow {
    match earliest(times) {
        Some(at) => ControlFlow::WaitUntil(at),
        None => ControlFlow::Wait,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use winit::event_loop::ControlFlow;

    use super::{control_flow, earliest};

    #[test]
    fn earliest_skips_missing_deadlines() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(10);
        let later = now + Duration::from_millis(400);
        assert_eq!(earliest([Some(later), None, Some(soon)]), Some(soon));
        assert_eq!(earliest([None, None]), None);
        assert_eq!(earliest(std::iter::empty()), None);
    }

    #[test]
    fn control_flow_waits_until_the_earliest_or_forever() {
        let at = Instant::now() + Duration::from_secs(1);
        assert_eq!(control_flow([None, Some(at)]), ControlFlow::WaitUntil(at));
        assert_eq!(control_flow([None]), ControlFlow::Wait);
    }
}
