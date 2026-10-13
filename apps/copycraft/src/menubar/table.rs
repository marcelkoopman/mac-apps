//! Table actions: table versions, the table window and table jobs.

use super::*;

impl App {
    /// The text on the card and where its table versions live, when it is a table.
    pub(crate) fn table_source(&self) -> Option<(Zeroizing<String>, TableHome)> {
        // What the card shows (a browsed history entry, an unrecorded copy), not a fresh read of
        // the pasteboard, which may differ or be withheld once the card closes.
        let text = match &self.card_text {
            Some(text) => text.clone(),
            None => Zeroizing::new(self.source_text()?),
        };
        if !crate::toolbar_visibility::shows_dataframe_button(format::detect(&text), &text) {
            return None;
        }
        let home = pick_table_home(
            self.history.text_index(&text, self.history_cursor),
            self.opened.is_some(),
        );
        Some((text, home))
    }

    /// The loose slot holds `text`'s table: a new text starts it over.
    pub(crate) fn adopt_loose(&mut self, text: &str) {
        if self.loose_text.as_deref().map(|t| t.as_str()) != Some(text) {
            self.loose_table = crate::settings::preferred_table();
            self.loose_text = Some(Zeroizing::new(text.to_string()));
        }
    }

    pub(crate) fn table_at(&mut self, home: TableHome) -> Option<&mut TableVersions> {
        match home {
            TableHome::History(index) => self.history.table_mut(index),
            TableHome::Opened => Some(&mut self.opened_table),
            TableHome::Loose => Some(&mut self.loose_table),
        }
    }

    /// The text and table of `target`: the card's ([`table_source`](Self::table_source)) or
    /// the table window's ([`window_source`](Self::window_source)).
    pub(crate) fn target_source(
        &self,
        target: TableTarget,
    ) -> Option<(Zeroizing<String>, TableHome)> {
        match target {
            TableTarget::Card => self.table_source(),
            TableTarget::Window => self.window_source(),
        }
    }

    /// The versions of `target`'s table. The window's leaves the other entries' frames alone.
    pub(crate) fn target_table(
        &mut self,
        target: TableTarget,
        home: TableHome,
    ) -> Option<&mut TableVersions> {
        match (target, home) {
            (TableTarget::Window, TableHome::History(index)) => {
                self.history.table_mut_in_place(index)
            }
            _ => self.table_at(home),
        }
    }

    /// The entry the table window shows, while it is still there (in history, or the chosen
    /// file still on the card).
    pub(crate) fn window_source(&self) -> Option<(Zeroizing<String>, TableHome)> {
        let window = self.table_window.as_ref()?;
        let text = Zeroizing::new(window.text.to_string());
        let home = if window.opened {
            let opened = Zeroizing::new(self.opened_text()?);
            (opened.as_str() == text.as_str()).then_some(TableHome::Opened)?
        } else {
            match self.history.text_index(&text, self.history_cursor) {
                Some(index) => TableHome::History(index),
                None => (self.loose_text.as_deref().map(|t| t.as_str()) == Some(text.as_str()))
                    .then_some(TableHome::Loose)?,
            }
        };
        Some((text, home))
    }

    /// "Open table": the table on the card in the table window (revealed, no blur); the card
    /// may close. The window then stays with that entry.
    pub(crate) fn open_table_window(&mut self) {
        let Some((text, home)) = self.table_source() else {
            crate::diag::log(crate::diag::LogEvent::TableNotShown);
            return;
        };
        if home == TableHome::Loose {
            self.adopt_loose(&text);
        }
        // Counts only: never the table's text.
        crate::diag::log(crate::diag::LogEvent::TableOpened {
            columns: crate::dataframe::header_columns(&text).unwrap_or(0) as u64,
            rows: text.lines().count().saturating_sub(1) as u64,
        });
        self.close_table_window();
        launcher::close_settings();
        launcher::close_about();
        if let Some(table) = self.target_table(TableTarget::Window, home) {
            table.pin(true);
        }
        self.table_window = Some(TableWindow {
            text,
            opened: home == TableHome::Opened,
            keeps_file: self.opened.is_some(),
            error: None,
            shown: None,
            checking: false,
        });
        self.refresh_table_window();
        // Only now the card may close: the window holds the entry (and keeps a chosen file).
        // No fade, so the card is not ordered out after the window comes forward.
        launcher::hide_at_once();
        launcher::bring_table_window_forward();
    }

    /// Close the table window (Wipe, Clear history, lock, its entry gone): its table may forget
    /// its frames again, and the window wipes what it showed.
    pub(crate) fn close_table_window(&mut self) {
        let had = self.table_window.is_some();
        if let Some((_, home)) = self.window_source()
            && let Some(table) = self.target_table(TableTarget::Window, home)
        {
            table.pin(false);
        }
        self.table_window = None;
        if had {
            launcher::close_table_window();
        }
    }

    pub(crate) fn table_window_event(&mut self, event: launcher::TableWindowEvent) {
        match event {
            launcher::TableWindowEvent::Closed => {
                // The window wiped itself; forget the entry.
                if let Some((_, home)) = self.window_source()
                    && let Some(table) = self.target_table(TableTarget::Window, home)
                {
                    table.pin(false);
                }
                let keeps_file = self
                    .table_window
                    .take()
                    .is_some_and(|window| window.keeps_file);
                // A chosen file goes back on the card, with its versions, as a history entry
                // would.
                if card_returns_after_window(keeps_file, self.opened.is_some(), launcher::is_open())
                {
                    let data = self.launch_for_popup();
                    self.reveal_card(data);
                }
            }
            launcher::TableWindowEvent::Run(id) => {
                let target = TableTarget::Window;
                match id {
                    CommandId::TableStep(op) => self.table_step(target, op),
                    CommandId::TableUndo => {
                        self.table_goto(target, |cursor| cursor.checked_sub(1));
                    }
                    CommandId::TableRedo => self.table_goto(target, |cursor| Some(cursor + 1)),
                    CommandId::TableVersion(index) => self.table_goto(target, |_| Some(index)),
                    CommandId::TableDateOrder(month_first) => {
                        self.table_reread(target, |options| {
                            options.month_first = month_first;
                        });
                    }
                    CommandId::TableChooseColumns => {
                        launcher::open_table_window_column_picker();
                        return;
                    }
                    CommandId::Copy => {
                        if let Err(e) = self.copy_table_window() {
                            crate::diag::log(crate::diag::LogEvent::CopyFailed(
                                crate::diag::ErrorKind::of(&e),
                            ));
                        }
                        return;
                    }
                    CommandId::Save => {
                        self.save_table_window();
                        return;
                    }
                    _ => {}
                }
                self.refresh_table_window();
            }
        }
    }

    /// Give the table window the version its entry shows, working its frame out first when it
    /// is not there (not over another table job: the window waits for it to finish). Closes
    /// the window when its entry is gone.
    pub(crate) fn refresh_table_window(&mut self) {
        if self.table_window.is_none() {
            return;
        }
        let Some((text, home)) = self.window_source() else {
            self.close_table_window();
            return;
        };
        let running = self.table_job.as_ref().map(|run| run.generation);
        let (error, shown, checking) = match self.table_window.as_ref() {
            Some(window) => (window.error.clone(), window.shown, window.checking),
            None => return,
        };
        let Some(table) = self.target_table(TableTarget::Window, home) else {
            self.close_table_window();
            return;
        };
        table.pin(true);
        let running_here = running.is_some_and(|generation| table.awaits(generation));
        let load = if running.is_some() {
            None
        } else {
            table.load(&text)
        };
        let working = load.is_some() || running_here;
        let key = TableWindow::key(table, error.as_deref(), working);
        if shown == Some(key) && !checking && load.is_none() {
            return;
        }
        let shown_table = commands::TableShown {
            frame: table.frame().cloned(),
            frame_id: table.frame_id(),
            version: table.cursor(),
            labels: table.labels(),
            working,
            error,
            options: table.options(),
            notes: table.notes(),
            overview: Some(false),
        };
        if let Some(window) = self.table_window.as_mut() {
            window.shown = Some(key);
        }
        // The grid text of a large table takes a while: built on a thread, shown on main.
        let source = text;
        let built = self.start_work(
            work::Lane::TableWindow,
            work::QUICK,
            move || commands::table_window_view(&shown_table, &source),
            move |app, view| app.table_view_built(key, view),
        );
        if let Some(view) = built {
            self.table_view_built(key, view);
        }
        if let Some(job) = load {
            self.run_table_job(job, true, TableTarget::Window);
        }
    }

    /// The table window's view for `key` is built; dropped when the window moved on.
    fn table_view_built(&mut self, key: u64, view: commands::TableWindowView) {
        let Some(window) = self.table_window.as_mut().filter(|w| w.shown == Some(key)) else {
            return;
        };
        window.checking = view.checking;
        launcher::show_table_window(view);
    }

    /// Read the table another way (the date order): it starts over from
    /// the original.
    pub(crate) fn table_reread(
        &mut self,
        target: TableTarget,
        change: impl FnOnce(&mut crate::dataframe::ReadOptions),
    ) {
        let Some((text, home)) = self.target_source(target) else {
            return;
        };
        let Some(table) = self.target_table(target, home) else {
            return;
        };
        let mut options = table.options();
        change(&mut options);
        if let Some(job) = table.reread(options, &text) {
            self.set_table_error(target, None);
            self.run_table_job(job, false, target);
            self.refresh_popup();
        }
    }

    /// A step's error (or none) for `target`'s meta line. A step from the card also brings the
    /// card to the table (the Dataframe view).
    pub(crate) fn set_table_error(&mut self, target: TableTarget, error: Option<String>) {
        match target {
            TableTarget::Card => {
                if error.is_none() {
                    self.card_view = CardView::Dataframe;
                }
                self.table_error = error;
            }
            TableTarget::Window => {
                if let Some(window) = self.table_window.as_mut() {
                    window.error = error;
                }
            }
        }
    }

    /// Run `op` on the version shown; the card shows the new version when it is done.
    pub(crate) fn table_step(&mut self, target: TableTarget, op: TableOp) {
        let Some((text, home)) = self.target_source(target) else {
            return;
        };
        let Some(table) = self.target_table(target, home) else {
            return;
        };
        match table.push(op, &text) {
            Ok(job) => {
                self.set_table_error(target, None);
                self.run_table_job(job, false, target);
            }
            Err(e) => self.set_table_error(target, Some(e.to_string())),
        }
        self.refresh_popup();
    }

    /// Show the version `to` picks from the one shown (undo, redo, a version from the menu).
    pub(crate) fn table_goto(
        &mut self,
        target: TableTarget,
        to: impl FnOnce(usize) -> Option<usize>,
    ) {
        let Some((text, home)) = self.target_source(target) else {
            return;
        };
        let Some(table) = self.target_table(target, home) else {
            return;
        };
        let Some(index) = to(table.cursor()) else {
            return;
        };
        if let Some(job) = table.goto(index, &text) {
            self.set_table_error(target, None);
            self.run_table_job(job, false, target);
            self.refresh_popup();
        }
    }

    /// Start `job` on a thread, cancelling the one running. A `quiet` job shows no spinner.
    pub(crate) fn run_table_job(
        &mut self,
        job: crate::table::Job,
        quiet: bool,
        target: TableTarget,
    ) {
        self.cancel_table_job();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let generation = job.generation;
        let spawned = std::thread::Builder::new()
            .name("copycraft-table".into())
            .spawn(move || {
                let result = job.run(&flag);
                launcher::emit(UserEvent::TableDone(Box::new(launcher::TableDone {
                    generation,
                    result,
                })));
            });
        match spawned {
            Ok(_) => {
                self.table_job = Some(TableRun {
                    started: Background::now(),
                    generation,
                    cancel,
                    quiet,
                    target,
                });
            }
            Err(e) => self.work_failed(crate::diag::Work::TableJob, &e),
        }
    }

    pub(crate) fn cancel_table_job(&mut self) {
        if let Some(run) = self.table_job.take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        self.stop_spinner_when_idle();
    }

    /// Take a finished table job into the table it was made for (if that is still waiting for
    /// it), and show it.
    pub(crate) fn finish_table_job(&mut self, done: launcher::TableDone) {
        let mut target = TableTarget::Card;
        if let Some(run) = self
            .table_job
            .take_if(|run| run.generation == done.generation)
        {
            target = run.target;
            crate::diag::log(crate::diag::LogEvent::TableJobDone {
                ms: run.started.elapsed_ms() as u64,
            });
        }
        self.stop_spinner_when_idle();
        match done.result {
            Ok(finished) => {
                let generation = finished.generation();
                let table = if self.opened_table.awaits(generation) {
                    Some(&mut self.opened_table)
                } else if self.loose_table.awaits(generation) {
                    Some(&mut self.loose_table)
                } else {
                    self.history.table_awaiting(generation)
                };
                // No table waits for it when it was wiped meanwhile: the frame is dropped.
                if let Some(table) = table {
                    table.finish(finished);
                }
            }
            Err(crate::table::TableError::Cancelled) => {
                // A window waiting for another job can work its own table out now.
                self.refresh_table_window();
                return;
            }
            // A step that changes nothing ("Nothing to change"): a meta-line note, no version.
            Err(crate::table::TableError::Unchanged(note)) => {
                self.set_table_note(target, note.to_string());
            }
            Err(e) => self.set_table_note(target, e.to_string()),
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
        self.refresh_table_window();
    }

    /// Why a finished job made no version, on the meta line of the one that asked.
    pub(crate) fn set_table_note(&mut self, target: TableTarget, note: String) {
        match target {
            TableTarget::Card => self.table_error = Some(note),
            TableTarget::Window => {
                if let Some(window) = self.table_window.as_mut() {
                    window.error = Some(note);
                }
            }
        }
    }

    /// The table version for the card: `data` is the card for the text it holds. Starts the job
    /// that works out the version when its frame is not there (only once the card has worked on
    /// the table: steps were taken, or the Dataframe view is open).
    pub(crate) fn attach_table(&mut self, data: &mut LaunchData) {
        let Some(text) = data
            .subject_text
            .as_deref()
            .filter(|_| data.subject_kind == SubjectKind::Text)
        else {
            return;
        };
        let home = pick_table_home(
            self.history.text_index(text, self.history_cursor),
            self.opened.is_some(),
        );
        let dataframe_view = data.view == CardView::Dataframe;
        // The chips are remembered per text, so this is cheap: a table has "Open table".
        if !commands::chips(data)
            .iter()
            .any(|chip| chip.id == CommandId::TableOpenWindow)
        {
            return;
        }
        if home == TableHome::Loose {
            self.adopt_loose(text);
        }
        let text = Zeroizing::new(text.to_string());
        let running = self.table_job.as_ref().map(|run| run.generation);
        let error = self.table_error.clone();
        let Some(table) = self.table_at(home) else {
            return;
        };
        // The frame is worked out for every table on the card (its columns fill the window
        // Table ▾ menu), but not over a job already running for it.
        let running_here = running.is_some_and(|generation| table.awaits(generation));
        let load = if running_here {
            None
        } else {
            table.load(&text)
        };
        let working = load.is_some() || running_here;
        // The first version shown picks the overview or the grid for the entry.
        if let Some(frame) = table.frame() {
            let width = frame.width();
            table.shown_with(width);
        }
        let shown = commands::TableShown {
            frame: table.frame().cloned(),
            frame_id: table.frame_id(),
            version: table.cursor(),
            labels: table.labels(),
            working,
            error,
            options: table.options(),
            notes: table.notes(),
            overview: table.overview_choice(),
        };
        data.table = Some(shown);
        if let Some(job) = load {
            // Outside the Dataframe view the card does not wait for it: no spinner.
            self.run_table_job(job, !dataframe_view, TableTarget::Card);
        }
    }

    /// The table version shown when it is not the original: Copy and Save take it in the
    /// Dataframe view.
    pub(crate) fn shown_table_version(&mut self) -> Option<polars::prelude::DataFrame> {
        let (_, home) = self.table_source()?;
        let table = match home {
            TableHome::History(index) => self.history.table(index)?,
            TableHome::Opened => &self.opened_table,
            TableHome::Loose => &self.loose_table,
        };
        let reread = table.options() != crate::dataframe::ReadOptions::default();
        (table.cursor() > 0 || reread)
            .then(|| table.frame().cloned())
            .flatten()
    }
}
