//! Native dialogs (NSAlert via `mac_ui::dialog`) with the answers the menu handlers expect.
//! Without macOS, or off the main thread, every dialog counts as cancelled.

/// Text entered at `prompt`, trimmed. `None` on Cancel or an empty answer.
pub fn prompt_text(prompt: &str, default: &str) -> Option<String> {
    let text = native::prompt_text(prompt, default)?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Index into `options` picked at `prompt`, with the first option preselected. `None` on
/// Cancel.
pub fn choose(prompt: &str, options: &[&str]) -> Option<usize> {
    native::choose(prompt, options)
}

/// Show `message` under `title` with `buttons` (the first is the default). Index of the
/// clicked button.
pub fn buttons(title: &str, message: &str, buttons: &[&str]) -> Option<usize> {
    native::buttons(title, message, buttons)
}

#[cfg(target_os = "macos")]
mod native {
    use mac_ui::dialog;
    use mac_ui::objc2::MainThreadMarker;

    pub fn prompt_text(prompt: &str, default: &str) -> Option<String> {
        dialog::prompt_text(MainThreadMarker::new()?, prompt, "", default)
    }

    pub fn choose(prompt: &str, options: &[&str]) -> Option<usize> {
        dialog::choose(MainThreadMarker::new()?, prompt, "", options, 0)
    }

    pub fn buttons(title: &str, message: &str, buttons: &[&str]) -> Option<usize> {
        dialog::buttons(MainThreadMarker::new()?, title, message, buttons)
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
}
