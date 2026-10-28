//! Two text files dropped at once: compared in the diff view, as two history copies are. Their
//! texts are held in memory for that diff only (zeroized when it goes: Wipe, lock, another
//! compare), never recorded in history, and labelled by the same check as any diff side, so a
//! token or key in either keeps the diff covered. Nothing about them is logged.

use std::path::PathBuf;

use super::*;

/// The diff picks that stand for the two dropped files: never a history entry's id.
pub(crate) const DROPPED_A: u64 = u64::MAX - 1;
pub(crate) const DROPPED_B: u64 = u64::MAX;

/// The texts of the two dropped files: A (the first in the drag) and B.
pub(crate) struct DroppedPair {
    a: Zeroizing<String>,
    b: Zeroizing<String>,
}

/// Whether diff pick `id` is one of the dropped files.
pub(crate) fn is_dropped_side(id: u64) -> bool {
    matches!(id, DROPPED_A | DROPPED_B)
}

/// A dropped file's text, when it is a text file: not a picture, a PDF, a Word document (which
/// shows a page) or a file too large to show.
pub(crate) fn compared_text(mut opened: crate::open_file::OpenedFile) -> Option<Zeroizing<String>> {
    if opened.image.is_some() || opened.pdf.is_some() {
        return None;
    }
    opened.text.take()
}

impl App {
    /// Several files dropped at once. Two text files open the diff, A the first; anything else
    /// (more than two, or a file that is not text) gets the toast. The files are read on a
    /// thread, as one dropped file is.
    pub(crate) fn compare_dropped_files(&mut self, paths: Vec<PathBuf>) {
        let Ok([a, b]) = <[PathBuf; 2]>::try_from(paths) else {
            super::state::save_feedback(crate::locale::t("compare_two_text_files"));
            return;
        };
        let read = self.start_work(
            work::Lane::File,
            work::QUICK,
            move || {
                let a = compared_text(crate::open_file::load_dropped(&a))?;
                let b = compared_text(crate::open_file::load_dropped(&b))?;
                Some((a, b))
            },
            |app, pair| app.dropped_pair_read(pair),
        );
        if let Some(pair) = read {
            self.dropped_pair_read(pair);
        }
    }

    fn dropped_pair_read(&mut self, pair: Option<(Zeroizing<String>, Zeroizing<String>)>) {
        let Some((a, b)) = pair else {
            super::state::save_feedback(crate::locale::t("compare_two_text_files"));
            return;
        };
        self.dropped_pair = Some(DroppedPair { a, b });
        self.diff_sel.set_pair(DROPPED_A, DROPPED_B);
        self.invalidate_diff();
        self.card_view = CardView::Diff;
        if !self.kick_diff() {
            self.reset_view();
        }
        self.refresh_popup();
    }

    /// The text of diff side `id`: a dropped file's, or a history copy's.
    pub(crate) fn diff_side_text(&self, id: u64) -> Option<Zeroizing<String>> {
        let pair = self.dropped_pair.as_ref();
        match id {
            DROPPED_A => pair.map(|pair| pair.a.clone()),
            DROPPED_B => pair.map(|pair| pair.b.clone()),
            _ => self.history.text_of(id),
        }
    }

    /// Before history copies are picked for a diff: the dropped files' diff goes (their texts
    /// are zeroized). `true` when there was one.
    pub(crate) fn forget_dropped_pair(&mut self) -> bool {
        if self.dropped_pair.take().is_none() {
            return false;
        }
        self.diff_sel.clear();
        self.invalidate_diff();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder for this test's made-up files, removed at the end.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let folder = std::env::temp_dir().join(format!(
                "copycraft-dropped-diff-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).unwrap();
            Self(folder)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn only_text_files_are_compared() {
        let scratch = Scratch::new("kinds");
        let text = scratch.file("a.txt", b"alpha\nbeta\n");
        let code = scratch.file("b.rs", b"fn main() {}\n");
        let binary = scratch.file("c.bin", &[0, 159, 146, 150, 0, 1, 2, 3, 255, 0]);
        let png = scratch.file("d.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR fake");
        let read = |path: &PathBuf| compared_text(crate::open_file::load_dropped(path));
        assert_eq!(
            read(&text).as_deref().map(String::as_str),
            Some("alpha\nbeta\n")
        );
        assert!(read(&code).is_some());
        assert!(read(&binary).is_none());
        assert!(read(&png).is_none());
        assert!(read(&scratch.0).is_none(), "a folder");
    }

    #[test]
    fn the_dropped_sides_are_no_history_ids() {
        assert!(is_dropped_side(DROPPED_A) && is_dropped_side(DROPPED_B));
        assert!(!is_dropped_side(0) && !is_dropped_side(1) && !is_dropped_side(u64::MAX - 2));
        let mut picks = crate::diff::Selection::default();
        picks.add(3);
        picks.set_pair(DROPPED_A, DROPPED_B);
        assert_eq!(picks.pair(), Some((DROPPED_A, DROPPED_B)));
        assert!(picks.swap());
        assert_eq!(picks.pair(), Some((DROPPED_B, DROPPED_A)));
    }

    #[test]
    fn the_toast_reads_differently_in_dutch_and_english() {
        let en = crate::locale::t("compare_two_text_files");
        crate::locale::set_test_language(crate::settings::Language::Nl);
        let nl = crate::locale::t("compare_two_text_files");
        crate::locale::set_test_language(crate::settings::Language::En);
        assert_eq!(en, "Compare works with two text files");
        assert_eq!(nl, "Vergelijken werkt met twee tekstbestanden");
    }
}
