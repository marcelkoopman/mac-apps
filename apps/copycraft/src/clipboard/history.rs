use super::*;

#[derive(Default, Clone)]
pub struct ClipboardHistory {
    entries: Vec<HistoryEntry>,
    next_id: u64,
}

impl ClipboardHistory {
    /// `text` as the newest copy. Already the newest: nothing changes (`text` is zeroized), so
    /// the entry keeps its cards. A pinned copy of the same text moves to the front and stays
    /// pinned. An unpinned one further down is replaced by a new entry in front.
    pub fn record(&mut self, mut text: String) {
        if text.trim().is_empty() {
            text.zeroize();
            return;
        }
        if let Some(index) = self.text_pos(&text)
            && (index == 0 || self.entries[index].pinned)
        {
            if index != 0 {
                self.move_to_front(index);
            }
            text.zeroize();
            return;
        }
        self.entries.retain(|existing| match &existing.body {
            HistoryBody::Text(existing) => existing.as_str() != text,
            HistoryBody::Image(_) => true,
        });
        let id = self.alloc_id();
        self.entries.insert(0, HistoryEntry::text(id, text));
        self.trim_unpinned();
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
            let entry = self.entries.remove(index);
            self.entries.insert(0, entry);
        }
    }

    /// [`record`](Self::record) `text`, and say where the entry that was at `cursor` is now:
    /// one further down, or at the front when it was `text` itself. `None` when nothing was
    /// recorded (blank text), so `cursor` still holds.
    pub fn record_tracking(&mut self, text: String, cursor: usize) -> Option<usize> {
        if text.trim().is_empty() {
            let mut text = text;
            text.zeroize();
            return None;
        }
        let same = self.entries.iter().position(|entry| match &entry.body {
            HistoryBody::Text(existing) => existing.as_str() == text,
            HistoryBody::Image(_) => false,
        });
        self.record(text);
        Some(self.tracked(same, cursor))
    }

    /// Like [`record_tracking`](Self::record_tracking), for an image: the entry shares `bytes`'
    /// allocation, as a copied image's does.
    pub fn record_image_tracking(&mut self, bytes: SecretBytes, cursor: usize) -> usize {
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
    pub fn record_image(&mut self, bytes: Vec<u8>) -> Option<SecretBytes> {
        let bytes = SecretBytes::new(bytes)?;
        Some(self.insert_image(bytes))
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
        let id = self.alloc_id();
        self.entries
            .insert(0, HistoryEntry::image(id, bytes.clone()));
        self.trim_unpinned();
        bytes
    }

    /// Drop the oldest unpinned entry until at most [`MAX_HISTORY`] unpinned entries remain.
    fn trim_unpinned(&mut self) {
        while self.unpinned_count() > MAX_HISTORY {
            let Some(index) = self.entries.iter().rposition(|entry| !entry.pinned) else {
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

    /// The versions of the picture entry holding these very bytes, made when needed. Entries
    /// further than [`DERIVED_NEAR`] from it forget their derived data.
    pub fn image_versions_mut(&mut self, bytes: &SecretBytes) -> Option<&mut ImageVersions> {
        let index = self.image_entry(bytes)?;
        self.forget_derived_beyond(index);
        Some(
            self.entries[index]
                .images
                .get_or_insert_with(ImageVersions::default),
        )
    }

    /// History holds the picture with these very bytes.
    pub fn holds_image(&self, bytes: &SecretBytes) -> bool {
        self.image_entry(bytes).is_some()
    }

    /// Keep `scan` with the picture version `picture` ([`ImageVersions::remember_scan`]).
    pub fn remember_version_scan(&mut self, picture: &SecretBytes, scan: &ImageScan) -> bool {
        self.entries
            .iter_mut()
            .filter_map(|entry| entry.images.as_mut())
            .any(|images| images.remember_scan(picture, scan))
    }

    /// The picture whose newest job is `generation` ([`ImageVersions::awaits`]).
    pub fn image_awaiting(&mut self, generation: u64) -> Option<&mut ImageVersions> {
        self.entries
            .iter_mut()
            .filter_map(|entry| entry.images.as_mut())
            .find(|images| images.awaits(generation))
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

    /// Drop one entry. The body's [`Drop`] zeroizes it. The index it had, when it was there.
    pub fn remove_id(&mut self, id: u64) -> Option<usize> {
        let index = self.index_of(id)?;
        self.entries.remove(index);
        Some(index)
    }

    /// Drop every entry that is not pinned.
    pub fn clear_unpinned(&mut self) {
        self.entries.retain(|entry| entry.pinned);
    }

    /// Drop every pin. Unpinned entries stay.
    pub fn clear_pins(&mut self) {
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
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
