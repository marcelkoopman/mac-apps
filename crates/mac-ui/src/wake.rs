//! When the event loop should wake up next, from several optional deadlines; with the `wake`
//! feature also when the Mac sleeps and wakes ([`observe_sleep_wake`]).

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

#[cfg(all(target_os = "macos", feature = "wake"))]
pub use sleep_wake::{Power, SleepWakeObserver, observe_sleep_wake};

#[cfg(all(target_os = "macos", feature = "wake"))]
mod sleep_wake {
    use std::ptr::NonNull;
    use std::sync::Arc;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};

    /// A system power change.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Power {
        /// The Mac is about to sleep.
        WillSleep,
        /// The Mac woke up (a full wake, not a Power Nap dark wake).
        DidWake,
    }

    /// Keeps the observers registered; dropping it removes them.
    pub struct SleepWakeObserver {
        center: Retained<NSNotificationCenter>,
        tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    }

    impl Drop for SleepWakeObserver {
        fn drop(&mut self) {
            for token in &self.tokens {
                // SAFETY: `token` is an observer this center returned from
                // addObserverForName:object:queue:usingBlock:.
                unsafe { self.center.removeObserver(token.as_ref()) };
            }
        }
    }

    /// Call `handler` when the Mac goes to sleep and when it wakes up (NSWorkspace's own
    /// notification center). The calls come on the main operation queue (main thread); `handler`
    /// is `Send + Sync` anyway, so the block meets the "sendable" requirement of the API (for
    /// example a winit `EventLoopProxy` that turns the change into a user event).
    pub fn observe_sleep_wake(
        handler: impl Fn(Power) + Send + Sync + 'static,
    ) -> SleepWakeObserver {
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        let queue = NSOperationQueue::mainQueue();
        let handler = Arc::new(handler);
        // SAFETY: extern statics provided by AppKit, valid for the life of the process.
        let names = unsafe {
            [
                (NSWorkspaceWillSleepNotification, Power::WillSleep),
                (NSWorkspaceDidWakeNotification, Power::DidWake),
            ]
        };
        let tokens = names
            .into_iter()
            .map(|(name, power)| {
                let handler = Arc::clone(&handler);
                let block = RcBlock::new(move |_: NonNull<NSNotification>| handler(power));
                // SAFETY: no object filter; the block only captures a `Send + Sync` handler, so
                // it may run (and be released) on any thread.
                unsafe {
                    center.addObserverForName_object_queue_usingBlock(
                        Some(name),
                        None,
                        Some(&queue),
                        &block,
                    )
                }
            })
            .collect();
        SleepWakeObserver { center, tokens }
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
