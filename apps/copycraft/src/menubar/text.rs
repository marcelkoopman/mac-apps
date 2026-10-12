//! Text… actions: count, first and last lines, reverse and column ([`crate::text_ops`]). Each
//! step copies its result, so every version is a history entry; ⌘Z puts the one before back.

use super::*;
use crate::text_ops::{self, Separator, TextOp};

/// The versions Text… made: the history entries they are, oldest first, and which is shown.
/// Only ids and labels: the texts stay in history (and go when it forgets them).
pub(crate) struct TextSteps {
    pub(crate) ids: Vec<u64>,
    pub(crate) labels: Vec<String>,
    pub(crate) cursor: usize,
}

impl App {
    /// The history entry on the card and its text, when Text… may work on it: a text card that
    /// follows history and is not a table (tables have their own steps).
    pub(crate) fn text_entry(&self) -> Option<(u64, Zeroizing<String>)> {
        if self.opened.is_some() {
            return None;
        }
        let text = Zeroizing::new(self.source_text()?);
        if self.table_source().is_some() {
            return None;
        }
        let index = self.history.text_index(&text, self.history_cursor)?;
        Some((self.history.id_at(index)?, text))
    }

    /// The steps whose shown version is entry `id`.
    fn text_steps_of(&self, id: u64) -> Option<&TextSteps> {
        self.text_steps
            .as_ref()
            .filter(|steps| steps.ids.get(steps.cursor) == Some(&id))
    }

    /// Undo, redo and the version capsule go to the text's versions.
    pub(crate) fn text_versions_shown(&self) -> bool {
        self.text_entry()
            .is_some_and(|(id, _)| self.text_steps_of(id).is_some_and(|s| s.ids.len() > 1))
    }

    /// Text… for the card: its versions and the count or note for the meta line.
    pub(crate) fn attach_text(&self, data: &mut LaunchData) {
        if data.subject_kind != SubjectKind::Text {
            return;
        }
        let Some((id, _)) = self.text_entry() else {
            return;
        };
        let (labels, version) = match self.text_steps_of(id) {
            Some(steps) => (steps.labels.clone(), steps.cursor),
            None => (vec![crate::locale::t("original").to_string()], 0),
        };
        let note = self
            .text_note
            .as_ref()
            .filter(|(entry, _)| *entry == id)
            .map(|(_, note)| note.clone());
        if let Some(note) = &note {
            data.notice = Some(match data.notice.take() {
                Some(notice) => format!("{notice}  ·  {note}"),
                None => note.clone(),
            });
        }
        data.text_edit = Some(commands::TextShown {
            labels,
            version,
            note,
        });
    }

    /// Count lines, words and characters onto the meta line. The clipboard stays.
    pub(crate) fn text_count(&mut self) {
        let Some((id, text)) = self.text_entry() else {
            return;
        };
        let counts = text_ops::count(&text);
        self.text_note = Some((
            id,
            crate::locale::tf(
                "text_count_result",
                &[&counts.lines, &counts.words, &counts.chars],
            ),
        ));
        self.refresh_popup();
    }

    /// Run `op` on the text on the card: copy the result (a new history entry) and make it the
    /// version after the one shown.
    pub(crate) fn text_step(&mut self, op: TextOp) {
        let Some((id, text)) = self.text_entry() else {
            return;
        };
        let result = op.apply(&text).map(Zeroizing::new);
        let Some(result) = result.filter(|result| result.as_str() != text.as_str()) else {
            self.text_note = Some((id, crate::locale::t("text_nothing").to_string()));
            self.refresh_popup();
            return;
        };
        if let Err(e) = self.write_own(&result) {
            eprintln!("text step failed: {e:#}");
            return;
        }
        self.record_own_copy(&result);
        let Some(new_id) = self.history.id_at(0) else {
            return;
        };
        let mut steps = match self.text_steps.take() {
            Some(steps) if steps.ids.get(steps.cursor) == Some(&id) => steps,
            _ => TextSteps {
                ids: vec![id],
                labels: vec![crate::locale::t("original").to_string()],
                cursor: 0,
            },
        };
        steps.ids.truncate(steps.cursor + 1);
        steps.labels.truncate(steps.cursor + 1);
        steps.ids.push(new_id);
        steps.labels.push(op.label());
        steps.cursor += 1;
        self.text_steps = Some(steps);
        self.text_note = None;
        self.card_view = CardView::Original;
        self.refresh_popup();
    }

    /// Show the version `to` picks from the one shown (undo, redo, the version capsule): put
    /// its history entry back on the clipboard.
    pub(crate) fn text_goto(&mut self, to: impl FnOnce(usize) -> Option<usize>) {
        let Some((id, _)) = self.text_entry() else {
            return;
        };
        let Some(steps) = self.text_steps_of(id) else {
            return;
        };
        let Some(index) = to(steps.cursor).filter(|&index| index < steps.ids.len()) else {
            return;
        };
        if index == steps.cursor {
            return;
        }
        // History may have let that version go (it keeps a limited number of copies).
        let Some(at) = self.history.index_of(steps.ids[index]) else {
            return;
        };
        if let Err(e) = self.restore_history(at) {
            eprintln!("text version failed: {e:#}");
            return;
        }
        // Put back as a new copy, the entry can have a new id.
        let new_id = self.history.id_at(0);
        if let Some(steps) = self.text_steps.as_mut() {
            steps.cursor = index;
            if let Some(new_id) = new_id {
                steps.ids[index] = new_id;
            }
        }
        self.text_note = None;
        self.refresh_popup();
    }

    /// First lines… (`last` false) or Last lines…: ask for N (10 to start), then take the step.
    pub(crate) fn text_lines_prompt(&mut self, last: bool) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let title = if last {
                crate::locale::t("text_tail_prompt")
            } else {
                crate::locale::t("text_head_prompt")
            };
            let mut message = crate::locale::t("text_lines_message").to_string();
            loop {
                let answer = mac_ui::dialog::prompt_text(mtm, title, &message, "10");
                launcher::order_front();
                let Some(answer) = answer.map(Zeroizing::new) else {
                    return;
                };
                if answer.trim().is_empty() {
                    return;
                }
                match text_ops::parse_count(&answer) {
                    Some(n) => {
                        self.text_step(if last {
                            TextOp::Tail(n)
                        } else {
                            TextOp::Head(n)
                        });
                        return;
                    }
                    None => message = crate::locale::t("text_count_invalid").to_string(),
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = last;
    }

    /// Column…: the separator (detected, can be changed), the column number (1 to start) and a
    /// live preview of the first lines; then take the step.
    pub(crate) fn text_column_prompt(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let Some((id, text)) = self.text_entry() else {
                return;
            };
            let detected = text_ops::detect_separator(&text);
            let titles: Vec<&str> = Separator::ALL.iter().map(|sep| sep.title()).collect();
            let selected = Separator::ALL
                .iter()
                .position(|sep| *sep == detected)
                .unwrap_or(0);
            let ask = mac_ui::dialog::NumberChoice {
                title: crate::locale::t("text_column_title"),
                message: crate::locale::t("text_column_message"),
                options: &titles,
                selected,
                option_label: crate::locale::t("text_separator"),
                number_label: crate::locale::t("text_column_number"),
                number_default: "1",
                preview_label: crate::locale::t("text_preview"),
            };
            let answer = mac_ui::dialog::choose_with_number(mtm, &ask, move |option, typed| {
                column_preview(&text, option, typed)
            });
            launcher::order_front();
            let Some((option, typed)) = answer else {
                return;
            };
            match (Separator::ALL.get(option), text_ops::parse_count(&typed)) {
                (Some(separator), Some(n)) => self.text_step(TextOp::Column(*separator, n)),
                _ => {
                    self.text_note = Some((id, crate::locale::t("text_count_invalid").to_string()));
                    self.refresh_popup();
                }
            }
        }
    }
}

/// The preview under Column…: the first lines of the column, or why there are none.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn column_preview(text: &str, option: usize, typed: &str) -> String {
    let (Some(separator), Some(n)) = (Separator::ALL.get(option), text_ops::parse_count(typed))
    else {
        return crate::locale::t("text_count_invalid").to_string();
    };
    let preview = text_ops::column_preview(text, *separator, n, 6);
    if preview.is_empty() {
        crate::locale::t("text_preview_empty").to_string()
    } else {
        preview
    }
}
