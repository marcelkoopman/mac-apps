//! The checksums of a dropped or chosen file ([`crate::checksum`]): read on a thread of their
//! own from the path, in chunks, while the card shows the file; a large file shows its
//! progress. Only a yes or no about them is kept beyond the card, and nothing is logged.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

use super::*;
use crate::checksum::{Digests, Failed, Verdict};

/// A run reading one file.
pub(crate) struct ChecksumRun {
    generation: u64,
    cancel: Arc<AtomicBool>,
    total: u64,
    read: Arc<AtomicU64>,
    /// The drop it belongs to is on the card ([`App::show_dropped_file`]).
    shown: bool,
    result: Option<Result<Digests, Failed>>,
    verdict: Option<Verdict>,
}

impl Drop for ChecksumRun {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A promised file (from Photos, Mail, …) is removed once the card's read and the checksum
/// read are both done with it: when the last handle goes.
pub(crate) struct PromisedFile(PathBuf);

impl PromisedFile {
    pub(crate) fn new(path: PathBuf) -> Arc<Self> {
        Arc::new(Self(path))
    }
}

impl Drop for PromisedFile {
    fn drop(&mut self) {
        mac_ui::drop::discard_promised(&self.0);
    }
}

fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl ChecksumRun {
    /// Its checksums are in (or the read failed): nothing is reading the file any more.
    pub(crate) fn finished(&self) -> bool {
        self.result.is_some()
    }
}

impl App {
    /// Start reading the file at `path` for its checksums (the run before is cancelled). The
    /// file is the drop `generation` returned here; [`Self::show_dropped_file`] ties the two.
    /// `promised` is held until the read is done.
    pub(crate) fn start_checksum(
        &mut self,
        path: PathBuf,
        promised: Option<Arc<PromisedFile>>,
    ) -> u64 {
        self.checksum = None;
        let generation = next_generation();
        let total = std::fs::metadata(&path).map_or(0, |meta| meta.len());
        let cancel = Arc::new(AtomicBool::new(false));
        let read = Arc::new(AtomicU64::new(0));
        let (flag, counter) = (Arc::clone(&cancel), Arc::clone(&read));
        let spawned = std::thread::Builder::new()
            .name("copycraft-checksum".into())
            .spawn(move || {
                let mut shown = None;
                let result = crate::checksum::digest_file(&path, &flag, |done| {
                    counter.store(done, Ordering::Relaxed);
                    if total <= crate::checksum::SHOW_PROGRESS_ABOVE {
                        return;
                    }
                    let percent = crate::checksum::percent(done, total);
                    if shown != Some(percent) {
                        shown = Some(percent);
                        work::on_main(move |app| app.checksum_progressed(generation));
                    }
                });
                drop(promised);
                work::on_main(move |app| app.checksum_done(generation, result));
            });
        if let Err(e) = spawned {
            self.work_failed(crate::diag::Work::Lane, &e);
            return generation;
        }
        self.checksum = Some(ChecksumRun {
            generation,
            cancel,
            total,
            read,
            shown: false,
            result: None,
            verdict: None,
        });
        generation
    }

    /// A drop is going onto the card: its checksums are those of run `generation` (a file read
    /// from a path), or there are none (`None`: text, a picture, a made copy).
    pub(crate) fn tie_checksum(&mut self, generation: Option<u64>) {
        match (self.checksum.as_mut(), generation) {
            (Some(run), Some(generation)) if run.generation == generation => run.shown = true,
            _ => self.checksum = None,
        }
    }

    /// The file left the card: its checksums go with it (a run still reading stops).
    pub(crate) fn drop_checksum(&mut self) {
        self.checksum = None;
    }

    fn checksum_progressed(&mut self, generation: u64) {
        if self.shown_checksum(generation) && launcher::is_open() {
            self.refresh_popup();
        }
    }

    fn checksum_done(&mut self, generation: u64, result: Result<Digests, Failed>) {
        let Some(run) = self
            .checksum
            .as_mut()
            .filter(|run| run.generation == generation)
        else {
            return;
        };
        run.result = Some(result);
        if run.shown && launcher::is_open() {
            self.refresh_popup();
        }
    }

    fn shown_checksum(&self, generation: u64) -> bool {
        self.checksum
            .as_ref()
            .is_some_and(|run| run.generation == generation && run.shown)
    }

    /// *Check Hash from Clipboard*: a hex hash on the clipboard (case and spaces do not count)
    /// against the file's SHA-256 and MD5. The card shows ✓ or ✗ in its checksum view, and a
    /// toast says it too. The clipboard is read once, as for ⌘V; neither it nor the result is
    /// logged.
    pub(crate) fn compare_checksum(&mut self) {
        let Some(digests) = self
            .checksum
            .as_ref()
            .filter(|run| run.shown)
            .and_then(|run| run.result.clone())
            .and_then(Result::ok)
        else {
            super::state::save_feedback(crate::locale::t("checksum_not_ready"));
            return;
        };
        #[cfg(target_os = "macos")]
        let pasted = crate::macos_pasteboard::read_for_paste();
        #[cfg(not(target_os = "macos"))]
        let pasted = crate::history_mode::Pasted::Nothing;
        let text = match &pasted {
            crate::history_mode::Pasted::Text(text) => Some(text.as_str()),
            _ => None,
        };
        let verdict = crate::checksum::compare(text, &digests);
        drop(pasted);
        if let Some(run) = self.checksum.as_mut() {
            run.verdict = Some(verdict);
        }
        super::state::save_feedback(commands::checksum_verdict(verdict));
        self.card_view = CardView::Checksum;
        self.refresh_popup();
    }

    /// The checksums for the card, when it shows the dropped or chosen file they belong to.
    pub(crate) fn attach_checksum(&self, data: &mut LaunchData) {
        if self.opened.is_none() {
            return;
        }
        let Some(run) = self.checksum.as_ref().filter(|run| run.shown) else {
            return;
        };
        data.checksum = Some(commands::ChecksumShown {
            working: run.result.is_none().then(|| {
                (run.total > crate::checksum::SHOW_PROGRESS_ABOVE)
                    .then(|| crate::checksum::percent(run.read.load(Ordering::Relaxed), run.total))
            }),
            digests: run.result.clone().and_then(Result::ok),
            failed: matches!(run.result, Some(Err(Failed::Unreadable))),
            verdict: run.verdict,
        });
    }
}
