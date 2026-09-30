//! Native dialogs (NSAlert via `mac_ui::dialog`) with the answers the menu handlers expect.
//! Without macOS, or off the main thread, every dialog counts as cancelled. Every dialog is
//! logged to `~/.ticker_debug.log` ("dialog …" lines), including the reason when it is skipped.

use crate::log_message;

/// Text entered at `prompt`, trimmed. `None` on Cancel or an empty answer.
pub fn prompt_text(prompt: &str, default: &str) -> Option<String> {
    log_message(&format!("dialog prompt_text: open {prompt:?}"));
    let answer = native::prompt_text(prompt, default);
    log_message(&format!(
        "dialog prompt_text: closed {prompt:?} -> {}",
        if answer.is_some() {
            "OK"
        } else {
            "cancel/skip"
        }
    ));
    let text = answer?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Index into `options` picked at `prompt`, with the first option preselected. `None` on
/// Cancel.
pub fn choose(prompt: &str, options: &[&str]) -> Option<usize> {
    log_message(&format!(
        "dialog choose: open {prompt:?} ({} options)",
        options.len()
    ));
    let answer = native::choose(prompt, options);
    log_message(&format!("dialog choose: closed {prompt:?} -> {answer:?}"));
    answer
}

/// Show `message` under `title` with `buttons` (the first is the default). Index of the
/// clicked button.
pub fn buttons(title: &str, message: &str, buttons: &[&str]) -> Option<usize> {
    log_message(&format!("dialog buttons: open {title:?}"));
    let answer = native::buttons(title, message, buttons);
    log_message(&format!("dialog buttons: closed {title:?} -> {answer:?}"));
    answer
}

/// Whether a dialog can open right now (main thread, and no menu still tracking). Logs the
/// run loop mode when it cannot.
pub fn can_run_modal() -> bool {
    native::can_run_modal()
}

#[cfg(target_os = "macos")]
mod native {
    use crate::log_message;
    use mac_ui::dialog;
    use mac_ui::objc2::MainThreadMarker;

    /// The marker, or a log line saying why the dialog is skipped.
    fn main_thread(what: &str) -> Option<MainThreadMarker> {
        let mtm = MainThreadMarker::new();
        if mtm.is_none() {
            log_message(&format!(
                "dialog {what}: SKIPPED, not on the main thread ({:?})",
                std::thread::current().name()
            ));
        }
        mtm
    }

    fn log_state(mtm: MainThreadMarker, what: &str) {
        log_message(&format!(
            "dialog {what}: run loop mode {:?}, app active before: {}",
            dialog::run_loop_mode(mtm),
            dialog::app_is_active(mtm)
        ));
    }

    pub fn prompt_text(prompt: &str, default: &str) -> Option<String> {
        let mtm = main_thread("prompt_text")?;
        log_state(mtm, "prompt_text");
        dialog::prompt_text(mtm, prompt, "", default)
    }

    pub fn choose(prompt: &str, options: &[&str]) -> Option<usize> {
        let mtm = main_thread("choose")?;
        if options.is_empty() {
            log_message("dialog choose: SKIPPED, no options");
            return None;
        }
        log_state(mtm, "choose");
        dialog::choose(mtm, prompt, "", options, 0)
    }

    pub fn buttons(title: &str, message: &str, buttons: &[&str]) -> Option<usize> {
        let mtm = main_thread("buttons")?;
        log_state(mtm, "buttons");
        dialog::buttons(mtm, title, message, buttons)
    }

    pub fn can_run_modal() -> bool {
        let Some(mtm) = main_thread("check") else {
            return false;
        };
        let ok = dialog::can_run_modal(mtm);
        if !ok {
            log_message(&format!(
                "dialog: deferred, run loop mode {:?} (menu still tracking?)",
                dialog::run_loop_mode(mtm)
            ));
        }
        ok
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    pub fn prompt_text(_prompt: &str, _default: &str) -> Option<String> {
        None
    }

    pub fn choose(_prompt: &str, _options: &[&str]) -> Option<usize> {
        None
    }

    pub fn buttons(_title: &str, _message: &str, _buttons: &[&str]) -> Option<usize> {
        None
    }

    pub fn can_run_modal() -> bool {
        true
    }
}
