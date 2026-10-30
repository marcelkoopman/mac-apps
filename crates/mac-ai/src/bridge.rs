//! The Swift bridge (`swift/bridge.swift`), the only `unsafe` in the AI feature.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::{self, NonNull};

use crate::{Action, Availability, Error, FactTask, Language, decode_availability};

unsafe extern "C" {
    fn mac_ai_availability() -> i32;
    fn mac_ai_job_new() -> *mut c_void;
    fn mac_ai_job_cancel(job: *mut c_void);
    fn mac_ai_job_free(job: *mut c_void);
    fn mac_ai_job_run(
        job: *mut c_void,
        instruction: *const u8,
        instruction_len: usize,
        input: *const u8,
        len: usize,
        out: *mut *mut c_char,
    ) -> i32;
    fn mac_ai_string_free(text: *mut c_char);
}

pub(crate) fn availability() -> Availability {
    // SAFETY: takes no arguments and only reads the system's model status.
    decode_availability(unsafe { mac_ai_availability() })
}

/// A run that can be cancelled from another thread: share it in an `Arc`, call [`Job::run`] on a
/// worker thread and [`Job::cancel`] from anywhere.
#[derive(Debug)]
pub struct Job(NonNull<c_void>);

// SAFETY: the Swift `Job` behind the pointer guards its state with a lock; `cancel` and `run` may
// be called from different threads, and it is freed only by `Drop`, once nobody holds the `Job`.
unsafe impl Send for Job {}
// SAFETY: see `Send`.
unsafe impl Sync for Job {}

impl Job {
    pub fn new() -> Self {
        // SAFETY: returns a retained Swift object, released in `Drop`; never null.
        let raw = unsafe { mac_ai_job_new() };
        Job(NonNull::new(raw).expect("mac_ai_job_new returns an object"))
    }

    /// Stop the run: [`Job::run`] then returns [`Error::Cancelled`]. Also when it has not
    /// started yet.
    pub fn cancel(&self) {
        // SAFETY: the pointer is a live Swift `Job` for as long as `self` exists.
        unsafe { mac_ai_job_cancel(self.0.as_ptr()) }
    }

    /// Run `action` on `text` and wait for the answer. Blocks: call it from a background thread.
    pub fn run(&self, action: Action, text: &str) -> Result<String, Error> {
        // Only an [`Action`]'s own task goes in: there is no way to pass a free instruction.
        self.run_raw(&crate::instructions(action.task()), &crate::prompt(text))
    }

    /// Run a [`FactTask`] on `facts` (numbers the app worked out) and wait for the answer, in
    /// `language`. Blocks: call it from a background thread.
    pub fn run_facts(
        &self,
        task: FactTask,
        language: Language,
        facts: &str,
    ) -> Result<String, Error> {
        self.run_raw(
            &crate::fact_instructions(task, language),
            &crate::prompt(facts),
        )
    }

    fn run_raw(&self, instructions: &str, input: &str) -> Result<String, Error> {
        let mut out: *mut c_char = ptr::null_mut();
        // SAFETY: `self.0` is a live Swift `Job`; both strings are valid for their lengths; `out`
        // points to a writable pointer. On success Swift leaves a malloc'ed C string in it.
        let status = unsafe {
            mac_ai_job_run(
                self.0.as_ptr(),
                instructions.as_ptr(),
                instructions.len(),
                input.as_ptr(),
                input.len(),
                &mut out,
            )
        };
        if status != 0 {
            return Err(Error::from_code(status));
        }
        if out.is_null() {
            return Err(Error::Failed);
        }
        // SAFETY: on status 0 `out` is a NUL-terminated string made by Swift's `strdup`; it is
        // copied here, then freed with the matching `mac_ai_string_free`.
        let result = unsafe {
            let copy = CStr::from_ptr(out).to_string_lossy().into_owned();
            mac_ai_string_free(out);
            copy
        };
        Ok(result)
    }
}

impl Default for Job {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: releases the reference `mac_ai_job_new` returned; `self` is not used afterwards.
        unsafe { mac_ai_job_free(self.0.as_ptr()) }
    }
}
