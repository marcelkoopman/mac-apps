//! Apple Intelligence on the text of the card (Lines… › Apple Intelligence, [`mac_ai`]).
//!
//! One click runs one action, on a background thread, on the text the card shows (cut to
//! [`mac_ai::MAX_CHARS`]). The result is a preview over the card, not yet anywhere else: nothing
//! reaches the clipboard or history until "Apply to clipboard", which makes it a new text
//! version like a Lines… step (⌘Z goes back). Cancel, Esc, the card closing, Wipe and a new
//! copy stop the run and drop the result. Text and result are never logged: a failure logs
//! only its kind ([`crate::diag::LogEvent::AiFailed`]).

use super::*;
use mac_ai::Action;

/// The run the preview belongs to.
pub(crate) struct AiRun {
    /// Tells this run's result from a cancelled or older one's.
    number: u64,
    /// The history entry (or file) the action was run on.
    entry: u64,
    action: Action,
    job: Arc<mac_ai::Job>,
    /// The model's answer, once it is there.
    result: Option<Zeroizing<String>>,
    /// Hash of the text the run was started on (never the text): Apply only makes a version
    /// of the card when that text is still the card's.
    input_hash: u64,
}

type AiResult = Result<Zeroizing<String>, mac_ai::Error>;

/// The startup warning was shown (once per session).
static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl App {
    /// The startup check is done: when Apple Intelligence is not usable, say so once per session
    /// and what to do about it. Nothing is shown when it works. Only the status is involved.
    pub(crate) fn ai_startup_checked(&mut self, availability: mac_ai::Availability) {
        if availability.is_ok() {
            return;
        }
        if WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.show_ai_health(availability);
    }

    /// ⋯ › Check AI: ask again, now, and show the result either way.
    pub(crate) fn ai_check_now(&mut self) {
        self.show_ai_health(mac_ai::availability());
    }

    fn show_ai_health(&self, availability: mac_ai::Availability) {
        #[cfg(target_os = "macos")]
        if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
            let (title, message) = commands::ai_health(availability);
            mac_ui::panel::activate_app(mtm);
            mac_ui::dialog::alert(mtm, title, &message);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = availability;
    }

    /// An AI action was clicked: open the preview and start the model.
    pub(crate) fn ai_start(&mut self, action: Action) {
        self.ai_begin(action);
    }

    fn ai_begin(&mut self, action: Action) {
        // The menu has the actions dimmed when the model is not there, and search can still
        // pick one: check again here.
        if mac_ai::availability().is_err() {
            return;
        }
        let Some((entry, text)) = self.ai_entry() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        self.ai_stop();
        let (cut, truncated) = mac_ai::truncate(&text);
        let input = Zeroizing::new(cut.to_string());
        self.ai_runs = self.ai_runs.wrapping_add(1);
        let number = self.ai_runs;
        let job = Arc::new(mac_ai::Job::new());
        self.ai = Some(AiRun {
            number,
            entry,
            action,
            job: Arc::clone(&job),
            result: None,
            input_hash: hash_text(&text),
        });
        launcher::ai_preview_working(
            commands::ai_title(action),
            truncated.then(|| crate::locale::t("ai_truncated")),
        );
        let done = self.start_work(
            work::Lane::Ai,
            Duration::ZERO,
            move || -> AiResult { job.run(action, &input).map(Zeroizing::new) },
            move |app, result| app.ai_finished(number, result),
        );
        if let Some(result) = done {
            self.ai_finished(number, result);
        } else if self.work_error.is_some() {
            // The thread did not start (already logged): say so on the preview.
            launcher::ai_preview_error(crate::locale::t("work_failed"));
        }
    }

    /// The model is done (or failed) for run `number`, on the main thread.
    fn ai_finished(&mut self, number: u64, result: AiResult) {
        let Some(run) = self.ai.as_mut().filter(|run| run.number == number) else {
            return;
        };
        match result {
            Ok(text) if !text.trim().is_empty() => {
                launcher::ai_preview_result(&text);
                run.result = Some(text);
            }
            Ok(_) => launcher::ai_preview_error(crate::locale::t("ai_err_failed")),
            Err(mac_ai::Error::Cancelled) => self.ai_stop(),
            Err(error) => {
                crate::diag::log(crate::diag::LogEvent::AiFailed(error));
                launcher::ai_preview_error(commands::ai_error_text(error));
            }
        }
    }

    /// "Apply to clipboard": the result becomes the next text version. Undoable.
    pub(crate) fn ai_apply(&mut self) {
        let Some(run) = self.ai.take() else {
            return;
        };
        launcher::ai_preview_close();
        let Some(result) = run.result else {
            run.job.cancel();
            return;
        };
        // A table is not overwritten, and neither is a card that is not the one the run was
        // started for (a new copy came in meanwhile): the result only goes to the clipboard.
        if !apply_makes_version(run.entry, run.input_hash, self.ai_entry().as_ref()) {
            self.copy_ai_result(&result);
            return;
        }
        self.text_stepped(
            run.entry,
            commands::ai_label(run.action).to_string(),
            true,
            &result,
        );
    }

    /// Cancel, Esc or the card closing: stop the model and drop the result. The preview is
    /// already closed when this comes from it.
    pub(crate) fn ai_stop(&mut self) {
        if let Some(run) = self.ai.take() {
            run.job.cancel();
        }
        launcher::ai_preview_close();
    }
}

/// Whether an applied result may become a version of the card: the card still shows the
/// entry the run started on (same id, same text).
pub(crate) fn apply_makes_version(
    entry: u64,
    input_hash: u64,
    now: Option<&(u64, Zeroizing<String>)>,
) -> bool {
    now.is_some_and(|(id, text)| *id == entry && hash_text(text) == input_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_only_becomes_a_version_of_the_card_it_was_started_for() {
        let text = Zeroizing::new("hello".to_string());
        let hash = hash_text(&text);
        let same = (7, text.clone());
        assert!(apply_makes_version(7, hash, Some(&same)));
        // Another entry, other text (a new copy) or no card text: copy only.
        assert!(!apply_makes_version(8, hash, Some(&same)));
        assert!(!apply_makes_version(
            7,
            hash,
            Some(&(7, Zeroizing::new("new".into())))
        ));
        assert!(!apply_makes_version(7, hash, None));
    }
    #[test]
    fn truncation_notice_counts_the_limit() {
        // The notice names the limit in digits: keep it in step with mac-ai.
        assert_eq!(mac_ai::MAX_CHARS, 20_000);
        for lang in [crate::locale::Lang::Nl, crate::locale::Lang::En] {
            let notice = crate::locale::t_in(lang, "ai_truncated");
            assert!(notice.contains("20.000") || notice.contains("20,000"));
        }
    }
}
