//! The file a copy was made from. *Copy as JSON* or *CSV*, *Copy cleaned link* or *domain* on a
//! dropped, chosen or pasted file, and a copy from its table window, put the new copy on the
//! card ([`App::follow_made_copy`]); the file, its name and its table versions are kept here,
//! in memory only, so *Origineel* and ⌘Z put it back on the card
//! ([`CommandId::BackToSource`]). Also a file history does not have (larger than
//! [`crate::clipboard::MAX_TEXT_BYTES`], holding a secret, held by Pause or an ignored app, or
//! still being checked). Never written to disk, never added to history, never logged; wiped
//! when it is let go ([`App::release_made_source`]): when another file comes onto the card, the
//! card goes to another history entry than that copy, another app copies something, a new
//! copy is made from another file, or on Wipe, Clear History and the lock screen.

use super::*;

/// The kept file and what the card had with it.
pub(crate) struct MadeSource {
    /// The file (its text is zeroized when this goes).
    opened: crate::open_file::OpenedFile,
    /// Its table versions, when its table lived with the file (not in history).
    table: TableVersions,
    /// The view the card had on it.
    view: CardView,
    /// The copy made from it ([`hash_text`]): browsing to that entry keeps the file
    /// ([`browse_keeps_source`]).
    copy: u64,
}

/// Browsing to a history entry whose text hashes to `entry` (`None`: a picture) keeps the file
/// a copy (`copy`) was made from only when that entry is the copy itself; any other entry lets
/// the file go.
pub(crate) fn browse_keeps_source(entry: Option<u64>, copy: u64) -> bool {
    entry == Some(copy)
}

/// *Origineel* and ⌘Z are offered while a file is kept (`kept`) and no file is on the card. The
/// card can only have moved on from the copy without letting the file go by staying with it:
/// Text ▾ steps on the copy, its table versions, Pause showing the entry before it, the copy
/// cleared from the clipboard.
pub(crate) fn offers_back_to_source(kept: bool, file_on_card: bool) -> bool {
    kept && !file_on_card
}

impl App {
    /// A copy (`made`, its [`hash_text`]) was made from the file on the card, which now leaves
    /// it: keep the file and its versions for *Origineel* and ⌘Z. One file
    /// is kept: one kept before is wiped. Nothing when no copy was written or no file is shown.
    /// A table window on the file must be closed first (it unpins the file's table).
    pub(crate) fn keep_made_source(&mut self, made: Option<u64>) {
        let Some(copy) = made else {
            return;
        };
        let Some(opened) = self.opened.take() else {
            return;
        };
        let table = std::mem::replace(&mut self.opened_table, crate::settings::preferred_table());
        self.release_made_source();
        self.made_source = Some(MadeSource {
            opened,
            table,
            view: self.card_view,
            copy,
        });
    }

    /// *Origineel* or ⌘Z on the copy: the kept file back on the card, as it was dropped, with
    /// its versions and view. Where history still has its text, the strip marks that entry,
    /// and the card goes back to the clipboard's copy when it leaves the file.
    pub(crate) fn back_to_source(&mut self) {
        let Some(source) = self.made_source.take() else {
            return;
        };
        let MadeSource {
            opened,
            table,
            view,
            copy: _,
        } = source;
        if self
            .table_window
            .as_ref()
            .is_some_and(|window| window.opened)
        {
            self.close_table_window();
        }
        let clipboard = self.clipboard_cursor.unwrap_or(self.history_cursor);
        if let Some(at) = opened
            .text
            .as_ref()
            .and_then(|text| self.history.text_index(text.as_str(), self.history_cursor))
        {
            self.history_cursor = at;
            self.clipboard_cursor = Some(clipboard);
        }
        self.opened = Some(opened);
        self.opened_superseded = false;
        self.opened_table = table;
        self.browsing = None;
        self.card_view = view;
        self.refresh_popup();
    }

    /// Let the kept file go: its text is zeroized and its table dropped off the main thread,
    /// as the table window's are.
    pub(crate) fn release_made_source(&mut self) {
        if let Some(source) = self.made_source.take() {
            crate::work::wipe_later(source);
        }
    }

    /// The card about to be shown: *Origineel* and ⌘Z while a file is kept
    /// ([`offers_back_to_source`]).
    pub(crate) fn attach_made_source(&self, data: &mut LaunchData) {
        data.back_to_source =
            offers_back_to_source(self.made_source.is_some(), self.opened.is_some());
    }

    /// The card goes to history entry `index` (a strip card, the arrows, an entry chosen from
    /// the menu): the kept file is let go unless that entry is the copy made from it.
    pub(crate) fn browse_releases_source(&mut self, index: usize) {
        let Some(copy) = self.made_source.as_ref().map(|source| source.copy) else {
            return;
        };
        if !browse_keeps_source(self.history.get(index).map(hash_text), copy) {
            self.release_made_source();
        }
    }
}
