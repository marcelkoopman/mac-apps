//! Background work for the card, the table window, Copy and opened files: the work runs on a
//! thread, only AppKit and the clipboard stay on the main thread. Same pattern as Text ▾: a
//! sequence number per kind of job, and a stale result is dropped.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::*;
use crate::work::{Handoff, Sequence};

/// How long the main thread waits for a job before it lets the result come later: under a
/// frame, so a quick card or copy behaves exactly as when it was built on the main thread.
pub(crate) const QUICK: Duration = Duration::from_millis(30);

/// Kinds of background job; a newer job of a kind makes older ones stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lane {
    Card,
    TableWindow,
    Copy,
    File,
    /// Building [`SecretBytes`] for a newly copied picture (hash and any file read).
    Image,
    /// The Apple Intelligence model ([`AiRun`]).
    Ai,
    /// The clipboard's picture as the card shows it, for Image ▾ ([`App::card_picture`]).
    CardPicture,
}

impl Lane {
    /// Whether a slow job of this lane makes the card show its busy wheel. The AI run has its
    /// own spinner in its preview over the card: a second one would be drawn under it.
    pub(crate) fn shows_card_spinner(self) -> bool {
        // Image ▾'s picture read for the card shows no wheel either: the picture is on screen.
        !matches!(self, Lane::Ai | Lane::CardPicture)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Lanes {
    sequence: [Sequence; 7],
    /// When the newest job of a lane became slow (the main thread stopped waiting): the
    /// spinner shows after [`SPINNER_DELAY`], like other background work.
    slow: [Option<Instant>; 7],
}

impl Lanes {
    fn get(&mut self, lane: Lane) -> &mut Sequence {
        &mut self.sequence[lane as usize]
    }

    /// The start of the oldest slow job, if one is running.
    pub(crate) fn slow_since(&self) -> Option<Instant> {
        self.slow.iter().flatten().min().copied()
    }
}

type Finish = Box<dyn FnOnce(&mut App) + Send>;

/// Run `finish` on the main thread, from a worker: its progress or its result.
pub(crate) fn on_main(finish: impl FnOnce(&mut App) + Send + 'static) {
    let finish: Finish = Box::new(finish);
    launcher::emit(UserEvent::WorkDone(WorkDone(Arc::new(Mutex::new(Some(
        finish,
    ))))));
}

/// A background result on its way to the main thread ([`UserEvent::WorkDone`]).
#[derive(Clone)]
pub struct WorkDone(Arc<Mutex<Option<Finish>>>);

/// The result can hold copied text: never printed.
impl std::fmt::Debug for WorkDone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkDone")
    }
}

impl App {
    /// Run `compute` on a thread. When it is done within `wait` its result is returned here,
    /// on the main thread, and `later` is not used. Otherwise `None`, and `later` gets the
    /// result on the main thread when it arrives, unless a newer job of `lane` started.
    pub(crate) fn start_work<T: Send + 'static>(
        &mut self,
        lane: Lane,
        wait: Duration,
        compute: impl FnOnce() -> T + Send + 'static,
        later: impl FnOnce(&mut App, T) + Send + 'static,
    ) -> Option<T> {
        let number = self.lanes.get(lane).next();
        let handoff = Handoff::default();
        let worker = handoff.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("copycraft-{lane:?}").to_lowercase())
            .spawn(move || {
                let Some(value) = worker.give(compute()) else {
                    return;
                };
                let finish: Finish = Box::new(move |app: &mut App| {
                    if app.lanes.get(lane).is_current(number) {
                        app.lanes.slow[lane as usize] = None;
                        app.stop_spinner_when_idle();
                        later(app, value);
                    }
                });
                launcher::emit(UserEvent::WorkDone(WorkDone(Arc::new(Mutex::new(Some(
                    finish,
                ))))));
            });
        if let Err(e) = spawned {
            self.lanes.slow[lane as usize] = None;
            self.work_failed(crate::diag::Work::Lane, &e);
            return None;
        }
        self.work_error = None;
        let value = handoff.take_within(wait);
        self.lanes.slow[lane as usize] =
            (value.is_none() && lane.shows_card_spinner()).then(Instant::now);
        value
    }

    /// Run `compute` on a thread of its own and give its result to `later` on the main thread.
    /// No lane: nothing makes it stale (`later` checks that itself), and the card shows no
    /// spinner for it. `false` when the thread could not start (logged; `later` is not run).
    pub(crate) fn run_detached<T: Send + 'static>(
        &mut self,
        name: &str,
        compute: impl FnOnce() -> T + Send + 'static,
        later: impl FnOnce(&mut App, T) + Send + 'static,
    ) -> bool {
        let spawned = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let value = compute();
                let finish: Finish = Box::new(move |app: &mut App| later(app, value));
                launcher::emit(UserEvent::WorkDone(WorkDone(Arc::new(Mutex::new(Some(
                    finish,
                ))))));
            });
        match spawned {
            Ok(_) => true,
            Err(e) => {
                crate::diag::log(crate::diag::LogEvent::WorkNotStarted(
                    crate::diag::Work::Lane,
                    crate::diag::ErrorKind::io(&e),
                ));
                false
            }
        }
    }

    /// A worker thread could not start: say so on the card (NL/EN), drop the Working… state
    /// and the spinner. The card is refreshed once, from the event loop (never from inside a
    /// card update), so a failing refresh cannot loop.
    pub(crate) fn work_failed(&mut self, what: crate::diag::Work, error: &std::io::Error) {
        crate::diag::log(crate::diag::LogEvent::WorkNotStarted(
            what,
            crate::diag::ErrorKind::io(error),
        ));
        let first = self.work_error.is_none();
        self.work_error = Some(crate::locale::t("work_failed").to_string());
        self.stop_spinner_when_idle();
        if first {
            let refresh: Finish = Box::new(|app: &mut App| app.refresh_popup());
            launcher::emit(UserEvent::WorkDone(WorkDone(Arc::new(Mutex::new(Some(
                refresh,
            ))))));
        }
    }

    pub(crate) fn finish_work(&mut self, done: WorkDone) {
        let finish = done.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(finish) = finish {
            finish(self);
        }
    }
}

#[cfg(test)]
mod ai_spinner_tests {
    use super::*;

    #[test]
    fn a_slow_ai_run_does_not_start_the_cards_spinner() {
        assert!(!Lane::Ai.shows_card_spinner());
        for lane in [
            Lane::Card,
            Lane::TableWindow,
            Lane::Copy,
            Lane::File,
            Lane::Image,
        ] {
            assert!(lane.shows_card_spinner());
        }
        let mut lanes = Lanes::default();
        // What start_work records for a slow AI run: nothing, so the card shows no wheel.
        lanes.slow[Lane::Ai as usize] = Lane::Ai.shows_card_spinner().then(Instant::now);
        assert!(lanes.slow_since().is_none());
    }
}
