use super::*;

#[derive(Default, Clone)]
pub struct ClipboardHistory {
    entries: Vec<HistoryEntry>,
    next_id: u64,
    /// Asked before anything is added: `true` while recording is paused or the frontmost app is
    /// ignored ([`crate::capture::history_held`]). The one guard for every way into history
    /// (copies, own copies such as Text ▾ steps and filter results, pictures).
    gate: Option<fn() -> bool>,
    /// The entry the card's ✕ (or ⌫) took out last, and where it was, so ⌘Z can put it back.
    /// Only one; anything that adds or wipes entries drops it (zeroized with the entry).
    removed: Option<(usize, HistoryEntry)>,
}

impl ClipboardHistory {
    pub fn with_gate(gate: fn() -> bool) -> Self {
        Self {
            gate: Some(gate),
            ..Self::default()
        }
    }

    /// Nothing may be added now (see `gate`).
    pub fn held(&self) -> bool {
        self.gate.is_some_and(|gate| gate())
    }

    /// `text` as the newest copy. Already the newest: nothing changes (`text` is zeroized), so
    /// the entry keeps its cards. A pinned copy of the same text moves to the front and stays
    /// pinned. An unpinned one further down is replaced by a new entry in front.
    /// Returns whether `text` is in history now (`false`: blank, or history is held).
    pub fn record(&mut self, mut text: String) -> bool {
        if text.trim().is_empty() || self.held() || too_large(text.len(), MAX_TEXT_BYTES, false) {
            text.zeroize();
            return false;
        }
        if let Some(index) = self.text_pos(&text)
            && (index == 0 || self.entries[index].pinned)
        {
            if index != 0 {
                self.move_to_front(index);
            }
            text.zeroize();
            return true;
        }
        self.entries.retain(|existing| match &existing.body {
            HistoryBody::Text(existing) => existing.as_str() != text,
            HistoryBody::Image(_) => true,
        });
        self.removed = None;
        let id = self.alloc_id();
        self.entries.insert(0, HistoryEntry::text(id, text));
        self.trim_unpinned();
        true
    }

    fn text_pos(&self, text: &str) -> Option<usize> {
        self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Text(existing) => existing.as_str() == text,
            HistoryBody::Image(_) => false,
        })
    }

    /// Move entry `index` to the front without dropping it: dropping zeroizes the entry, and a
    /// picture's bytes are shared with the handles that point at it.
    pub(super) fn move_to_front(&mut self, index: usize) {
        if index > 0 {
            let mut entry = self.entries.remove(index);
            entry.copied = std::time::Instant::now();
            self.entries.insert(0, entry);
        }
    }

    /// [`record`](Self::record) `text`, and say where the entry that was at `cursor` is now:
    /// one further down, or at the front when it was `text` itself. `None` when nothing was
    /// recorded (blank text), so `cursor` still holds.
    pub fn record_tracking(&mut self, text: String, cursor: usize) -> Option<usize> {
        if text.trim().is_empty() || self.held() {
            let mut text = text;
            text.zeroize();
            return None;
        }
        let same = self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Text(existing) => existing.as_str() == text,
            HistoryBody::Image(_) => false,
        });
        if !self.record(text) {
            return None;
        }
        Some(self.tracked(same, cursor))
    }

    /// Like [`record_tracking`](Self::record_tracking), for an image: the entry shares `bytes`'
    /// allocation, as a copied image's does.
    pub fn record_image_tracking(&mut self, bytes: SecretBytes, cursor: usize) -> usize {
        if self.held() || too_large(bytes.byte_len(), MAX_IMAGE_BYTES, true) {
            return cursor;
        }
        let same = self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Image(existing) => existing == &bytes,
            HistoryBody::Text(_) => false,
        });
        self.insert_image(bytes);
        self.tracked(same, cursor)
    }

    /// Where the entry that was at `cursor` is after an entry was put in front, `same` being
    /// where an equal entry was before (it was taken out).
    fn tracked(&self, same: Option<usize>, cursor: usize) -> usize {
        let moved = match same {
            Some(index) if index == cursor => 0,
            Some(index) if index < cursor => cursor,
            _ => cursor + 1,
        };
        moved.min(self.entries.len().saturating_sub(1))
    }

    /// The picture as the newest copy, and the handle history keeps for it.
    #[cfg(test)]
    pub fn record_image(&mut self, bytes: Vec<u8>) -> Option<SecretBytes> {
        self.record_secret_image(SecretBytes::new(bytes)?)
    }

    /// [`record_image`](Self::record_image) for bytes already made into [`SecretBytes`] (and
    /// hashed) on a background thread. Too large or held: not stored, and zeroized.
    pub fn record_secret_image(&mut self, bytes: SecretBytes) -> Option<SecretBytes> {
        if self.held() || too_large(bytes.byte_len(), MAX_IMAGE_BYTES, true) {
            bytes.zeroize();
            return None;
        }
        Some(self.insert_image(bytes))
    }

    /// Bytes held by all entries together.
    pub fn total_bytes(&self) -> usize {
        self.entries.iter().map(|entry| entry.body.len()).sum()
    }

    /// Like [`record`](Self::record) for a picture. The entry that holds these very bytes (a
    /// history entry put back) moves to the front: dropping it would zeroize the bytes it shares
    /// with `bytes`. An equal picture in other bytes is replaced by `bytes`, as before.
    fn insert_image(&mut self, bytes: SecretBytes) -> SecretBytes {
        let shared = self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Image(existing) => existing.same_allocation(&bytes),
            HistoryBody::Text(_) => false,
        });
        if let Some(index) = shared {
            self.move_to_front(index);
            return bytes;
        }
        if let Some(index) = self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Image(existing) => existing == &bytes,
            HistoryBody::Text(_) => false,
        }) && self.entries[index].pinned
        {
            self.move_to_front(index);
            let HistoryBody::Image(existing) = &self.entries[0].body else {
                return bytes;
            };
            return existing.clone();
        }
        self.entries.retain(|existing| match &existing.body {
            HistoryBody::Image(existing) => existing != &bytes,
            HistoryBody::Text(_) => true,
        });
        self.removed = None;
        let id = self.alloc_id();
        self.entries
            .insert(0, HistoryEntry::image(id, bytes.clone()));
        self.trim_unpinned();
        bytes
    }

    /// Drop the oldest unpinned entry until at most [`MAX_HISTORY`] unpinned entries remain.
    /// Then, while history holds more than [`MAX_TOTAL_BYTES`], drop the oldest unpinned
    /// entry, never the newest one. A dropped entry is zeroized (`Drop`).
    fn trim_unpinned(&mut self) {
        while self.unpinned_count() > MAX_HISTORY {
            let Some(index) = self.entries.iter().rposition(|entry| !entry.pinned) else {
                break;
            };
            self.entries.remove(index);
        }
        while self.total_bytes() > MAX_TOTAL_BYTES {
            let Some(index) = self
                .entries
                .iter()
                .rposition(|entry| !entry.pinned)
                .filter(|index| *index > 0)
            else {
                break;
            };
            self.entries.remove(index);
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// The stable id of the entry at `index`.
    pub fn id_at(&self, index: usize) -> Option<u64> {
        self.entries.get(index).map(|entry| entry.id)
    }

    pub fn contains_id(&self, id: u64) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    /// The text of entry `id`, copied so a diff thread can keep it. `None` for a picture.
    pub fn text_of(&self, id: u64) -> Option<Zeroizing<String>> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| match &entry.body {
                HistoryBody::Text(text) => Some(Zeroizing::new(text.as_str().to_string())),
                HistoryBody::Image(_) => None,
            })
    }

    /// Entry `id` is text, so it can be added to a diff.
    pub fn is_text_id(&self, id: u64) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.id == id && matches!(entry.body, HistoryBody::Text(_)))
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        let entry = self.entries.get(index)?;
        match &entry.body {
            HistoryBody::Text(text) => Some(text.as_str()),
            HistoryBody::Image(_) => None,
        }
    }

    pub fn image(&self, index: usize) -> Option<SecretBytes> {
        let entry = self.entries.get(index)?;
        match &entry.body {
            HistoryBody::Image(bytes) => Some(bytes.clone()),
            HistoryBody::Text(_) => None,
        }
    }

    /// The type of entry `index`, for a label that shows no content.
    pub fn kind(&self, index: usize) -> Option<format::FormatKind> {
        Some(self.entries.get(index)?.kind)
    }

    /// When entry `index` was copied (or copied again).
    pub fn copied_at(&self, index: usize) -> Option<std::time::Instant> {
        Some(self.entries.get(index)?.copied)
    }

    pub fn mark(&self, index: usize) -> Option<&'static str> {
        Some(self.entries.get(index)?.kind.menu_symbol())
    }

    /// Where the text entry `text` is, looking at `hint` (the history cursor) first.
    pub fn text_index(&self, text: &str, hint: usize) -> Option<usize> {
        let is_text = |entry: &HistoryEntry| match &entry.body {
            HistoryBody::Text(existing) => existing.as_str() == text,
            HistoryBody::Image(_) => false,
        };
        if self.entries.get(hint).is_some_and(is_text) {
            return Some(hint);
        }
        self.entries.iter().position(is_text)
    }

    /// The card built earlier for the entry at `index` in `view` ([`remember_card`](Self::remember_card)).
    pub fn card(&self, index: usize, view: CardView) -> Option<&WorkCard> {
        self.entries
            .get(index)?
            .cards
            .iter()
            .find(|(shown, _)| *shown == view)
            .map(|(_, card)| card)
    }

    /// Drop cached preview cards. The next card is built in the current language.
    pub fn forget_cards(&mut self) {
        for entry in &mut self.entries {
            for (_, card) in &mut entry.cards {
                card.wipe();
            }
            entry.cards.clear();
        }
    }

    /// Keep `card` (the preview card of the text entry at `index` in `view`) with that entry.
    /// Image entries keep none: their card follows the scan, which arrives later.
    pub fn remember_card(&mut self, index: usize, view: CardView, mut card: WorkCard) {
        let Some(entry) = self.entries.get_mut(index) else {
            card.wipe();
            return;
        };
        // A diff is not a view of one entry, and it is not kept.
        if view == CardView::Diff || !matches!(entry.body, HistoryBody::Text(_)) {
            card.wipe();
            return;
        }
        if let Some(slot) = entry.cards.iter_mut().find(|(shown, _)| *shown == view) {
            slot.1.wipe();
            slot.1 = card;
        } else {
            entry.cards.push((view, card));
        }
        self.forget_derived_beyond(index);
    }

    /// The versions of the table in the text entry at `index`, if the card worked on it.
    pub fn table(&self, index: usize) -> Option<&TableVersions> {
        self.entries.get(index)?.table.as_ref()
    }

    /// The versions of the table in the text entry at `index`, made when needed. `None` for a
    /// picture. Entries further than [`DERIVED_NEAR`] from it forget their frames.
    pub fn table_mut(&mut self, index: usize) -> Option<&mut TableVersions> {
        if !matches!(self.entries.get(index)?.body, HistoryBody::Text(_)) {
            return None;
        }
        self.forget_derived_beyond(index);
        self.table_mut_in_place(index)
    }

    /// [`table_mut`](Self::table_mut) for the table window: the other entries keep what they
    /// have (the card's entry its frames), so the window and the card do not take turns
    /// working their tables out again.
    pub fn table_mut_in_place(&mut self, index: usize) -> Option<&mut TableVersions> {
        if !matches!(self.entries.get(index)?.body, HistoryBody::Text(_)) {
            return None;
        }
        Some(
            self.entries[index]
                .table
                .get_or_insert_with(crate::settings::preferred_table),
        )
    }

    /// The table whose newest job is `generation` ([`TableVersions::awaits`]).
    pub fn table_awaiting(&mut self, generation: u64) -> Option<&mut TableVersions> {
        self.entries
            .iter_mut()
            .filter_map(|entry| entry.table.as_mut())
            .find(|table| table.awaits(generation))
    }

    /// The id of the picture entry holding these very bytes.
    pub fn image_id(&self, bytes: &SecretBytes) -> Option<u64> {
        self.image_entry(bytes).map(|index| self.entries[index].id)
    }

    /// History holds the picture with these very bytes.
    pub fn holds_image(&self, bytes: &SecretBytes) -> bool {
        self.image_entry(bytes).is_some()
    }

    /// The scan kept with the picture entry holding these very bytes.
    pub fn image_scan(&self, bytes: &SecretBytes) -> Option<ImageScan> {
        self.image_entry(bytes)
            .and_then(|index| self.entries[index].scan.clone())
    }

    /// Keep `scan` with the picture entry holding these very bytes.
    pub fn remember_image_scan(&mut self, bytes: &SecretBytes, scan: &ImageScan) {
        let Some(index) = self.image_entry(bytes) else {
            return;
        };
        if let Some(old) = self.entries[index].scan.as_mut() {
            old.wipe();
        }
        self.entries[index].scan = Some(scan.clone());
        self.forget_derived_beyond(index);
    }

    fn image_entry(&self, bytes: &SecretBytes) -> Option<usize> {
        self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Image(existing) => existing.same_allocation(bytes),
            HistoryBody::Text(_) => false,
        })
    }

    /// Zeroize the cards and scans of entries more than [`DERIVED_NEAR`] away from `index`.
    fn forget_derived_beyond(&mut self, index: usize) {
        for (at, entry) in self.entries.iter_mut().enumerate() {
            if at.abs_diff(index) > DERIVED_NEAR {
                entry.forget_derived();
            }
        }
    }

    pub fn byte_len(&self, index: usize) -> Option<usize> {
        let entry = self.entries.get(index)?;
        match &entry.body {
            HistoryBody::Text(text) => Some(text.len()),
            HistoryBody::Image(bytes) => Some(bytes.with(<[u8]>::len)),
        }
    }

    /// The current clipboard item stays on the Current row. Older images stay listed.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_in_history(
        &self,
        index: usize,
        current_text: Option<&str>,
        current_image: Option<&SecretBytes>,
    ) -> bool {
        match self.entries.get(index).map(|entry| &entry.body) {
            Some(HistoryBody::Text(text)) => current_text != Some(text.as_str()),
            Some(HistoryBody::Image(bytes)) => {
                !current_image.is_some_and(|current| current.same_allocation(bytes))
            }
            None => false,
        }
    }

    pub fn labels(&self) -> Vec<(usize, String)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                // The format found when the entry was recorded (`one_line` would detect it again).
                (i, entry.kind.menu_symbol().to_string())
            })
            .collect()
    }

    pub fn is_pinned(&self, index: usize) -> bool {
        self.entries.get(index).is_some_and(|entry| entry.pinned)
    }

    pub fn pin_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.pinned).count()
    }

    fn unpinned_count(&self) -> usize {
        self.entries.len() - self.pin_count()
    }

    pub fn has_unpinned(&self) -> bool {
        self.entries.iter().any(|entry| !entry.pinned)
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == id)
    }

    /// Pin entry `id`. Already pinned: stays pinned. At [`PIN_MAX`] pins, a new one is refused.
    pub fn pin(&mut self, id: u64) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.entries[index].pinned {
            return true;
        }
        if self.pin_count() >= PIN_MAX {
            return false;
        }
        self.entries[index].pinned = true;
        true
    }

    /// Unpin entry `id`. Always works. If that pushes the unpinned list past the cap, the
    /// oldest unpinned entry is dropped. `false` when the entry is gone afterwards.
    pub fn unpin(&mut self, id: u64) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        self.entries[index].pinned = false;
        self.trim_unpinned();
        self.contains_id(id)
    }

    /// Take one entry out so [`restore_removed`](Self::restore_removed) can put it back at the
    /// same place. The entry taken out before is dropped (zeroized). The index it had.
    pub fn take_id(&mut self, id: u64) -> Option<usize> {
        let index = self.index_of(id)?;
        let entry = self.entries.remove(index);
        self.removed = Some((index, entry));
        Some(index)
    }

    /// Drop one entry for good (zeroized; not kept for ⌘Z): a copy that turned out not to
    /// belong in history. Whether it was there.
    pub fn discard_id(&mut self, id: u64) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        self.entries.remove(index);
        true
    }

    /// An entry taken out by [`take_id`](Self::take_id) can be put back.
    pub fn can_restore(&self) -> bool {
        self.removed.is_some()
    }

    /// Put the entry [`take_id`](Self::take_id) took out back where it was (or at the end when
    /// history got shorter). Its text is in history again by now: nothing is added. At
    /// [`PIN_MAX`] pins it comes back unpinned. Where it is now.
    pub fn restore_removed(&mut self) -> Option<usize> {
        let (index, mut entry) = self.removed.take()?;
        let duplicate = match &entry.body {
            HistoryBody::Text(text) => self.text_pos(text.as_str()).is_some(),
            HistoryBody::Image(bytes) => self.entries.iter().any(|other| match &other.body {
                HistoryBody::Image(existing) => existing == bytes,
                HistoryBody::Text(_) => false,
            }),
        };
        if duplicate {
            return None;
        }
        if entry.pinned && self.pin_count() >= PIN_MAX {
            entry.pinned = false;
        }
        let id = entry.id;
        let at = index.min(self.entries.len());
        self.entries.insert(at, entry);
        self.trim_unpinned();
        self.index_of(id)
    }

    /// Drop the entry kept for ⌘Z (zeroized).
    pub fn forget_removed(&mut self) {
        self.removed = None;
    }

    /// Drop every entry that is not pinned.
    pub fn clear_unpinned(&mut self) {
        self.removed = None;
        self.entries.retain(|entry| entry.pinned);
    }

    /// Drop every pin. Unpinned entries stay.
    pub fn clear_pins(&mut self) {
        self.removed = None;
        self.entries.retain(|entry| !entry.pinned);
    }

    /// The text search looks at: the start of the entry, capped at [`SEARCH_CAP`]. Empty for
    /// a picture.
    pub fn search_text(&self, index: usize) -> Zeroizing<String> {
        let Some(text) = self.get(index) else {
            return Zeroizing::new(String::new());
        };
        Zeroizing::new(prefix_chars(text, SEARCH_CAP))
    }

    pub fn clear(&mut self) {
        self.removed = None;
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `len` is over `limit`: log it (the size only) and say so.
fn too_large(len: usize, limit: usize, image: bool) -> bool {
    let over = len > limit;
    if over {
        crate::diag::log(crate::diag::LogEvent::HistoryItemTooLarge { image, bytes: len });
    }
    over
}
