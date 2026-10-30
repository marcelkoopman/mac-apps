//! Versions of a copied table. Each operation on the card (sort, dedupe, …) adds a version to
//! the history entry instead of a new entry: the original, then one per step, at most
//! [`MAX_VERSIONS`]. Only the steps are kept for every version ([`TableOp`], a plan CopyCraft
//! replays itself, not polars' lazy engine, which would bring network crates); the original
//! and the version shown are kept as frames. A step taken from an older version drops the
//! versions after it.
//!
//! Work runs in a [`Job`] (on a background thread): replay steps from a frame, or parse the
//! copied text first when the frames were forgotten ([`TableVersions::forget_frames`], for
//! entries far from the one shown). A job carries the generation it was made for; only the
//! newest one is taken ([`TableVersions::finish`]).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use polars::prelude::*;
use zeroize::Zeroizing;

/// The original and up to 19 steps.
pub const MAX_VERSIONS: usize = 20;

/// Job generations, unique over every table, so a job for one table is never taken by another.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// One step from a version to the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableOp {
    /// Drop repeated rows, keeping the first of each.
    Dedupe,
    /// Trim text, then drop the rows and columns with no value.
    DropEmpty,
    /// Rows in the order of one column (empty cells last; equal values keep their order).
    Sort { column: String, descending: bool },
    /// These columns, in this order (the others dropped), chosen in the column picker from
    /// `kept_of` columns ("Kept 8 of 20 columns"), in the table's order.
    SelectColumns {
        columns: Vec<String>,
        kept_of: Option<usize>,
    },
}

impl TableOp {
    /// The one-click steps, in the order of the "Table ▾" menu.
    pub const ONE_CLICK: [TableOp; 2] = [TableOp::Dedupe, TableOp::DropEmpty];

    /// Name of the version this step makes, for the version capsule.
    pub fn label(&self) -> String {
        match self {
            Self::Dedupe => crate::locale::t("table_label_dedupe").to_string(),
            Self::DropEmpty => crate::locale::t("table_label_drop_empty").to_string(),
            Self::Sort { column, descending } => {
                let arrow = if *descending { "↓" } else { "↑" };
                crate::locale::tf("table_label_sorted", &[&column, &arrow])
            }
            Self::SelectColumns {
                columns,
                kept_of: Some(total),
            } => crate::locale::tf("table_label_kept", &[&columns.len(), &total]),
            Self::SelectColumns { columns, .. } => match columns.as_slice() {
                [one] => crate::locale::tf("table_label_only", &[one]),
                _ => crate::locale::tf("table_label_chosen", &[&columns.len()]),
            },
        }
    }

    /// The menu item and search title that takes this step. A sort is titled with the
    /// column's name (it sits in that step's submenu, [`group`](Self::group)).
    pub fn title(&self) -> String {
        match self {
            Self::Dedupe => crate::locale::t("table_remove_duplicates").to_string(),
            Self::DropEmpty => crate::locale::t("table_remove_empty").to_string(),
            Self::Sort { column, .. } => column.clone(),
            Self::SelectColumns { columns, .. } => {
                crate::locale::tf("table_keep_columns", &[&columns.len()])
            }
        }
    }

    /// The submenu of a step on a column: "Sort Ascending", "Sort Descending".
    pub fn group(&self) -> Option<&'static str> {
        match self {
            Self::Sort {
                descending: false, ..
            } => Some(crate::locale::t("sort_ascending")),
            Self::Sort {
                descending: true, ..
            } => Some(crate::locale::t("sort_descending")),
            _ => None,
        }
    }

    /// The sort steps of a table with `columns`: ascending, then descending, a column each.
    pub fn column_steps(columns: &[String]) -> Vec<TableOp> {
        let sort = |descending| {
            columns.iter().map(move |column| TableOp::Sort {
                column: column.clone(),
                descending,
            })
        };
        sort(false).chain(sort(true)).collect()
    }

    /// Search words for [`title`](Self::title).
    pub fn keywords(&self) -> &'static str {
        match self {
            Self::Dedupe => "dedupe unique duplicates rows table",
            Self::DropEmpty => "drop empty null blank trim rows columns table",
            Self::Sort { .. } => "sort order column table",
            Self::SelectColumns { .. } => "select choose keep columns table",
        }
    }

    /// The version after this step. `Ok(None)` when it would be the same: no version is made,
    /// and the card says [`unchanged_note`](Self::unchanged_note).
    pub fn apply(&self, df: &DataFrame) -> Result<Option<DataFrame>, String> {
        use crate::table_ops;
        let next = match self {
            Self::Dedupe => df
                .unique_stable(None, UniqueKeepStrategy::First, None)
                .map_err(|e| e.to_string()),
            Self::DropEmpty => table_ops::drop_empty(df),
            Self::Sort { column, descending } => table_ops::sort(df, column, *descending),
            Self::SelectColumns { columns, .. } => table_ops::select_columns(df, columns),
        };
        next.map(Some)
    }

    /// The meta-line note when this step would change nothing ([`apply`](Self::apply)).
    pub fn unchanged_note(&self) -> &'static str {
        crate::locale::t("nothing_to_change")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// [`MAX_VERSIONS`] already.
    Full,
    /// The copied text is not a table (any more).
    NotATable,
    /// A newer job took over.
    Cancelled,
    /// Polars refused the step (message for the card).
    Failed(String),
    /// The step would change nothing, so it makes no version (a note for the card, not an
    /// error): [`TableOp::unchanged_note`].
    Unchanged(&'static str),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "{}", crate::locale::tf("max_versions", &[&MAX_VERSIONS])),
            Self::NotATable => write!(f, "{}", crate::locale::t("not_a_table")),
            Self::Cancelled => write!(f, "{}", crate::locale::t("cancelled")),
            Self::Failed(message) => write!(f, "{message}"),
            Self::Unchanged(note) => write!(f, "{note}"),
        }
    }
}

/// What a job starts from.
enum JobBase {
    Frame(DataFrame),
    /// The copied text, parsed on the job's thread; that frame is the original.
    Source(Zeroizing<String>),
}

/// How many of `steps` must be replayed to recount every column choice with an "of N" in them:
/// up to and including the last such choice.
fn counted_after(steps: &[TableOp]) -> usize {
    steps
        .iter()
        .rposition(|op| {
            matches!(
                op,
                TableOp::SelectColumns {
                    kept_of: Some(_),
                    ..
                }
            )
        })
        .map_or(0, |last| last + 1)
}

/// Work for one version: replay `ops` on the base. Send it to a thread and [`run`](Self::run).
pub struct Job {
    pub generation: u64,
    base: JobBase,
    /// How the copied text is read, when the job starts from it.
    options: crate::dataframe::ReadOptions,
    ops: Vec<TableOp>,
    /// The index in the steps of `ops[0]`.
    first_step: usize,
    /// The version shown when done.
    target: usize,
    /// A new step (`target` is then the version it makes).
    new_step: Option<TableOp>,
    /// The steps after `target` (`count[0]` is step `target`), replayed on a scratch frame
    /// only to recount their column choices ("Kept 2 of N") after a step was taken out or put
    /// back. Only in a count job ([`TableVersions::count_job`]); empty for the others.
    count: Vec<TableOp>,
}

/// A finished [`Job`], for [`TableVersions::finish`].
#[derive(Clone)]
pub struct JobDone {
    generation: u64,
    target: usize,
    new_step: Option<TableOp>,
    original: Option<DataFrame>,
    frame: DataFrame,
    /// What reading the copied text found (a job from the text).
    notes: Option<crate::dataframe::ReadNotes>,
    /// The column count each replayed or recounted column choice saw, by step index:
    /// "Kept 3 of N" is right again after an earlier step was taken out or put back
    /// ([`TableVersions::remove_step`], [`TableVersions::restore_step`]).
    kept_of: Vec<(usize, usize)>,
}

impl JobDone {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Job {
    /// Replay the steps. `cancel` is checked before each one.
    pub fn run(self, cancel: &AtomicBool) -> Result<JobDone, TableError> {
        let mut read = None;
        let (original, mut frame) = match self.base {
            JobBase::Frame(frame) => (None, frame),
            JobBase::Source(text) => {
                let (frame, notes) = crate::dataframe::parse_table_with(&text, self.options)
                    .ok_or(TableError::NotATable)?;
                read = Some(notes);
                (Some(frame.clone()), frame)
            }
        };
        let mut kept_of = Vec::new();
        let steps = self.ops.iter().enumerate().map(|(i, op)| (op, Some(i)));
        for (op, replayed) in steps.chain(self.new_step.iter().map(|op| (op, None))) {
            let new = replayed.is_none();
            if cancel.load(Ordering::Relaxed) {
                return Err(TableError::Cancelled);
            }
            if let (
                Some(i),
                TableOp::SelectColumns {
                    kept_of: Some(_), ..
                },
            ) = (replayed, op)
            {
                kept_of.push((self.first_step + i, frame.width()));
            }
            match op.apply(&frame).map_err(TableError::Failed)? {
                Some(next) => frame = next,
                // A new step that changes nothing makes no version.
                None if new => return Err(TableError::Unchanged(op.unchanged_note())),
                // A step replayed on the same frame as before changes it the same way.
                None => {}
            }
        }
        // The later versions are not shown: only their column counts are wanted.
        let mut scratch: Option<DataFrame> = None;
        for (i, op) in self.count.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(TableError::Cancelled);
            }
            let seen = scratch.as_ref().unwrap_or(&frame);
            match op {
                TableOp::SelectColumns {
                    kept_of: Some(_), ..
                } => kept_of.push((self.target + i, seen.width())),
                // Order and repeated rows leave the columns as they are, and so what a later
                // Drop Empty finds empty: no need to work them out.
                TableOp::Sort { .. } | TableOp::Dedupe => continue,
                TableOp::SelectColumns { .. } | TableOp::DropEmpty => {}
            }
            match op.apply(seen) {
                Ok(Some(next)) => scratch = Some(next),
                Ok(None) => {}
                // It fails the same way when that version is shown; the rest keep their count.
                Err(_) => break,
            }
        }
        Ok(JobDone {
            generation: self.generation,
            target: self.target,
            new_step: self.new_step,
            original,
            frame,
            notes: read,
            kept_of,
        })
    }
}

struct Frames {
    /// The job that made `current`.
    generation: u64,
    original: DataFrame,
    /// The version at the cursor.
    current: DataFrame,
}

/// The versions of one table (see the module docs).
#[derive(Default)]
pub struct TableVersions {
    steps: Vec<TableOp>,
    /// The version shown: 0 is the original, `n` the one after `steps[n - 1]`.
    cursor: usize,
    generation: u64,
    frames: Option<Frames>,
    options: crate::dataframe::ReadOptions,
    notes: Option<crate::dataframe::ReadNotes>,
    /// Display preference once the entry was shown (`Some(false)` = grid). Overview UI removed.
    overview: Option<bool>,
    /// The table window shows this table: its frames stay when the card moves far away
    /// ([`forget_frames`](Self::forget_frames)). They still go with the entry.
    pinned: bool,
    /// The frame shown is not the version at the cursor any more: a step before it was taken
    /// out or put back ([`remove_step`](Self::remove_step)); the job for the cursor runs.
    behind: bool,
    /// The step [`remove_step`](Self::remove_step) took out last, for ⌘Z
    /// ([`restore_step`](Self::restore_step)); one, as history keeps one removed copy.
    removed: Option<RemovedStep>,
    /// A step went or came back: the later column choices' "of N" waits for a count job
    /// ([`count_job`](Self::count_job)), run quietly once the version shown is there.
    counts_stale: bool,
    /// The newest count job ([`finish_counts`](Self::finish_counts)); a change to the steps
    /// makes the one running stale.
    count_generation: u64,
}

/// A step taken out of the chain: the version it made, and the step.
#[derive(Debug, Clone, PartialEq)]
struct RemovedStep {
    version: usize,
    op: TableOp,
}

impl Clone for TableVersions {
    /// A copy keeps the steps; frames are worked out again when needed.
    fn clone(&self) -> Self {
        Self {
            steps: self.steps.clone(),
            cursor: self.cursor,
            generation: self.generation,
            frames: None,
            options: self.options,
            notes: self.notes,
            overview: self.overview,
            pinned: false,
            behind: false,
            removed: None,
            counts_stale: false,
            count_generation: 0,
        }
    }
}

impl TableVersions {
    /// Versions there are: the original and one per step.
    pub fn len(&self) -> usize {
        self.steps.len() + 1
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The overview or the grid ([`crate::dataframe::shows_overview`]); `None` until the entry
    /// was shown with its frame.
    pub fn overview_choice(&self) -> Option<bool> {
        self.overview
    }

    /// The entry is shown with a version of `width` columns: the first time, lock to the grid
    /// (Table always shows the grid; column picking is Choose Columns… only).
    pub fn shown_with(&mut self, width: usize) {
        if self.overview.is_none() && width > 0 {
            self.overview = Some(false);
        }
    }

    /// "Original", then each step's label.
    pub fn labels(&self) -> Vec<String> {
        std::iter::once(crate::locale::t("original").to_string())
            .chain(self.steps.iter().map(TableOp::label))
            .collect()
    }

    /// Which frame [`frame`](Self::frame) is: the generation of the job that made it.
    pub fn frame_id(&self) -> u64 {
        self.frames.as_ref().map_or(0, |frames| frames.generation)
    }

    /// The version shown, when it is worked out.
    pub fn frame(&self) -> Option<&DataFrame> {
        self.frames.as_ref().map(|frames| &frames.current)
    }

    /// The job that works out the version shown, when its frame is not there.
    pub fn load(&mut self, source: &str) -> Option<Job> {
        if self.frames.is_some() && !self.behind {
            return None;
        }
        Some(self.job_to(self.cursor, source))
    }

    /// The job for `op` on the version shown. Versions after it are dropped when it is done.
    pub fn push(&mut self, op: TableOp, source: &str) -> Result<Job, TableError> {
        if self.cursor + 2 > MAX_VERSIONS {
            return Err(TableError::Full);
        }
        self.removed = None;
        let (base, ops) = match &self.frames {
            Some(frames) if !self.behind => (
                JobBase::Frame(frames.current.clone()),
                (self.cursor, Vec::new()),
            ),
            Some(frames) => (
                JobBase::Frame(frames.original.clone()),
                (0, self.steps[..self.cursor].to_vec()),
            ),
            None => (
                JobBase::Source(Zeroizing::new(source.to_string())),
                (0, self.steps[..self.cursor].to_vec()),
            ),
        };
        Ok(self.job(base, ops, self.cursor + 1, Some(op)))
    }

    /// The job that shows version `index` (undo, redo, a pick in the version menu). `None` when
    /// it is shown already or does not exist.
    pub fn goto(&mut self, index: usize, source: &str) -> Option<Job> {
        if index >= self.len() || (index == self.cursor && self.frames.is_some() && !self.behind) {
            return None;
        }
        Some(self.job_to(index, source))
    }

    /// The job that shows version `index`: from the frame shown when `index` is after it, else
    /// from the original (from the copied text when the frames are gone).
    fn job_to(&mut self, index: usize, source: &str) -> Job {
        let (base, ops) = match &self.frames {
            Some(frames) if index > self.cursor && !self.behind => (
                JobBase::Frame(frames.current.clone()),
                (self.cursor, self.steps[self.cursor..index].to_vec()),
            ),
            Some(frames) => (
                JobBase::Frame(frames.original.clone()),
                (0, self.steps[..index].to_vec()),
            ),
            None => (
                JobBase::Source(Zeroizing::new(source.to_string())),
                (0, self.steps[..index].to_vec()),
            ),
        };
        self.job(base, ops, index, None)
    }

    /// Version `version` can be taken out of the chain: any but the original (0).
    pub fn removable(&self, version: usize) -> bool {
        version > 0 && version < self.len()
    }

    /// Take version `version`'s step out of the chain (the × on its card, ⌫); nothing happens
    /// when it is not [`removable`](Self::removable). The steps after it stay and are replayed
    /// without it: every step names its columns, and without an earlier step a table has the
    /// same columns and rows or more, so they still apply. The cursor stays on the version it
    /// showed; when that was the one taken out, on the one before it. Returns the job for the
    /// version then shown when its frame changes (it was at or after `version`), `None` when
    /// only a later version went: the grid stays as it is. The later column choices' "of N"
    /// is recounted after that by a quiet [`count_job`](Self::count_job). ⌘Z puts the step back
    /// ([`restore_step`]).
    ///
    /// [`restore_step`]: Self::restore_step
    pub fn remove_step(&mut self, version: usize, source: &str) -> Option<Job> {
        if !self.removable(version) {
            return None;
        }
        let op = self.steps.remove(version - 1);
        self.removed = Some(RemovedStep { version, op });
        self.steps_changed();
        if self.cursor < version {
            // The version shown stays; the later column choices are recounted quietly.
            return None;
        }
        self.cursor -= 1;
        self.behind = self.frames.is_some();
        Some(self.job_to(self.cursor, source))
    }

    /// The steps changed: a count job running is stale, and one is needed when a column choice
    /// with an "of N" may now see another number of columns.
    fn steps_changed(&mut self) {
        self.count_generation = next_generation();
        self.counts_stale = counted_after(&self.steps) > 0;
    }

    /// The quiet job that recounts the "of N" of the column choices after the version shown,
    /// from its frame, once a step went or came back. `None` when nothing waits for it, or
    /// while the version shown is still being worked out (ask again when it is done). Its
    /// result goes to [`finish_counts`](Self::finish_counts); the frame shown never changes.
    pub fn count_job(&mut self) -> Option<Job> {
        if !self.counts_stale || self.behind {
            return None;
        }
        let frames = self.frames.as_ref()?;
        let later = &self.steps[self.cursor.min(self.steps.len())..];
        let count = later[..counted_after(later)].to_vec();
        self.counts_stale = false;
        if count.is_empty() {
            return None;
        }
        Some(Job {
            generation: self.count_generation,
            options: self.options,
            base: JobBase::Frame(frames.current.clone()),
            ops: Vec::new(),
            first_step: self.cursor,
            target: self.cursor,
            new_step: None,
            count,
        })
    }

    /// `generation` is this table's newest count job ([`count_job`](Self::count_job)).
    pub fn awaits_counts(&self, generation: u64) -> bool {
        self.count_generation == generation
    }

    /// Take a finished count job: the later column choices' "of N". `false` (and nothing
    /// changes) when the steps changed since.
    pub fn finish_counts(&mut self, done: JobDone) -> bool {
        if done.generation != self.count_generation {
            return false;
        }
        self.set_kept_of(&done.kept_of);
        true
    }

    fn set_kept_of(&mut self, kept_of: &[(usize, usize)]) {
        for &(index, width) in kept_of {
            if let Some(TableOp::SelectColumns {
                kept_of: Some(total),
                ..
            }) = self.steps.get_mut(index)
            {
                *total = width;
            }
        }
    }

    /// The step [`remove_step`](Self::remove_step) took out can go back (⌘Z).
    pub fn can_restore(&self) -> bool {
        self.removed.is_some()
    }

    /// Forget the step taken out: another action came after it.
    pub fn forget_removed(&mut self) {
        self.removed = None;
    }

    /// ⌘Z after [`remove_step`](Self::remove_step): the step goes back where it was and its
    /// version is shown. `None` when no step waits (or there is no room for it).
    pub fn restore_step(&mut self, source: &str) -> Option<Job> {
        let removed = self.removed.take()?;
        if removed.version > self.len() || self.len() >= MAX_VERSIONS {
            return None;
        }
        self.steps.insert(removed.version - 1, removed.op);
        self.steps_changed();
        if self.cursor >= removed.version {
            // The version shown was replayed without the step: its frame is behind now.
            self.cursor += 1;
            self.behind = self.frames.is_some();
        }
        Some(self.job_to(removed.version, source))
    }

    fn job(
        &mut self,
        base: JobBase,
        (first_step, ops): (usize, Vec<TableOp>),
        target: usize,
        new_step: Option<TableOp>,
    ) -> Job {
        self.generation = next_generation();
        Job {
            generation: self.generation,
            options: self.options,
            base,
            ops,
            first_step,
            target,
            new_step,
            count: Vec::new(),
        }
    }

    /// Take a finished job: the version it made is shown. `false` (and nothing changes) when a
    /// newer job was made since, or the frames it needs were forgotten meanwhile.
    pub fn finish(&mut self, done: JobDone) -> bool {
        if done.generation != self.generation {
            return false;
        }
        let original = match (done.original, self.frames.take()) {
            (Some(original), _) => original,
            (None, Some(frames)) => frames.original,
            (None, None) => return false,
        };
        if let Some(op) = done.new_step {
            self.steps.truncate(self.cursor);
            self.steps.push(op);
            // The later steps are gone, and with them any count waiting.
            self.count_generation = next_generation();
            self.counts_stale = false;
        }
        self.set_kept_of(&done.kept_of);
        self.cursor = done.target.min(self.steps.len());
        self.behind = false;
        if done.notes.is_some() {
            self.notes = done.notes;
        }
        self.frames = Some(Frames {
            generation: done.generation,
            original,
            current: done.frame,
        });
        true
    }

    /// Keep the frames while the table window shows this table (`true`), or not any more.
    pub fn pin(&mut self, pinned: bool) {
        self.pinned = pinned;
    }

    /// Drop the frames (the steps stay), unless the table window shows them ([`pin`]). A
    /// pending job is stale from now on.
    ///
    /// [`pin`]: Self::pin
    pub fn forget_frames(&mut self) {
        if self.pinned {
            return;
        }
        self.frames = None;
        self.behind = false;
        self.generation = next_generation();
    }

    /// How the copied text is read.
    pub fn options(&self) -> crate::dataframe::ReadOptions {
        self.options
    }

    /// Set how the next read of the original starts (the Settings date order), before any notes.
    pub fn set_options(&mut self, options: crate::dataframe::ReadOptions) {
        if self.notes.is_none() {
            self.options = options;
        }
    }

    /// What reading the copied text found, once it was read.
    pub fn notes(&self) -> Option<crate::dataframe::ReadNotes> {
        self.notes
    }

    /// Read the copied text as `options` says: the table starts over from the original (its
    /// columns can change, so the steps go). `None` when it is read that way already.
    pub fn reread(&mut self, options: crate::dataframe::ReadOptions, source: &str) -> Option<Job> {
        if options == self.options {
            return None;
        }
        self.options = options;
        self.steps.clear();
        self.cursor = 0;
        self.frames = None;
        self.behind = false;
        self.removed = None;
        self.counts_stale = false;
        self.count_generation = next_generation();
        self.notes = None;
        self.load(source)
    }

    /// `generation` is this table's newest job.
    pub fn awaits(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_VERSIONS, TableError, TableOp, TableVersions};
    use crate::dataframe::ReadOptions;
    use std::sync::atomic::AtomicBool;

    const SRC: &str = "name,n\na,1\na,1\nb,2";

    fn run(versions: &mut TableVersions, job: super::Job) -> bool {
        let done = job.run(&AtomicBool::new(false)).expect("job");
        versions.finish(done)
    }

    /// The quiet count job, run as the app runs it once the version shown is there. `true`
    /// when there was one and it was taken.
    fn count(versions: &mut TableVersions) -> bool {
        let Some(job) = versions.count_job() else {
            return false;
        };
        let done = job.run(&AtomicBool::new(false)).expect("count");
        versions.finish_counts(done)
    }

    #[test]
    fn the_first_version_shown_picks_the_overview_and_a_choice_holds_for_every_version() {
        let mut wide = TableVersions::default();
        assert_eq!(wide.overview_choice(), None);
        wide.shown_with(0);
        assert_eq!(wide.overview_choice(), None);
        // Wide and narrow both lock to the grid (no Show Columns chip).
        wide.shown_with(20);
        assert_eq!(wide.overview_choice(), Some(false));
        wide.shown_with(6);
        assert_eq!(wide.overview_choice(), Some(false));
        wide.shown_with(20);
        assert_eq!(wide.overview_choice(), Some(false));
        assert_eq!(wide.clone().overview_choice(), Some(false));
        let mut narrow = TableVersions::default();
        narrow.shown_with(3);
        assert_eq!(narrow.overview_choice(), Some(false));
    }

    #[test]
    fn a_step_adds_a_version_and_undo_goes_back() {
        let mut versions = TableVersions::default();
        let load = versions.load(SRC).expect("load");
        assert!(run(&mut versions, load));
        assert_eq!(versions.frame().map(|df| df.height()), Some(3));
        assert!(versions.load(SRC).is_none());
        let job = versions.push(TableOp::Dedupe, SRC).expect("push");
        assert!(run(&mut versions, job));
        assert_eq!((versions.len(), versions.cursor()), (2, 1));
        assert_eq!(versions.labels(), ["Original", "Duplicates removed"]);
        assert_eq!(versions.frame().map(|df| df.height()), Some(2));
        assert!(versions.cursor() > 0 && versions.cursor() + 1 == versions.len());
        let undo = versions.goto(0, SRC).expect("undo");
        assert!(run(&mut versions, undo));
        assert_eq!(versions.frame().map(|df| df.height()), Some(3));
        assert!(versions.cursor() + 1 < versions.len());
        let redo = versions.goto(1, SRC).expect("redo");
        assert!(run(&mut versions, redo));
        assert_eq!(versions.frame().map(|df| df.height()), Some(2));
        assert!(versions.goto(1, SRC).is_none());
        assert!(versions.goto(2, SRC).is_none());
    }

    #[test]
    fn a_step_from_an_older_version_drops_the_later_ones() {
        let mut versions = TableVersions::default();
        for _ in 0..3 {
            let job = versions.push(TableOp::Dedupe, SRC).expect("push");
            assert!(run(&mut versions, job));
        }
        assert_eq!(versions.len(), 4);
        let back = versions.goto(1, SRC).expect("goto");
        assert!(run(&mut versions, back));
        let job = versions.push(TableOp::Dedupe, SRC).expect("push");
        assert!(run(&mut versions, job));
        assert_eq!((versions.len(), versions.cursor()), (3, 2));
        assert_eq!(versions.cursor() + 1, versions.len());
    }

    #[test]
    fn a_stale_job_is_not_taken() {
        let mut versions = TableVersions::default();
        let first = versions.push(TableOp::Dedupe, SRC).expect("push");
        let second = versions.goto(0, SRC).expect("goto");
        let stale = first.run(&AtomicBool::new(false)).expect("job");
        assert!(!versions.finish(stale));
        assert_eq!(versions.len(), 1);
        assert!(run(&mut versions, second));
        // A cancelled job stops before its steps.
        let job = versions.push(TableOp::Dedupe, SRC).expect("push");
        assert_eq!(
            job.run(&AtomicBool::new(true)).err(),
            Some(TableError::Cancelled)
        );
    }

    #[test]
    fn forgotten_frames_come_back_from_the_text_with_the_steps() {
        let mut versions = TableVersions::default();
        let job = versions.push(TableOp::Dedupe, SRC).expect("push");
        let pending = versions.push(TableOp::Dedupe, SRC).expect("push");
        assert!(!run(&mut versions, job));
        assert!(run(&mut versions, pending));
        versions.forget_frames();
        assert!(versions.frame().is_none());
        assert_eq!(versions.cursor(), 1);
        let load = versions.load(SRC).expect("load");
        assert!(run(&mut versions, load));
        assert_eq!(versions.frame().map(|df| df.height()), Some(2));
        let copy = versions.clone();
        assert!(copy.frame().is_none());
        assert_eq!(copy.labels(), versions.labels());
    }

    #[test]
    fn a_pinned_table_keeps_its_frames() {
        let mut versions = TableVersions::default();
        let job = versions.push(TableOp::Dedupe, SRC).expect("push");
        assert!(run(&mut versions, job));
        versions.pin(true);
        versions.forget_frames();
        assert_eq!(versions.frame().map(|df| df.height()), Some(2));
        // A copy of the entry is not shown in the window.
        let mut copy = versions.clone();
        copy.forget_frames();
        assert!(copy.frame().is_none());
        versions.pin(false);
        versions.forget_frames();
        assert!(versions.frame().is_none());
    }

    #[test]
    fn stops_at_the_version_limit_and_on_text_that_is_no_table() {
        let mut versions = TableVersions::default();
        for _ in 1..MAX_VERSIONS {
            let job = versions.push(TableOp::Dedupe, SRC).expect("push");
            assert!(run(&mut versions, job));
        }
        assert_eq!(versions.len(), MAX_VERSIONS);
        assert_eq!(
            versions.push(TableOp::Dedupe, SRC).err(),
            Some(TableError::Full)
        );
        // From an older version there is room again.
        let back = versions.goto(3, SRC).expect("goto");
        assert!(run(&mut versions, back));
        assert!(versions.push(TableOp::Dedupe, SRC).is_ok());
        let mut prose = TableVersions::default();
        let job = prose.load("just words").expect("load");
        assert_eq!(
            job.run(&AtomicBool::new(false)).err(),
            Some(TableError::NotATable)
        );
    }

    #[test]
    fn reading_the_text_another_way_starts_over() {
        let src = "Export of 2026\nwhen,n\n01/02/2026,1\n01/02/2026,1\n03/04/2026,2";
        let mut versions = TableVersions::default();
        let job = versions.push(TableOp::Dedupe, src).expect("push");
        assert!(run(&mut versions, job));
        let notes = versions.notes().expect("notes");
        assert_eq!(notes.start.map(|start| start.header_line), Some(1));
        assert!(notes.ambiguous_dates && !notes.month_first);
        // Dates the other way round: the steps go, the original is read again.
        let options = ReadOptions { month_first: true };
        let job = versions.reread(options, src).expect("reread");
        assert_eq!((versions.len(), versions.cursor()), (1, 0));
        assert!(run(&mut versions, job));
        assert!(versions.notes().is_some_and(|notes| notes.month_first));
        assert!(versions.reread(options, src).is_none());
        assert_eq!(versions.clone().options(), options);
    }

    /// The original, then: duplicates removed (v1), the name column only (v2), sorted by name
    /// descending (v3); v3 shown.
    fn three_steps() -> TableVersions {
        let mut versions = TableVersions::default();
        let steps = [
            TableOp::Dedupe,
            TableOp::SelectColumns {
                columns: vec!["name".into()],
                kept_of: None,
            },
            TableOp::Sort {
                column: "name".into(),
                descending: true,
            },
        ];
        for op in steps {
            let job = versions.push(op, SRC).expect("push");
            assert!(run(&mut versions, job));
        }
        assert_eq!((versions.len(), versions.cursor()), (4, 3));
        versions
    }

    fn shape(versions: &TableVersions) -> Option<(usize, usize)> {
        versions.frame().map(|df| (df.height(), df.width()))
    }

    #[test]
    fn the_original_cannot_be_removed() {
        let mut versions = three_steps();
        assert!(!versions.removable(0) && !versions.removable(4));
        assert!(versions.removable(1) && versions.removable(3));
        assert!(versions.remove_step(0, SRC).is_none());
        assert!(versions.remove_step(4, SRC).is_none());
        assert_eq!((versions.len(), versions.cursor()), (4, 3));
        assert!(!versions.can_restore());
        let mut alone = TableVersions::default();
        assert!(!alone.removable(0));
        assert!(alone.remove_step(0, SRC).is_none());
    }

    #[test]
    fn removing_an_earlier_version_replays_the_later_steps_without_it() {
        let mut versions = three_steps();
        let labels = versions.labels();
        let replay = versions.remove_step(1, SRC).expect("replay");
        // The step goes at once; the version shown stays (one place to the left).
        assert_eq!((versions.len(), versions.cursor()), (3, 2));
        assert_eq!(
            versions.labels(),
            [labels[0].clone(), labels[2].clone(), labels[3].clone()]
        );
        assert!(versions.can_restore());
        // Until the replay is in, the frame shown is behind: a cancelled replay runs again.
        drop(replay);
        assert!(versions.goto(2, SRC).is_some());
        let job = versions.load(SRC).expect("load while behind");
        assert!(run(&mut versions, job));
        // The repeated row is back, the later steps still apply (name only, sorted).
        assert_eq!(shape(&versions), Some((3, 1)));
        assert!(versions.goto(2, SRC).is_none());
        assert!(versions.load(SRC).is_none());
    }

    #[test]
    fn removing_an_earlier_column_choice_recounts_kept_of() {
        let src = "a,b,c,d\n1,2,3,4\n5,6,7,8";
        let mut versions = TableVersions::default();
        let picks = [
            TableOp::SelectColumns {
                columns: vec!["a".into(), "b".into(), "c".into()],
                kept_of: Some(4),
            },
            TableOp::SelectColumns {
                columns: vec!["a".into(), "c".into()],
                kept_of: Some(3),
            },
        ];
        for op in picks {
            let job = versions.push(op, src).expect("push");
            assert!(run(&mut versions, job));
        }
        let kept = |versions: &TableVersions| match versions.steps.last() {
            Some(TableOp::SelectColumns { kept_of, .. }) => *kept_of,
            _ => None,
        };
        assert_eq!(kept(&versions), Some(3));
        // Without the first choice, the second one picks from all 4 columns.
        let job = versions.remove_step(1, src).expect("replay");
        assert!(
            versions.count_job().is_none(),
            "not before the version shown is there"
        );
        assert!(run(&mut versions, job));
        assert_eq!(kept(&versions), Some(4));
        assert!(!count(&mut versions));
        assert_eq!(
            versions.labels()[1],
            crate::locale::tf("table_label_kept", &[&2, &4])
        );
        // ⌘Z puts the first choice back and shows it: the later choice, not replayed, is
        // recounted to 2 of 3 by the quiet count job after it.
        let job = versions.restore_step(src).expect("restore");
        assert!(run(&mut versions, job));
        assert_eq!(versions.cursor(), 1);
        assert_eq!(kept(&versions), Some(4));
        assert!(count(&mut versions));
        assert_eq!(kept(&versions), Some(3));
        assert_eq!(
            versions.labels()[2],
            crate::locale::tf("table_label_kept", &[&2, &3])
        );
        let job = versions.goto(2, src).expect("goto");
        assert!(run(&mut versions, job));
        assert_eq!(kept(&versions), Some(3));
        // Taking out the first choice while it is shown (the later one is not): recounted too.
        let job = versions.goto(1, src).expect("goto");
        assert!(run(&mut versions, job));
        let job = versions.remove_step(1, src).expect("replay");
        assert!(run(&mut versions, job));
        assert!(count(&mut versions));
        assert_eq!(versions.cursor(), 0);
        assert_eq!(kept(&versions), Some(4));
        // A later version going while an earlier one shows: the one after it is recounted.
        let job = versions.restore_step(src).expect("restore");
        assert!(run(&mut versions, job));
        assert!(count(&mut versions));
        let job = versions.goto(0, src).expect("goto");
        assert!(run(&mut versions, job));
        // No job for the version shown: the grid stays (no dimming), only the count runs.
        let id = versions.frame_id();
        assert!(versions.remove_step(1, src).is_none());
        assert_eq!(kept(&versions), Some(3));
        assert!(count(&mut versions));
        assert_eq!((versions.cursor(), kept(&versions)), (0, Some(4)));
        assert_eq!(versions.frame_id(), id);
        assert!(versions.load(src).is_none());
        // A count from before a change to the steps is stale.
        let job = versions.restore_step(src).expect("restore");
        assert!(run(&mut versions, job));
        let stale = versions.count_job().expect("count");
        let job = versions.goto(0, src).expect("goto");
        assert!(run(&mut versions, job));
        assert!(versions.remove_step(1, src).is_none());
        let stale = stale.run(&AtomicBool::new(false)).expect("count");
        assert!(!versions.finish_counts(stale));
        assert!(count(&mut versions));
        assert_eq!(kept(&versions), Some(4));
    }

    #[test]
    fn a_count_skips_sorts_and_dedupes_but_follows_drop_empty() {
        // Column c is empty: Drop Empty takes it out, so the choice after it sees 3 of them.
        let src = "a,b,c,d\n1,2,,4\n1,2,,4\n5,6,,8";
        let mut versions = TableVersions::default();
        let load = versions.load(src).expect("load");
        assert!(run(&mut versions, load));
        for op in [
            TableOp::Dedupe,
            TableOp::DropEmpty,
            TableOp::Sort {
                column: "a".into(),
                descending: true,
            },
            TableOp::SelectColumns {
                columns: vec!["a".into(), "d".into()],
                kept_of: Some(99),
            },
        ] {
            let job = versions.push(op, src).expect("push");
            assert!(run(&mut versions, job));
        }
        let job = versions.goto(0, src).expect("goto");
        assert!(run(&mut versions, job));
        // Taking out the dedupe: recounted from the original through Drop Empty.
        assert!(versions.remove_step(1, src).is_none());
        assert!(count(&mut versions));
        assert_eq!(
            versions.steps.last(),
            Some(&TableOp::SelectColumns {
                columns: vec!["a".into(), "d".into()],
                kept_of: Some(3),
            })
        );
    }

    #[test]
    fn removing_the_version_shown_shows_the_one_before_it() {
        let mut versions = three_steps();
        let job = versions.remove_step(3, SRC).expect("replay");
        assert_eq!((versions.len(), versions.cursor()), (3, 2));
        assert!(run(&mut versions, job));
        assert_eq!(versions.cursor(), 2);
        assert_eq!(shape(&versions), Some((2, 1)));
        // The last step left: only the original remains, and is shown.
        let job = versions.remove_step(2, SRC).expect("replay");
        assert!(run(&mut versions, job));
        let job = versions.remove_step(1, SRC).expect("replay");
        assert!(run(&mut versions, job));
        assert_eq!((versions.len(), versions.cursor()), (1, 0));
        assert_eq!(shape(&versions), Some((3, 2)));
    }

    #[test]
    fn removing_a_later_version_keeps_the_frame_shown() {
        let mut versions = three_steps();
        let back = versions.goto(1, SRC).expect("goto");
        assert!(run(&mut versions, back));
        let id = versions.frame_id();
        assert!(versions.remove_step(3, SRC).is_none());
        assert_eq!((versions.len(), versions.cursor()), (3, 1));
        assert_eq!(versions.frame_id(), id);
        assert!(versions.load(SRC).is_none());
        assert_eq!(shape(&versions), Some((2, 2)));
    }

    #[test]
    fn undo_puts_the_removed_step_back_and_shows_it() {
        let mut versions = three_steps();
        let labels = versions.labels();
        let job = versions.remove_step(2, SRC).expect("replay");
        assert!(run(&mut versions, job));
        assert_eq!(shape(&versions), Some((2, 2)));
        let job = versions.restore_step(SRC).expect("restore");
        assert_eq!(versions.labels(), labels);
        assert!(!versions.can_restore());
        assert!(versions.restore_step(SRC).is_none());
        assert!(run(&mut versions, job));
        assert_eq!(versions.cursor(), 2);
        assert_eq!(shape(&versions), Some((2, 1)));
        // A later version taken out while an earlier one shows comes back shown too.
        let back = versions.goto(1, SRC).expect("goto");
        assert!(run(&mut versions, back));
        assert!(versions.remove_step(3, SRC).is_none());
        let job = versions.restore_step(SRC).expect("restore");
        assert!(run(&mut versions, job));
        assert_eq!((versions.len(), versions.cursor()), (4, 3));
        assert_eq!(versions.labels(), labels);
    }

    #[test]
    fn a_new_step_or_reread_forgets_the_removed_step() {
        let mut versions = three_steps();
        let job = versions.remove_step(1, SRC).expect("replay");
        assert!(run(&mut versions, job));
        assert!(versions.can_restore());
        let job = versions.push(TableOp::DropEmpty, SRC).expect("push");
        assert!(!versions.can_restore());
        drop(job);
        versions.remove_step(1, SRC);
        versions.forget_removed();
        assert!(!versions.can_restore());
        versions.remove_step(1, SRC);
        versions.reread(ReadOptions { month_first: true }, SRC);
        assert!(!versions.can_restore());
    }

    #[test]
    fn a_job_from_before_a_removal_is_stale() {
        let mut versions = three_steps();
        let before = versions.goto(0, SRC).expect("goto");
        let job = versions.remove_step(3, SRC).expect("replay");
        let stale = before.run(&AtomicBool::new(false)).expect("job");
        assert!(!versions.finish(stale));
        assert!(run(&mut versions, job));
        assert_eq!(versions.cursor(), 2);
    }
}
