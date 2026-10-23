//! Apple Intelligence on the text of the card (Text ▾ › Apple Intelligence, [`mac_ai`]).
//!
//! One click runs one action, on a background thread, on the text the card shows (cut to
//! [`mac_ai::MAX_CHARS`]). The result is a preview over the card, not yet anywhere else: nothing
//! reaches the clipboard or history until "Apply to Clipboard", which makes it a new text
//! version like a Text ▾ step (⌘Z goes back). Cancel, Esc, the card closing, Wipe and a new
//! copy stop the run and drop the result. Text and result are never logged: a failure logs
//! only its kind ([`crate::diag::LogEvent::AiFailed`]).

use super::*;
use crate::notifications::Kind;
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
    /// Working, ready or failed: what the icon shows while the card is hidden.
    state: RunState,
    /// Hash of the text the run was started on (never the text): Apply only makes a version
    /// of the card when that text is still the card's.
    input_hash: u64,
    /// The text was cut to [`mac_ai::MAX_CHARS`]: the result covers only its start, so Apply
    /// never replaces the card's text with it.
    truncated: bool,
}

type AiResult = Result<Zeroizing<String>, mac_ai::Error>;

/// Where an AI run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    Working,
    /// The model gave an answer.
    Ready,
    /// The model failed or refused (guardrail, too long, language, unavailable, empty answer).
    Failed,
}

/// What the menu bar icon says about an AI run while the card is hidden.
pub(crate) type AiTray = RunState;

/// The icon's indicator: only while the card is hidden and a run exists (or has finished).
pub(crate) fn ai_tray_state(run: Option<RunState>, card_open: bool) -> Option<AiTray> {
    run.filter(|_| !card_open)
}

/// The state a finished run is in: an answer with text is ready, anything else failed.
pub(crate) fn finished_state(result: &AiResult) -> RunState {
    match result {
        Ok(text) if !text.trim().is_empty() => RunState::Ready,
        _ => RunState::Failed,
    }
}

/// Whether Apply may replace the card's text with the result: the run saw all of the text, the
/// card still shows the entry it started on, and the text is unchanged. Otherwise the result
/// only goes to the clipboard.
pub(crate) fn apply_makes_version_of(
    truncated: bool,
    entry: u64,
    input_hash: u64,
    now: Option<&(u64, Zeroizing<String>)>,
) -> bool {
    !truncated && apply_makes_version(entry, input_hash, now)
}

/// Whether switching to another history entry first hides the card like "Hide": a run or preview
/// is on the open card. The card is the run's then (it comes back as it was, from the icon), so
/// the result is never shown on, or applied to, the entry switched to.
pub(crate) fn history_switch_hides_ai(ai_active: bool, card_open: bool) -> bool {
    ai_active && card_open
}

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

    /// Hide: the card goes to the background, the run and preview stay; the icon shows a mark
    /// and, once the run is over, a tick (result) or a warning sign (failed or refused).
    pub(crate) fn ai_minimize(&mut self) {
        if self.ai.is_some() {
            launcher::minimize_card();
            self.sync_ai_tray();
        }
    }

    /// History navigation while the AI panel is open acts as "Hide" and goes no further: the
    /// run goes on in the background and the card, with its preview, stays tied to the entry it
    /// was started on. `true`: the navigation is handled.
    pub(crate) fn ai_hide_for_history(&mut self) -> bool {
        if !history_switch_hides_ai(self.ai.is_some(), launcher::is_open()) {
            return false;
        }
        self.ai_minimize();
        true
    }

    /// Mark the menu bar icon with the state of the run ([`ai_tray_state`]), or clear it.
    pub(crate) fn sync_ai_tray(&self) {
        #[cfg(target_os = "macos")]
        {
            let state = ai_tray_state(self.ai.as_ref().map(|run| run.state), launcher::is_open());
            let (title, tip) = match state {
                Some(RunState::Working) => (Some("…"), crate::locale::t("ai_tray_working")),
                Some(RunState::Ready) => (Some("✓"), crate::locale::t("ai_tray_ready")),
                Some(RunState::Failed) => (Some("⚠"), crate::locale::t("ai_tray_failed")),
                None => (None, "CopyCraft"),
            };
            self.tray.set_title(title);
            mac_ui::tray::set_accessibility_label(&self.tray, &tray_label(tip));
        }
    }

    /// An AI action was clicked: open the preview and start the model. `function` is the index
    /// of the one function of a code card to work on ([`commands::ai_function_text`]); `None`
    /// is the whole text.
    pub(crate) fn ai_start(&mut self, action: Action, function: Option<u8>) {
        self.ai_begin(action, function);
    }

    fn ai_begin(&mut self, action: Action, function: Option<u8>) {
        // The menu has the actions dimmed when the model is not there, and search can still
        // pick one: check again here.
        if mac_ai::availability().is_err() {
            self.notify(Kind::AiUnavailable, false, Duration::ZERO, None);
            return;
        }
        let Some((entry, text)) = self.ai_entry() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }
        // Only the chosen function goes to the model, with its signature.
        let part = match function {
            Some(index) => match commands::ai_function_text(&text, index) {
                Some(part) => Some(part),
                None => return,
            },
            None => None,
        };
        let work_on: &str = part.unwrap_or(&text);
        // Code beyond the model's window fails with a vague error: say so before the run.
        let too_long = matches!(action, Action::ExplainCode | Action::DocComment)
            && !commands::ai_code_fits(work_on);
        self.ai_stop();
        let (cut, cut_off) = mac_ai::truncate(work_on);
        // The result covers only a part (a function or the start of a long text): Apply never
        // replaces the card's text with it.
        let truncated = cut_off || part.is_some();
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
            state: RunState::Working,
            input_hash: hash_text(&text),
            truncated,
        });
        let note = if part.is_some() {
            Some(crate::locale::t("ai_part_note"))
        } else {
            cut_off.then(|| crate::locale::t("ai_truncated"))
        };
        launcher::ai_preview_working(commands::ai_title(action), note);
        if too_long {
            if let Some(run) = self.ai.as_mut() {
                run.state = RunState::Failed;
            }
            launcher::ai_preview_error(&commands::ai_code_too_long(part.is_none()));
            self.sync_ai_tray();
            return;
        }
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
        run.state = finished_state(&result);
        let action = run.action;
        let ready = run.state == RunState::Ready;
        let unavailable = matches!(result, Err(mac_ai::Error::Unavailable));
        match result {
            Ok(text) if !text.trim().is_empty() => {
                launcher::ai_preview_result(&text);
                run.result = Some(text);
            }
            Ok(_) => {
                crate::diag::log(crate::diag::LogEvent::AiFailed(mac_ai::Error::Failed));
                launcher::ai_preview_error(crate::locale::t("ai_err_failed"));
            }
            Err(mac_ai::Error::Cancelled) => self.ai_stop(),
            Err(error) => {
                crate::diag::log(crate::diag::LogEvent::AiFailed(error));
                launcher::ai_preview_error(commands::ai_error_text(error));
            }
        }
        self.sync_ai_tray();
        // Only the action's name goes into the notification, never the text or the result. A
        // failed run says nothing (except once that the model is not available).
        if ready {
            let name = commands::ai_title(action);
            self.notify(Kind::AiDone, false, Duration::ZERO, Some(name));
        } else if unavailable {
            self.notify(Kind::AiUnavailable, false, Duration::ZERO, None);
        }
    }

    /// "Apply to Clipboard": the result is a new copy, as a Text ▾ step's; ⌘Z shows the copy
    /// it was made from.
    pub(crate) fn ai_apply(&mut self) {
        let Some(run) = self.ai.take() else {
            return;
        };
        self.sync_ai_tray();
        launcher::ai_preview_close();
        let Some(result) = run.result else {
            run.job.cancel();
            return;
        };
        // A cut text is not overwritten (its end would be lost), and neither is a card that is
        // not the one the run was started for (a new copy came in meanwhile): the result only
        // goes to the clipboard.
        if !apply_makes_version_of(
            run.truncated,
            run.entry,
            run.input_hash,
            self.ai_entry().as_ref(),
        ) {
            self.copy_ai_result(&result);
            return;
        }
        self.text_stepped(
            run.entry,
            commands::ai_label(run.action).to_string(),
            &result,
        );
    }

    /// Cancel, Esc or the card closing: stop the model and drop the result. The preview is
    /// already closed when this comes from it.
    pub(crate) fn ai_stop(&mut self) {
        if let Some(run) = self.ai.take() {
            run.job.cancel();
        }
        self.sync_ai_tray();
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
    fn switching_history_hides_an_open_ai_panel_only() {
        assert!(history_switch_hides_ai(true, true));
        // No run, or the card already hidden: history works as before.
        assert!(!history_switch_hides_ai(false, true));
        assert!(!history_switch_hides_ai(true, false));
        assert!(!history_switch_hides_ai(false, false));
    }

    #[test]
    fn a_cut_text_is_never_replaced_by_its_result() {
        let text = Zeroizing::new("hello".to_string());
        let hash = hash_text(&text);
        let same = (7, text);
        assert!(apply_makes_version_of(false, 7, hash, Some(&same)));
        // The run saw only the start of the text: clipboard only, even for the same card.
        assert!(!apply_makes_version_of(true, 7, hash, Some(&same)));
        // The preview says so, in both languages.
        for lang in [crate::locale::Lang::Nl, crate::locale::Lang::En] {
            let notice = crate::locale::t_in(lang, "ai_truncated");
            assert!(notice.contains("klembord") || notice.contains("clipboard"));
        }
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

    #[test]
    fn the_icon_marks_a_hidden_run_and_its_outcome_only() {
        use RunState::*;
        // Card on screen, or no run: nothing on the icon.
        assert_eq!(ai_tray_state(Some(Working), true), None);
        assert_eq!(ai_tray_state(Some(Ready), true), None);
        assert_eq!(ai_tray_state(Some(Failed), true), None);
        assert_eq!(ai_tray_state(None, false), None);
        // Hidden: working, then a tick or a warning sign.
        assert_eq!(ai_tray_state(Some(Working), false), Some(Working));
        assert_eq!(ai_tray_state(Some(Ready), false), Some(Ready));
        assert_eq!(ai_tray_state(Some(Failed), false), Some(Failed));
    }

    #[test]
    fn only_an_answer_with_text_is_a_success() {
        let ok: AiResult = Ok(Zeroizing::new("done".into()));
        let empty: AiResult = Ok(Zeroizing::new("  \n".into()));
        assert_eq!(finished_state(&ok), RunState::Ready);
        assert_eq!(finished_state(&empty), RunState::Failed);
        for error in [
            mac_ai::Error::Guardrail,
            mac_ai::Error::TooLong,
            mac_ai::Error::Language,
            mac_ai::Error::Refusal,
            mac_ai::Error::Unavailable,
            mac_ai::Error::Failed,
        ] {
            assert_eq!(finished_state(&Err(error)), RunState::Failed);
        }
    }

    #[test]
    fn hide_is_a_card_action_that_keeps_the_run() {
        assert!(commands::keeps_card_open(&CommandId::AiMinimize));
        for lang in [crate::locale::Lang::Nl, crate::locale::Lang::En] {
            for key in [
                "ai_minimize",
                "ai_minimize_a11y",
                "ai_tray_working",
                "ai_tray_ready",
                "ai_tray_failed",
            ] {
                assert_ne!(crate::locale::t_in(lang, key), "?");
            }
        }
    }
}
