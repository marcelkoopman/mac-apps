//! Snippets ([`crate::snippet`]): naming a pinned text, and the fill-in panel that makes a new
//! copy of it. The text, its name and what is typed are never logged.

use super::*;

impl App {
    /// ⋯ › Save as Snippet… / Rename Snippet…: a name for the pinned text on screen.
    pub(crate) fn snippet_name_prompt(&mut self) {
        let index = self.history_cursor;
        let Some(id) = self.history.id_at(index) else {
            return;
        };
        if self.opened.is_some() || !self.history.is_pinned(index) || !self.history.is_text_id(id) {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let current =
                Zeroizing::new(self.history.snippet_name(index).unwrap_or("").to_string());
            let answer = mac_ui::dialog::prompt_text(
                mtm,
                crate::locale::t("snippet_name_title"),
                crate::locale::t("snippet_name_message"),
                &current,
            );
            launcher::order_front();
            if let Some(name) = answer.map(Zeroizing::new) {
                self.name_snippet(id, &name);
            }
        }
    }

    /// Name (or with a blank name, unname) pinned text entry `id`, and store the snippets. A
    /// name or text that holds a secret makes no snippet: it is never written, and an alert
    /// says so.
    pub(crate) fn name_snippet(&mut self, id: u64, name: &str) {
        let text = self.history.text_of(id).unwrap_or_default();
        if !name.trim().is_empty() && crate::snippet_store::holds_secret(name, &text) {
            #[cfg(target_os = "macos")]
            if let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() {
                mac_ui::dialog::alert(
                    mtm,
                    crate::locale::t("snippet_secret_title"),
                    crate::locale::t("snippet_secret"),
                );
                launcher::order_front();
            }
            return;
        }
        if !self.history.name_snippet(id, name) {
            return;
        }
        self.save_snippets();
        self.refresh_table_window();
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Fill In Snippet…: the panel asks for each `{{name}}` (none: straight on), the others fill
    /// in by themselves, and the result is a new copy through [`Self::new_value`], so the
    /// secret check applies as to any copy CopyCraft makes.
    pub(crate) fn snippet_fill(&mut self, id: u64) {
        let Some(index) = self.history.index_of(id) else {
            return;
        };
        let Some(title) = self.history.snippet_name(index).map(str::to_string) else {
            return;
        };
        let Some(text) = self.history.text_of(id) else {
            return;
        };
        let auto = crate::snippet::Automatic::at(
            crate::generate::Moment::now(),
            crate::generate::random_bytes(),
            &clipboard_text(),
            crate::locale::lang() == crate::locale::Lang::Nl,
        );
        let names = crate::snippet::fields(&text);
        if names.is_empty() {
            let value = Zeroizing::new(crate::snippet::fill(&text, &[], &auto));
            self.new_value(&title, value);
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let asks: Vec<mac_ui::dialog::FieldAsk<'_>> = names
                .iter()
                .map(|name| mac_ui::dialog::FieldAsk {
                    label: name,
                    lines: 1,
                })
                .collect();
            let ask = mac_ui::dialog::Fields {
                title: &title,
                message: crate::locale::t("snippet_fill_message"),
                ok: crate::locale::t("snippet_make"),
                fields: &asks,
                preview_label: crate::locale::t("snippet_preview"),
            };
            let (shown_text, shown_auto) = (text.clone(), auto.clone());
            let answer = mac_ui::dialog::fields(
                mtm,
                &ask,
                Box::new(move |typed| crate::snippet::fill(&shown_text, typed, &shown_auto)),
            );
            launcher::order_front();
            let Some(typed) = answer else {
                return;
            };
            let answers = Zeroizing::new(typed);
            let value = Zeroizing::new(crate::snippet::fill(&text, &answers, &auto));
            self.new_value(&title, value);
        }
    }
}

impl App {
    /// Write the snippets now in history to disk ([`crate::snippet_store`]): after naming,
    /// unpinning, removing, ⌘Z and Clear Pins. Not after a wipe: what is on disk comes back.
    pub(crate) fn save_snippets(&self) {
        let Some(dir) = crate::snippet_store::dir() else {
            return;
        };
        let snippets: Vec<crate::snippet_store::Stored> = self
            .history
            .snippets()
            .into_iter()
            .map(|(name, text)| crate::snippet_store::Stored { name, text })
            .collect();
        if let Err(e) = crate::snippet_store::save(&dir, &snippets) {
            crate::diag::log(crate::diag::LogEvent::SnippetsSaveFailed(
                crate::diag::ErrorKind::io(&e),
            ));
        }
    }

    /// The snippets on disk become pinned snippets in history again: at start, and after a
    /// wipe, lock or sleep emptied history (they are on disk anyway).
    pub(crate) fn load_snippets(&mut self) {
        let Some(dir) = crate::snippet_store::dir() else {
            return;
        };
        let stored = crate::snippet_store::load(&dir);
        let mut dropped = false;
        for snippet in &stored {
            dropped |= !self.history.restore_snippet(&snippet.name, &snippet.text);
        }
        crate::diag::log(crate::diag::LogEvent::SnippetsLoaded {
            any: !stored.is_empty(),
            dropped,
        });
    }
}

/// The clipboard's text for `{{klembord}}`: empty for a picture, a copy another app marked
/// private, or one not read yet.
fn clipboard_text() -> Zeroizing<String> {
    #[cfg(target_os = "macos")]
    if let crate::clipboard::ClipboardView::Text(text) = crate::macos_pasteboard::current_view() {
        return text;
    }
    Zeroizing::new(String::new())
}
