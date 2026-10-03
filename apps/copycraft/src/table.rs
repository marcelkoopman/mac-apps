//! Versions of a copied table. Each operation on the card (sort, dedupe, …) adds a version to
//! the history entry instead of a new entry: the original, then one per step, at most
//! [`MAX_VERSIONS`]. Only the steps are kept for every version ([`TableOp`], a plan Copycraft
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
    /// Drop the columns with one value in every row.
    DropConstant,
    /// Text columns of numbers or dates become numbers or dates (only when every value fits).
    FixTypes,
    /// Rows become columns (at most [`crate::table_ops::TRANSPOSE_MAX_ROWS`] rows).
    Transpose,
    /// Rows in the order of one column (empty cells last; equal values keep their order).
    Sort { column: String, descending: bool },
    /// Each value of one column once, with how many rows have it, the most common first.
    ValueCounts { column: String },
    /// These columns, in this order (the others dropped).
    SelectColumns { columns: Vec<String> },
    /// These columns dropped.
    DropColumns { columns: Vec<String> },
}

impl TableOp {
    /// The one-click steps, in the order of the "Table ▾" menu.
    pub const ONE_CLICK: [TableOp; 5] = [
        TableOp::Dedupe,
        TableOp::DropEmpty,
        TableOp::DropConstant,
        TableOp::FixTypes,
        TableOp::Transpose,
    ];

    /// Name of the version this step makes, for the version bar.
    pub fn label(&self) -> String {
        match self {
            Self::Dedupe => "Duplicates removed",
            Self::DropEmpty => "Empty rows and columns removed",
            Self::DropConstant => "Constant columns removed",
            Self::FixTypes => "Types fixed",
            Self::Transpose => "Transposed",
            Self::Sort { column, descending } => {
                let arrow = if *descending { "↓" } else { "↑" };
                return format!("Sorted by {column} {arrow}");
            }
            Self::ValueCounts { column } => return format!("Value counts of {column}"),
            Self::SelectColumns { columns } => {
                return match columns.as_slice() {
                    [one] => format!("Only {one}"),
                    _ => format!("{} columns chosen", columns.len()),
                };
            }
            Self::DropColumns { columns } => {
                return match columns.as_slice() {
                    [one] => format!("{one} removed"),
                    _ => format!("{} columns removed", columns.len()),
                };
            }
        }
        .to_string()
    }

    /// The menu item and search title that takes this step. A step on a column is titled with
    /// the column's name (it sits in that step's submenu, [`group`](Self::group)).
    pub fn title(&self) -> String {
        match self {
            Self::Sort { column, .. } | Self::ValueCounts { column } => return column.clone(),
            Self::DropColumns { columns } if columns.len() == 1 => return columns[0].clone(),
            Self::SelectColumns { columns } => return columns.first().cloned().unwrap_or_default(),
            _ => {}
        }
        match self {
            Self::Dedupe => "Remove duplicate rows",
            Self::DropEmpty => "Remove empty rows and columns",
            Self::DropConstant => "Remove constant columns",
            Self::FixTypes => "Fix types",
            Self::Transpose => "Transpose",
            Self::DropColumns { .. } => "Remove columns",
            Self::Sort { .. } | Self::ValueCounts { .. } | Self::SelectColumns { .. } => {
                unreachable!("titled above")
            }
        }
        .to_string()
    }

    /// The submenu of a step on a column: "Sort ascending", "Sort descending", "Value counts".
    pub fn group(&self) -> Option<&'static str> {
        match self {
            Self::Sort {
                descending: false, ..
            } => Some("Sort ascending"),
            Self::Sort {
                descending: true, ..
            } => Some("Sort descending"),
            Self::ValueCounts { .. } => Some("Value counts"),
            Self::DropColumns { columns } if columns.len() == 1 => Some("Remove column"),
            Self::SelectColumns { .. } => Some("Move column to front"),
            _ => None,
        }
    }

    /// The steps on a column of a table with `columns`, by submenu.
    pub fn column_steps(columns: &[String]) -> Vec<TableOp> {
        let sort = |descending| {
            columns.iter().map(move |column| TableOp::Sort {
                column: column.clone(),
                descending,
            })
        };
        sort(false)
            .chain(sort(true))
            .chain(columns.iter().map(|column| TableOp::ValueCounts {
                column: column.clone(),
            }))
            .chain(columns.iter().map(|column| TableOp::DropColumns {
                columns: vec![column.clone()],
            }))
            // Moving the first column to the front changes nothing.
            .chain(columns.iter().skip(1).map(|column| {
                TableOp::SelectColumns {
                    columns: std::iter::once(column)
                        .chain(columns.iter().filter(|other| *other != column))
                        .cloned()
                        .collect(),
                }
            }))
            .collect()
    }

    /// Search words for [`title`](Self::title).
    pub fn keywords(&self) -> &'static str {
        match self {
            Self::Dedupe => "dedupe unique duplicates rows table",
            Self::DropEmpty => "drop empty null blank trim rows columns table",
            Self::DropConstant => "drop constant columns same value table",
            Self::FixTypes => "fix types numbers dates cast table",
            Self::Transpose => "transpose pivot rows columns swap table",
            Self::Sort { .. } => "sort order column table",
            Self::ValueCounts { .. } => "value counts frequency count column table",
            Self::SelectColumns { .. } => "select move column front order table",
            Self::DropColumns { .. } => "drop remove delete column table",
        }
    }

    pub fn apply(&self, df: &DataFrame) -> Result<DataFrame, String> {
        use crate::table_ops;
        match self {
            Self::Dedupe => df
                .unique_stable(None, UniqueKeepStrategy::First, None)
                .map_err(|e| e.to_string()),
            Self::DropEmpty => table_ops::drop_empty(df),
            Self::DropConstant => table_ops::drop_constant(df),
            Self::FixTypes => table_ops::fix_types(df),
            Self::Transpose => table_ops::transpose(df),
            Self::Sort { column, descending } => table_ops::sort(df, column, *descending),
            Self::ValueCounts { column } => table_ops::value_counts(df, column),
            Self::SelectColumns { columns } => table_ops::select_columns(df, columns),
            Self::DropColumns { columns } => table_ops::drop_columns(df, columns),
        }
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
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "{MAX_VERSIONS} versions at most"),
            Self::NotATable => write!(f, "Not a table"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Failed(message) => write!(f, "{message}"),
        }
    }
}

/// What a job starts from.
enum JobBase {
    Frame(DataFrame),
    /// The copied text, parsed on the job's thread; that frame is the original.
    Source(Zeroizing<String>),
}

/// Work for one version: replay `ops` on the base. Send it to a thread and [`run`](Self::run).
pub struct Job {
    pub generation: u64,
    base: JobBase,
    /// How the copied text is read, when the job starts from it.
    options: crate::dataframe::ReadOptions,
    ops: Vec<TableOp>,
    /// The version shown when done.
    target: usize,
    /// A new step (`target` is then the version it makes).
    new_step: Option<TableOp>,
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
        for op in self.ops.iter().chain(self.new_step.iter()) {
            if cancel.load(Ordering::Relaxed) {
                return Err(TableError::Cancelled);
            }
            frame = op
                .apply(&frame)
                .map_err(|e| TableError::Failed(e.to_string()))?;
        }
        Ok(JobDone {
            generation: self.generation,
            target: self.target,
            new_step: self.new_step,
            original,
            frame,
            notes: read,
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

    /// "Original", then each step's label.
    pub fn labels(&self) -> Vec<String> {
        std::iter::once("Original".to_string())
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
        if self.frames.is_some() {
            return None;
        }
        Some(self.job(
            JobBase::Source(Zeroizing::new(source.to_string())),
            self.steps[..self.cursor].to_vec(),
            self.cursor,
            None,
        ))
    }

    /// The job for `op` on the version shown. Versions after it are dropped when it is done.
    pub fn push(&mut self, op: TableOp, source: &str) -> Result<Job, TableError> {
        if self.cursor + 2 > MAX_VERSIONS {
            return Err(TableError::Full);
        }
        let (base, ops) = match &self.frames {
            Some(frames) => (JobBase::Frame(frames.current.clone()), Vec::new()),
            None => (
                JobBase::Source(Zeroizing::new(source.to_string())),
                self.steps[..self.cursor].to_vec(),
            ),
        };
        Ok(self.job(base, ops, self.cursor + 1, Some(op)))
    }

    /// The job that shows version `index` (undo, redo, a click on the version bar). `None` when
    /// it is shown already or does not exist.
    pub fn goto(&mut self, index: usize, source: &str) -> Option<Job> {
        if index >= self.len() || (index == self.cursor && self.frames.is_some()) {
            return None;
        }
        let (base, ops) = match &self.frames {
            Some(frames) if index > self.cursor => (
                JobBase::Frame(frames.current.clone()),
                self.steps[self.cursor..index].to_vec(),
            ),
            Some(frames) => (
                JobBase::Frame(frames.original.clone()),
                self.steps[..index].to_vec(),
            ),
            None => (
                JobBase::Source(Zeroizing::new(source.to_string())),
                self.steps[..index].to_vec(),
            ),
        };
        Some(self.job(base, ops, index, None))
    }

    fn job(
        &mut self,
        base: JobBase,
        ops: Vec<TableOp>,
        target: usize,
        new_step: Option<TableOp>,
    ) -> Job {
        self.generation = next_generation();
        Job {
            generation: self.generation,
            options: self.options,
            base,
            ops,
            target,
            new_step,
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
        }
        self.cursor = done.target.min(self.steps.len());
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

    /// Drop the frames (the steps stay). A pending job is stale from now on.
    pub fn forget_frames(&mut self) {
        self.frames = None;
        self.generation = next_generation();
    }

    /// How the copied text is read.
    pub fn options(&self) -> crate::dataframe::ReadOptions {
        self.options
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
        let options = ReadOptions {
            month_first: true,
            ..ReadOptions::default()
        };
        let job = versions.reread(options, src).expect("reread");
        assert_eq!((versions.len(), versions.cursor()), (1, 0));
        assert!(run(&mut versions, job));
        assert!(versions.notes().is_some_and(|notes| notes.month_first));
        assert!(versions.reread(options, src).is_none());
        // The header on the first line instead: one column, "Export of 2026", is no table.
        let first_line = ReadOptions {
            header_line: Some(0),
            ..options
        };
        let job = versions.reread(first_line, src).expect("reread");
        assert_eq!(
            job.run(&AtomicBool::new(false)).err(),
            Some(TableError::NotATable)
        );
        assert_eq!(versions.clone().options(), first_line);
    }
}
