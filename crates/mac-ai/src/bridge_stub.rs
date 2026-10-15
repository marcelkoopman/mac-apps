//! No Swift bridge in this build: nothing can run.

use crate::{Action, Availability, Error, Unavailable};

pub(crate) fn availability() -> Availability {
    Err(Unavailable::NoBridge)
}

/// A run that can be cancelled. Here it never runs anything.
#[derive(Debug, Default)]
pub struct Job;

impl Job {
    pub fn new() -> Self {
        Job
    }

    pub fn cancel(&self) {}

    pub fn run(&self, _action: Action, _text: &str) -> Result<String, Error> {
        Err(Error::Unavailable)
    }

    pub fn run_task(&self, _instruction: &str, _text: &str) -> Result<String, Error> {
        Err(Error::Unavailable)
    }
}
