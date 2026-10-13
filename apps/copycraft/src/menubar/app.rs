//! The menu bar app state and its helper types.

use super::*;

/// Card title for text dropped on it (a dropped file shows its name).
pub(crate) fn dropped_text() -> &'static str {
    crate::locale::t("dropped_text")
}
/// Card name for image data dropped on it (a picture from Safari or Preview).
pub(crate) fn dropped_image() -> &'static str {
    crate::locale::t("dropped_image")
}

pub(crate) struct App {
    pub(crate) tray: TrayIcon,
    pub(crate) history: ClipboardHistory,
    /// Index into history while the arrows are browsing. 0 is the newest.
    pub(crate) history_cursor: usize,
    /// While a drop is on the card (at history index 0): the clipboard's own history index,
    /// for when the card follows the clipboard again.
    pub(crate) clipboard_cursor: Option<usize>,
    pub(crate) current_image: Option<SecretBytes>,
    pub(crate) recorded_image_change: Option<isize>,
    pub(crate) skip_image_change: Option<isize>,
    pub(crate) skip_record: Option<Zeroizing<String>>,
    /// Result the card is showing. A new clipboard copy returns this to original.
    pub(crate) card_view: CardView,
    /// File on the card. Absent means the card follows the clipboard.
    pub(crate) opened: Option<crate::open_file::OpenedFile>,
    pub(crate) image_scan: Option<commands::ImageScan>,
    /// Pasteboard change the stored scan belongs to.
    pub(crate) image_scan_change: Option<isize>,
    /// Pasteboard change a scan is already running for.
    pub(crate) image_scan_for: Option<isize>,
    pub(crate) signature: ClipSig,
    /// The save running on its thread. One at a time; Save is ignored meanwhile.
    pub(crate) saving: Option<Background>,
    /// "Show all" rendering on its thread. One at a time.
    pub(crate) loading_all: Option<Background>,
    /// The Preview button's picture is being written off the main thread: the spinner shows at
    /// once (the click must answer) until the window opens or the write fails.
    pub(crate) previewing: Option<Background>,
    /// The whole-text card "Show all" built, for the item and view it belongs to.
    pub(crate) full_card: Option<Box<launcher::FullCard>>,
    /// The card spinner is showing (some background work ran past [`SPINNER_DELAY`]).
    pub(crate) spinner_on: bool,
    /// The short blink of the menu bar icon after a copy, and its two template glyphs.
    pub(crate) blink: tray::Blink,
    /// The glyph and its blink frame, both templates, built once.
    pub(crate) icons: tray::Glyphs,
    pub(crate) hotkeys: GlobalHotKeyManager,
    /// The currently registered chord (for unregister on change).
    pub(crate) format_hotkey: global_hotkey::hotkey::HotKey,
    pub(crate) format_hotkey_id: u32,
    /// The sensitive copy copycraft wrote last, to clear when its minute is up.
    pub(crate) sensitive_clear: Option<SensitiveClear>,
    /// Change count of copycraft's own write whose labels were still being checked.
    pub(crate) conceal_when_checked: Option<isize>,
    /// The pasteboard change count the last poll looked at.
    pub(crate) polled_change: Option<isize>,
    /// The last poll found nothing or no text: look again even without a new change count.
    pub(crate) poll_again: bool,
    /// The card shows the one-time explanation of the pasteboard privacy alert, before the
    /// copy is read (Default or Ask; see [`crate::paste_access::ask_note`]).
    pub(crate) paste_explaining: bool,
    /// That explanation was shown once (stored in the user defaults).
    pub(crate) paste_explained: bool,
    /// The history picture the pasteboard holds at that change count (a copy recorded, or an
    /// entry put back), so its scan can be kept with the entry and found there again.
    pub(crate) image_on_pasteboard: Option<(isize, SecretBytes)>,
    /// When history last got a new entry (copy, drop, copycraft's own write).
    pub(crate) last_copy: Option<Instant>,
    /// [`crate::settings::Settings::history_minutes`], read once and kept in step with the menu.
    pub(crate) history_minutes: u32,
    /// Table versions for a file on the card whose text is not in history. A drop, Kies bestand
    /// and a copied file are recorded, and then use that history entry's table.
    pub(crate) opened_table: TableVersions,
    /// The text the card shows (the text Open table and the card's table steps work on).
    pub(crate) card_text: Option<Zeroizing<String>>,
    /// The history entry (id) a Lines… version step shows instead of the clipboard: going
    /// between versions only shows them; Copy puts one on the clipboard.
    pub(crate) browsing: Option<u64>,
    /// Table versions for card text that is in history nor a chosen file, and that text.
    pub(crate) loose_table: TableVersions,
    pub(crate) loose_text: Option<Zeroizing<String>>,
    /// The table job on its thread. One at a time: a new one cancels it.
    pub(crate) table_job: Option<TableRun>,
    /// Why the last table step failed, shown on the card until the next one.
    pub(crate) table_error: Option<String>,
    /// The entry the table window shows, while it is open.
    pub(crate) table_window: Option<TableWindow>,
    /// Picture versions for a file on the card whose bytes are not in history. A drop, Kies
    /// bestand and a copied picture are recorded, and then use that history entry's versions.
    pub(crate) opened_image: ImageVersions,
    /// The Image ▾ job on its thread. One at a time: a new one cancels it.
    pub(crate) image_job: Option<ImageRun>,
    /// Why the last Image ▾ step did nothing or failed, and on which picture (its allocation),
    /// shown on that picture's card until the next step.
    pub(crate) image_note: Option<(usize, String)>,
    /// The versions Lines… made of the text on the card ([`TextSteps`]).
    pub(crate) text_steps: Option<TextSteps>,
    /// The count, or why the last Lines… step did nothing, and for which history entry.
    pub(crate) text_note: Option<(u64, String)>,
    /// Bumped for every Lines… job; a result with another number is stale and dropped.
    pub(crate) text_generation: u64,
    /// Sequence numbers of the background jobs ([`work::Lane`]).
    pub(crate) lanes: work::Lanes,
    /// A worker thread failed to start: the short message on the card until work starts again.
    pub(crate) work_error: Option<String>,
    /// The last pin was refused because the pin list is full. Shown on the card meta line.
    pub(crate) pin_refused: bool,
    /// The Base64 picture the Decode chip draws, for the copy it was decoded from.
    pub(crate) decoded_picture: Option<DecodedPicture>,
    /// History entries chosen for a diff, oldest first. Not stored.
    pub(crate) diff_sel: crate::diff::Selection,
    /// Bumped when a diff is dropped, so a late result is ignored.
    pub(crate) diff_generation: u64,
    /// The last finished diff, while both picks are still chosen.
    pub(crate) diff_outcome: Option<crate::diff::Outcome>,
    /// The diff thread. One at a time.
    pub(crate) diffing: Option<Background>,
}

/// A Base64 picture on its way to the card ([`UserEvent::DecodedPicture`]): the hash of the
/// copied text it is of, and its bytes once the thread has decoded them (`None` while it runs,
/// and for a picture the card cannot or may not draw).
pub(crate) struct DecodedPicture {
    pub(crate) key: u64,
    pub(crate) bytes: Option<SecretBytes>,
}

/// An Image ▾ job running on its thread ([`crate::image_edit::Job`]).
pub(crate) struct ImageRun {
    pub(crate) started: Background,
    pub(crate) generation: u64,
    pub(crate) cancel: Arc<AtomicBool>,
}

/// A table job running on its thread ([`crate::table::Job`]).
pub(crate) struct TableRun {
    pub(crate) started: Background,
    pub(crate) generation: u64,
    pub(crate) cancel: Arc<AtomicBool>,
    /// Loading a frame the card does not show yet: no spinner.
    pub(crate) quiet: bool,
    /// Who asked for it: its error goes to that one's meta line.
    pub(crate) target: TableTarget,
}

/// Where a table command comes from and goes to: the card's table, or the table window's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableTarget {
    Card,
    Window,
}

/// The entry the table window shows: its copied text (found again in history by it, so the
/// window stays with its entry while the card moves on), or the chosen file on the card.
pub(crate) struct TableWindow {
    pub(crate) text: Zeroizing<String>,
    pub(crate) opened: bool,
    /// Opened while the card showed a chosen file. The file stays on the card while the window
    /// is open (the card closes for Open table), and the card shows it again when the window
    /// closes. Its table may live in history (a chosen file is recorded there too).
    pub(crate) keeps_file: bool,
    /// Why the window's last step failed, for its meta line.
    pub(crate) error: Option<String>,
    /// What the window was last given ([`TableWindow::key`]); the view is built again only
    /// when it changes, or while its labels are still being checked.
    pub(crate) shown: Option<u64>,
    pub(crate) checking: bool,
}

impl TableWindow {
    pub(crate) fn key(table: &TableVersions, error: Option<&str>, working: bool) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (table.frame_id(), table.frame().is_some(), table.cursor()).hash(&mut hasher);
        table.labels().hash(&mut hasher);
        (
            error,
            working,
            table.notes().map(|notes| notes.meta_notes()),
        )
            .hash(&mut hasher);
        hasher.finish()
    }
}

/// Where the table on the card keeps its versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableHome {
    History(usize),
    Opened,
    /// Clipboard text not in history (recording paused, an ignored app, a sensitive copy).
    Loose,
}

/// The history entry the card shows instead of the clipboard: the browsed version, unless a
/// chosen file is on the card or the version left history.
pub(crate) fn browsed_entry(browsed_index: Option<usize>, file_open: bool) -> Option<usize> {
    browsed_index.filter(|_| !file_open)
}

/// Where the table of the card's `text` lives: its history entry, the chosen file, or else the
/// loose slot, so Open table works for any table the card shows.
pub(crate) fn pick_table_home(history_index: Option<usize>, file_open: bool) -> TableHome {
    match history_index {
        Some(index) => TableHome::History(index),
        None if file_open => TableHome::Opened,
        None => TableHome::Loose,
    }
}

/// A pasteboard write to clear at `due`, if the pasteboard still holds it (`change`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SensitiveClear {
    pub(crate) change: isize,
    pub(crate) due: Instant,
}

/// Work running on a background thread: a save, or the "Show all" card.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Background {
    pub(crate) started: Instant,
}

impl Background {
    pub(crate) fn now() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub(crate) fn elapsed_ms(self) -> u128 {
        self.started.elapsed().as_millis()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipSig {
    pub(crate) text_hash: u64,
    pub(crate) image: bool,
    pub(crate) image_change: isize,
    pub(crate) history_len: usize,
}

impl ClipSig {
    /// Whether going from `self` to `next` is a new copy worth a blink: the pasteboard changed
    /// (text, image or change count), it is not the first look after launch or a clear (default
    /// signature), and it holds something. History edits alone do not count.
    pub(crate) fn is_new_copy(&self, next: &Self, empty: bool) -> bool {
        !empty
            && *self != Self::default()
            && (self.text_hash, self.image, self.image_change)
                != (next.text_hash, next.image, next.image_change)
    }
}

impl Default for ClipSig {
    fn default() -> Self {
        Self {
            text_hash: 0,
            image: false,
            image_change: -1,
            history_len: usize::MAX,
        }
    }
}
