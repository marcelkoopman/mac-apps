use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use anyhow::Context;
use zeroize::Zeroizing;

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use mac_ui::tray;
use mac_ui::tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
};
use mac_ui::winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
};

use crate::appearance;
use crate::clipboard::{self, ClipboardHistory, ClipboardView, SecretBytes};
use crate::commands::{self, CardView, CommandId, Hist, LaunchData, SubjectKind};
use crate::format;
use crate::icon;
use crate::image_edit::{ImageOp, ImageVersions};
use crate::launcher::{self, UserEvent};
use crate::table::{TableOp, TableVersions};

/// How often the pasteboard is polled while the card is open.
const REFRESH: Duration = Duration::from_millis(400);
/// How often it is polled while the card is closed. A quiet tick reads only the change count.
const IDLE_REFRESH: Duration = Duration::from_secs(1);
/// A copy copycraft wrote that is labelled sensitive is cleared from the pasteboard this long
/// after the write, when that setting is on and nothing else was copied since.
const SENSITIVE_CLEAR_AFTER: Duration = Duration::from_secs(60);
/// Background work that finishes sooner shows no spinner, so small files do not flicker.
const SPINNER_DELAY: Duration = Duration::from_millis(180);
/// Copies at least this long are classified on a background thread right after the copy, so the
/// card opens without scanning them first.
const PREWARM_LEN: usize = 64 * 1024;
/// Card title for text dropped on it (a dropped file shows its name).
const DROPPED_TEXT: &str = "Dropped text";
/// Card name for image data dropped on it (a picture from Safari or Preview).
const DROPPED_IMAGE: &str = "Dropped image";

struct App {
    tray: TrayIcon,
    history: ClipboardHistory,
    /// Index into history while the arrows are browsing. 0 is the newest.
    history_cursor: usize,
    /// While a drop is on the card (at history index 0): the clipboard's own history index,
    /// for when the card follows the clipboard again.
    clipboard_cursor: Option<usize>,
    current_image: Option<SecretBytes>,
    recorded_image_change: Option<isize>,
    skip_image_change: Option<isize>,
    skip_record: Option<Zeroizing<String>>,
    /// Result the card is showing. A new clipboard copy returns this to original.
    card_view: CardView,
    /// File on the card. Absent means the card follows the clipboard.
    opened: Option<crate::open_file::OpenedFile>,
    image_scan: Option<commands::ImageScan>,
    /// Pasteboard change the stored scan belongs to.
    image_scan_change: Option<isize>,
    /// Pasteboard change a scan is already running for.
    image_scan_for: Option<isize>,
    signature: ClipSig,
    /// The save running on its thread. One at a time; Save is ignored meanwhile.
    saving: Option<Background>,
    /// "Show all" rendering on its thread. One at a time.
    loading_all: Option<Background>,
    /// The whole-text card "Show all" built, for the item and view it belongs to.
    full_card: Option<Box<launcher::FullCard>>,
    /// The card spinner is showing (some background work ran past [`SPINNER_DELAY`]).
    spinner_on: bool,
    /// The short blink of the menu bar icon after a copy, and its two template glyphs.
    blink: tray::Blink,
    /// The glyph and its blink frame, both templates, built once.
    icons: tray::Glyphs,
    hotkeys: GlobalHotKeyManager,
    /// The currently registered chord (for unregister on change).
    format_hotkey: global_hotkey::hotkey::HotKey,
    format_hotkey_id: u32,
    /// The sensitive copy copycraft wrote last, to clear when its minute is up.
    sensitive_clear: Option<SensitiveClear>,
    /// Change count of copycraft's own write whose labels were still being checked.
    conceal_when_checked: Option<isize>,
    /// The pasteboard change count the last poll looked at.
    polled_change: Option<isize>,
    /// The last poll found nothing or no text: look again even without a new change count.
    poll_again: bool,
    /// The card shows the one-time explanation of the pasteboard privacy alert, before the
    /// copy is read (Default or Ask; see [`crate::paste_access::ASK_NOTE`]).
    paste_explaining: bool,
    /// That explanation was shown once (stored in the user defaults).
    paste_explained: bool,
    /// The history picture the pasteboard holds at that change count (a copy recorded, or an
    /// entry put back), so its scan can be kept with the entry and found there again.
    image_on_pasteboard: Option<(isize, SecretBytes)>,
    /// When history last got a new entry (copy, drop, copycraft's own write).
    last_copy: Option<Instant>,
    /// [`crate::settings::Settings::history_minutes`], read once and kept in step with the menu.
    history_minutes: u32,
    /// The table versions of a chosen file, which is not in history (a history entry keeps its
    /// own). Dropped with the file.
    opened_table: TableVersions,
    /// The table job on its thread. One at a time: a new one cancels it.
    table_job: Option<TableRun>,
    /// Why the last table step failed, shown on the card until the next one.
    table_error: Option<String>,
    /// The entry the table window shows, while it is open.
    table_window: Option<TableWindow>,
    /// The picture versions of a chosen file, which is not in history (a history entry,
    /// a dropped picture's too, keeps its own). Dropped with the file.
    opened_image: ImageVersions,
    /// The Image ▾ job on its thread. One at a time: a new one cancels it.
    image_job: Option<ImageRun>,
    /// Why the last Image ▾ step did nothing or failed, and on which picture (its allocation),
    /// shown on that picture's card until the next step.
    image_note: Option<(usize, String)>,
}

/// An Image ▾ job running on its thread ([`crate::image_edit::Job`]).
struct ImageRun {
    started: Background,
    generation: u64,
    cancel: Arc<AtomicBool>,
}

/// A table job running on its thread ([`crate::table::Job`]).
struct TableRun {
    started: Background,
    generation: u64,
    cancel: Arc<AtomicBool>,
    /// Loading a frame the card does not show yet: no spinner.
    quiet: bool,
    /// Who asked for it: its error goes to that one's meta line.
    target: TableTarget,
}

/// Where a table command comes from and goes to: the card's table, or the table window's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableTarget {
    Card,
    Window,
}

/// The entry the table window shows: its copied text (found again in history by it, so the
/// window stays with its entry while the card moves on), or the chosen file on the card.
struct TableWindow {
    text: Zeroizing<String>,
    opened: bool,
    /// Why the window's last step failed, for its meta line.
    error: Option<String>,
    /// What the window was last given ([`TableWindow::key`]); the view is built again only
    /// when it changes, or while its labels are still being checked.
    shown: Option<u64>,
    checking: bool,
}

impl TableWindow {
    fn key(table: &TableVersions, error: Option<&str>, working: bool) -> u64 {
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
enum TableHome {
    History(usize),
    Opened,
}

/// A pasteboard write to clear at `due`, if the pasteboard still holds it (`change`).
#[derive(Debug, Clone, Copy)]
struct SensitiveClear {
    change: isize,
    due: Instant,
}

/// Work running on a background thread: a save, or the "Show all" card.
#[derive(Debug, Clone, Copy)]
struct Background {
    started: Instant,
}

impl Background {
    fn now() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    fn elapsed_ms(self) -> u128 {
        self.started.elapsed().as_millis()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClipSig {
    text_hash: u64,
    image: bool,
    image_change: isize,
    history_len: usize,
}

impl ClipSig {
    /// Whether going from `self` to `next` is a new copy worth a blink: the pasteboard changed
    /// (text, image or change count), it is not the first look after launch or a clear (default
    /// signature), and it holds something. History edits alone do not count.
    fn is_new_copy(&self, next: &Self, empty: bool) -> bool {
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

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: mac_ui::winit::window::WindowId,
        _: WindowEvent,
    ) {
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Run(id) => self.run_command(event_loop, id),
            UserEvent::ImageScanned { change, scan } => self.finish_image_scan(change, scan),
            UserEvent::SaveFinished(result) => self.finish_save(result),
            UserEvent::FullCardReady(card) => self.finish_show_all(card),
            UserEvent::DroppedFile(path) => {
                self.show_dropped(crate::open_file::load_dropped(&path));
            }
            UserEvent::DroppedPromisedFile(path) => {
                let opened = crate::open_file::load_dropped(&path);
                mac_ui::drop::discard_promised(&path);
                self.show_dropped(opened);
            }
            UserEvent::DroppedImage(bytes) => {
                self.show_dropped(crate::open_file::from_image_data(DROPPED_IMAGE, bytes));
            }
            UserEvent::DroppedText(text) => {
                self.show_dropped(crate::open_file::from_text(DROPPED_TEXT, text));
            }
            UserEvent::DroppedImageScanned { image, scan } => {
                self.finish_dropped_scan(&image, scan);
            }
            UserEvent::LabelsChecked => self.labels_checked(),
            UserEvent::SessionEnded => self.session_ended(),
            UserEvent::TableDone(done) => self.finish_table_job(*done),
            UserEvent::TableWindow(event) => self.table_window_event(event),
            UserEvent::SettingsChanged => {
                if launcher::is_open() {
                    self.refresh_popup();
                }
            }
            UserEvent::HotkeyChanged => self.rebind_hotkey(),
            UserEvent::ImageDone(done) => self.finish_image_job(*done),
            UserEvent::VersionScanned { picture, scan } => {
                self.finish_version_scan(&picture, scan);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A closed card forgets the file, so the next hotkey shows the clipboard.
        if self.opened.is_some() && !launcher::is_open() {
            self.leave_opened();
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                tray::QUIT_ID => {
                    event_loop.exit();
                    return;
                }
                ALLOW_SCREENSHOTS_ID => {
                    let on = !launcher::allows_screenshots();
                    launcher::set_allow_screenshots(on);
                    eprintln!(
                        "copycraft: allow screenshots {}",
                        if on { "on" } else { "off" }
                    );
                    self.refresh_status_menu();
                }
                SETTINGS_ID => {
                    self.run_command(event_loop, CommandId::Settings);
                }
                ABOUT_ID => {
                    self.run_command(event_loop, CommandId::About);
                }
                id => {
                    if let Some(index) = id
                        .strip_prefix("hist-")
                        .and_then(|value| value.parse::<usize>().ok())
                    {
                        self.run_command(event_loop, CommandId::History(index));
                    }
                }
            }
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.format_hotkey_id && event.state == HotKeyState::Pressed {
                self.summon_popup();
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                self.toggle_popup_under_icon();
            }
        }

        let now = Instant::now();
        let changed = self.note_clipboard(now);
        if changed && launcher::is_open() {
            let data = self.current_launch_data();
            self.sync_popup(data);
        }
        if changed {
            // A new copy can push the window's entry off the end of history.
            self.refresh_table_window();
        }
        let wake = self.spin_slow_work(now);
        let blink_wake = self.show_blink(now);
        let clear_wake = self.clear_sensitive_when_due(now);
        let forget_wake = self.forget_history_when_due(now);
        event_loop.set_control_flow(mac_ui::wake::control_flow([
            Some(wake),
            blink_wake,
            clear_wake,
            forget_wake,
        ]));
    }
}

impl App {
    fn run_command(&mut self, event_loop: &ActiveEventLoop, id: CommandId) {
        match id {
            CommandId::Visit => self.visit_current(),
            CommandId::Copy => {
                if let Err(e) = self.copy_current() {
                    eprintln!("copy failed: {e:#}");
                }
            }
            CommandId::Save => self.save_current(),
            CommandId::ShowAll => self.show_all(),
            CommandId::Original
            | CommandId::Format
            | CommandId::Convert
            | CommandId::Decode
            | CommandId::Dataframe
            | CommandId::Schema
            | CommandId::Sample
            | CommandId::Info
            | CommandId::Ocr
            | CommandId::Qr => {
                if matches!(id, CommandId::Format) && self.format_link_in_place() {
                    return;
                }
                self.card_view = CardView::from_command(&id).unwrap_or_default();
                self.refresh_popup();
            }
            CommandId::History(index) => {
                if let Err(e) = self.restore_history(index) {
                    eprintln!("restore history failed: {e:#}");
                }
            }
            CommandId::HistoryOlder => self.step_history(true),
            CommandId::HistoryNewer => self.step_history(false),
            CommandId::ClearClipboard => {
                if let Err(e) = self.clear_clipboard() {
                    eprintln!("clear clipboard failed: {e:#}");
                }
            }
            CommandId::ClearHistory => self.clear_history(),
            CommandId::Clear => self.clear_secrets(),
            CommandId::ChooseFile => self.choose_file(),
            CommandId::UseClipboard => {
                self.leave_opened();
                self.card_view = CardView::Original;
                self.image_scan = None;
                self.image_scan_change = None;
                self.refresh_popup();
            }
            CommandId::Appearance(theme) => {
                appearance::save(theme);
                self.refresh_popup();
            }
            CommandId::ClearSensitive => {
                let on = !crate::settings::load().clear_sensitive;
                crate::settings::set_clear_sensitive(on);
                if !on {
                    self.sensitive_clear = None;
                }
                self.refresh_popup();
            }
            CommandId::KeepHistory(minutes) => {
                crate::settings::set_history_minutes(minutes);
                self.history_minutes = minutes;
                self.refresh_popup();
            }
            CommandId::TableStep(op) => self.table_step(TableTarget::Card, op),
            CommandId::TableOpenWindow => self.open_table_window(),
            // On a picture card, undo, redo and the version capsule are the picture's.
            CommandId::TableUndo if self.shows_image() => {
                self.image_goto(|cursor| cursor.checked_sub(1));
            }
            CommandId::TableRedo if self.shows_image() => {
                self.image_goto(|cursor| Some(cursor + 1))
            }
            CommandId::TableVersion(index) if self.shows_image() => {
                self.image_goto(|_| Some(index))
            }
            CommandId::TableUndo => {
                self.table_goto(TableTarget::Card, |cursor| cursor.checked_sub(1));
            }
            CommandId::TableRedo => self.table_goto(TableTarget::Card, |cursor| Some(cursor + 1)),
            CommandId::TableVersion(index) => self.table_goto(TableTarget::Card, |_| Some(index)),
            CommandId::ImageStep(op) => self.image_step(op),
            CommandId::ImageResizeCustom => self.custom_resize(),
            // The card pops the menu itself.
            CommandId::TableMenu | CommandId::ImageMenu | CommandId::TableChooseColumns => {}
            CommandId::TableDateOrder(month_first) => {
                self.table_reread(TableTarget::Card, |options| {
                    options.month_first = month_first;
                });
            }
            CommandId::Settings => launcher::show_settings(),
            CommandId::About => launcher::show_about(),
            CommandId::Quit => event_loop.exit(),
        }
    }

    /// Unregister the old chord and register the one stored in Settings.
    fn rebind_hotkey(&mut self) {
        let next = crate::settings::hotkey();
        if next.id() == self.format_hotkey_id {
            return;
        }
        if let Err(e) = self.hotkeys.unregister(self.format_hotkey) {
            eprintln!("copycraft: unregister hotkey: {e}");
        }
        match self.hotkeys.register(next) {
            Ok(()) => {
                self.format_hotkey = next;
                self.format_hotkey_id = next.id();
                self.refresh_status_menu();
                eprintln!("copycraft: hotkey {}", crate::settings::hotkey_label());
            }
            Err(e) => {
                eprintln!("copycraft: register hotkey: {e}");
                // Best effort: put the previous chord back.
                let _ = self.hotkeys.register(self.format_hotkey);
            }
        }
    }

    /// The text on the card and where its table versions live, when it is a table.
    fn table_source(&self) -> Option<(Zeroizing<String>, TableHome)> {
        let text = Zeroizing::new(self.source_text()?);
        if !crate::toolbar_visibility::shows_dataframe_button(format::detect(&text), &text) {
            return None;
        }
        let home = match self.history.text_index(&text, self.history_cursor) {
            Some(index) => TableHome::History(index),
            None if self.opened.is_some() => TableHome::Opened,
            None => return None,
        };
        Some((text, home))
    }

    fn table_at(&mut self, home: TableHome) -> Option<&mut TableVersions> {
        match home {
            TableHome::History(index) => self.history.table_mut(index),
            TableHome::Opened => Some(&mut self.opened_table),
        }
    }

    /// The text and table of `target`: the card's ([`table_source`](Self::table_source)) or
    /// the table window's ([`window_source`](Self::window_source)).
    fn target_source(&self, target: TableTarget) -> Option<(Zeroizing<String>, TableHome)> {
        match target {
            TableTarget::Card => self.table_source(),
            TableTarget::Window => self.window_source(),
        }
    }

    /// The versions of `target`'s table. The window's leaves the other entries' frames alone.
    fn target_table(&mut self, target: TableTarget, home: TableHome) -> Option<&mut TableVersions> {
        match (target, home) {
            (TableTarget::Window, TableHome::History(index)) => {
                self.history.table_mut_in_place(index)
            }
            _ => self.table_at(home),
        }
    }

    /// The entry the table window shows, while it is still there (in history, or the chosen
    /// file still on the card).
    fn window_source(&self) -> Option<(Zeroizing<String>, TableHome)> {
        let window = self.table_window.as_ref()?;
        let text = Zeroizing::new(window.text.to_string());
        let home = if window.opened {
            let opened = Zeroizing::new(self.opened_text()?);
            (opened.as_str() == text.as_str()).then_some(TableHome::Opened)?
        } else {
            TableHome::History(self.history.text_index(&text, self.history_cursor)?)
        };
        Some((text, home))
    }

    /// "Open in window": the table on the card in the table window, which then stays with
    /// that entry.
    fn open_table_window(&mut self) {
        let Some((text, home)) = self.table_source() else {
            return;
        };
        self.close_table_window();
        launcher::close_settings();
        launcher::close_about();
        if let Some(table) = self.target_table(TableTarget::Window, home) {
            table.pin(true);
        }
        self.table_window = Some(TableWindow {
            text,
            opened: home == TableHome::Opened,
            error: None,
            shown: None,
            checking: false,
        });
        self.refresh_table_window();
    }

    /// Close the table window (Wipe, Clear history, lock, its entry gone): its table may forget
    /// its frames again, and the window wipes what it showed.
    fn close_table_window(&mut self) {
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

    fn table_window_event(&mut self, event: launcher::TableWindowEvent) {
        match event {
            launcher::TableWindowEvent::Closed => {
                // The window wiped itself; forget the entry.
                if let Some((_, home)) = self.window_source()
                    && let Some(table) = self.target_table(TableTarget::Window, home)
                {
                    table.pin(false);
                }
                self.table_window = None;
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
                    _ => {}
                }
                self.refresh_table_window();
            }
        }
    }

    /// Give the table window the version its entry shows, working its frame out first when it
    /// is not there (not over another table job: the window waits for it to finish). Closes
    /// the window when its entry is gone.
    fn refresh_table_window(&mut self) {
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
        let view = commands::table_window_view(&shown_table, &text);
        if let Some(window) = self.table_window.as_mut() {
            window.shown = Some(key);
            window.checking = view.checking;
        }
        launcher::show_table_window(view);
        if let Some(job) = load {
            self.run_table_job(job, true, TableTarget::Window);
        }
    }

    /// Read the table another way (the date order): it starts over from
    /// the original.
    fn table_reread(
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
    fn set_table_error(&mut self, target: TableTarget, error: Option<String>) {
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
    fn table_step(&mut self, target: TableTarget, op: TableOp) {
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
    fn table_goto(&mut self, target: TableTarget, to: impl FnOnce(usize) -> Option<usize>) {
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
    fn run_table_job(&mut self, job: crate::table::Job, quiet: bool, target: TableTarget) {
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
            Err(e) => eprintln!("table job failed to start: {e}"),
        }
    }

    fn cancel_table_job(&mut self) {
        if let Some(run) = self.table_job.take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        self.stop_spinner_when_idle();
    }

    /// Take a finished table job into the table it was made for (if that is still waiting for
    /// it), and show it.
    fn finish_table_job(&mut self, done: launcher::TableDone) {
        let mut target = TableTarget::Card;
        if let Some(run) = self
            .table_job
            .take_if(|run| run.generation == done.generation)
        {
            target = run.target;
            eprintln!(
                "copycraft: table job done in {} ms",
                run.started.elapsed_ms()
            );
        }
        self.stop_spinner_when_idle();
        match done.result {
            Ok(finished) => {
                let generation = finished.generation();
                let table = if self.opened_table.awaits(generation) {
                    Some(&mut self.opened_table)
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
    fn set_table_note(&mut self, target: TableTarget, note: String) {
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
    fn attach_table(&mut self, data: &mut LaunchData) {
        let Some(text) = data
            .subject_text
            .as_deref()
            .filter(|_| data.subject_kind == SubjectKind::Text)
        else {
            return;
        };
        let home = match self.history.text_index(text, self.history_cursor) {
            Some(index) => TableHome::History(index),
            None if self.opened.is_some() => TableHome::Opened,
            None => return,
        };
        let dataframe_view = data.view == CardView::Dataframe;
        // The chips are remembered per text, so this is cheap: a table has "Table ▾".
        if !commands::chips(data)
            .iter()
            .any(|chip| chip.id == CommandId::TableMenu)
        {
            return;
        }
        let text = Zeroizing::new(text.to_string());
        let running = self.table_job.as_ref().map(|run| run.generation);
        let error = self.table_error.clone();
        let Some(table) = self.table_at(home) else {
            return;
        };
        // The frame is worked out for every table on the card (its columns fill the Table ▾
        // menu), but not over a job already running for it.
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
    fn shown_table_version(&mut self) -> Option<polars::prelude::DataFrame> {
        let (_, home) = self.table_source()?;
        let table = match home {
            TableHome::History(index) => self.history.table(index)?,
            TableHome::Opened => &self.opened_table,
        };
        let reread = table.options() != crate::dataframe::ReadOptions::default();
        (table.cursor() > 0 || reread)
            .then(|| table.frame().cloned())
            .flatten()
    }

    /// The card shows a picture (copied or dropped).
    fn shows_image(&self) -> bool {
        match &self.opened {
            Some(opened) => opened.image.is_some(),
            None => ClipboardView::from_os().is_image(),
        }
    }

    /// The picture on the card, as it came (its versions are worked out from it): the dropped
    /// or chosen picture, or the clipboard's when history holds it.
    fn image_source(&self) -> Option<SecretBytes> {
        if let Some(opened) = &self.opened {
            return opened.image.as_ref().map(|image| image.bytes.clone());
        }
        #[cfg(target_os = "macos")]
        {
            if !ClipboardView::from_os().is_image() {
                return None;
            }
            self.history_image_on_pasteboard(crate::macos_pasteboard::change_count())
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// The versions of the picture `bytes`: its history entry's, else the chosen file's.
    fn image_versions(&mut self, bytes: &SecretBytes) -> Option<&mut ImageVersions> {
        if self.history.holds_image(bytes) {
            return self.history.image_versions_mut(bytes);
        }
        let opened = self
            .opened
            .as_ref()
            .and_then(|file| file.image.as_ref())
            .is_some_and(|image| image.bytes.same_allocation(bytes));
        opened.then_some(&mut self.opened_image)
    }

    /// The versions whose newest job is `generation`.
    fn image_awaiting(&mut self, generation: u64) -> Option<&mut ImageVersions> {
        if self.opened_image.awaits(generation) {
            return Some(&mut self.opened_image);
        }
        self.history.image_awaiting(generation)
    }

    /// Run `op` on the version shown; the card shows the new version when it is done.
    fn image_step(&mut self, op: ImageOp) {
        let Some(bytes) = self.image_source() else {
            return;
        };
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        match versions.push(op, &bytes) {
            Ok(job) => {
                self.image_note = None;
                self.card_view = CardView::Original;
                self.run_image_job(job);
            }
            Err(e) => self.image_note = Some((bytes.allocation_id(), e.to_string())),
        }
        self.refresh_popup();
    }

    /// Resize › Custom…: ask for a width or a height, then resize to it.
    fn custom_resize(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mtm) = mac_ui::objc2::MainThreadMarker::new() else {
                return;
            };
            let mut message = "A width (1200 or w 1200) or a height (h 800), in pixels. \
                               The picture keeps its proportions and never gets larger."
                .to_string();
            loop {
                let answer = mac_ui::dialog::prompt_text(mtm, "Resize to", &message, "");
                launcher::order_front();
                let Some(answer) = answer.map(Zeroizing::new) else {
                    return;
                };
                if answer.trim().is_empty() {
                    return;
                }
                match crate::image_edit::parse_custom_resize(&answer) {
                    Some(to) => {
                        self.image_step(ImageOp::Resize(to));
                        return;
                    }
                    None => {
                        message = "That is not a size. Type a width like 1200 or w 1200, or a \
                                   height like h 800."
                            .to_string();
                    }
                }
            }
        }
    }

    /// Show the version `to` picks from the one shown (undo, redo, a version from the menu).
    fn image_goto(&mut self, to: impl FnOnce(usize) -> Option<usize>) {
        let Some(bytes) = self.image_source() else {
            return;
        };
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        let Some(index) = to(versions.cursor()) else {
            return;
        };
        let job = versions.goto(index, &bytes);
        self.image_note = None;
        self.card_view = CardView::Original;
        if let Some(job) = job {
            self.run_image_job(job);
        } else {
            // Shown at once (the original, or a version kept): a job still running is stale.
            self.cancel_image_job();
        }
        self.refresh_popup();
    }

    /// Start `job` on a thread, cancelling the one running. The version is scanned (info,
    /// text, barcodes) after it is shown ([`UserEvent::VersionScanned`]).
    fn run_image_job(&mut self, job: crate::image_edit::Job) {
        self.cancel_image_job();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let generation = job.generation;
        let spawned = std::thread::Builder::new()
            .name("copycraft-image".into())
            .spawn(move || {
                #[cfg(target_os = "macos")]
                let result = job.run(&flag, &crate::macos_image_edit::ImageIoCodec);
                #[cfg(not(target_os = "macos"))]
                let result: Result<crate::image_edit::JobDone, _> = {
                    drop((job, &flag));
                    Err(crate::image_edit::ImageError::Failed(
                        "Needs macOS".to_string(),
                    ))
                };
                let picture = result
                    .as_ref()
                    .ok()
                    .map(crate::image_edit::JobDone::picture);
                launcher::emit(UserEvent::ImageDone(Box::new(launcher::ImageDone {
                    generation,
                    result,
                })));
                #[cfg(target_os = "macos")]
                if let Some(picture) = picture
                    && !flag.load(Ordering::Relaxed)
                {
                    let bytes = Zeroizing::new(picture.with(<[u8]>::to_vec));
                    let scan = crate::macos_pasteboard::scan_image_bytes(&bytes);
                    launcher::emit(UserEvent::VersionScanned { picture, scan });
                }
                #[cfg(not(target_os = "macos"))]
                let _ = picture;
            });
        match spawned {
            Ok(_) => {
                self.image_job = Some(ImageRun {
                    started: Background::now(),
                    generation,
                    cancel,
                });
            }
            Err(e) => eprintln!("image job failed to start: {e}"),
        }
    }

    fn cancel_image_job(&mut self) {
        if let Some(run) = self.image_job.take() {
            run.cancel.store(true, Ordering::Relaxed);
        }
        self.stop_spinner_when_idle();
    }

    /// Take a finished Image ▾ job into the versions it was made for (if they still wait for
    /// it), and show it.
    fn finish_image_job(&mut self, done: launcher::ImageDone) {
        if let Some(run) = self
            .image_job
            .take_if(|run| run.generation == done.generation)
        {
            eprintln!(
                "copycraft: image job done in {} ms",
                run.started.elapsed_ms()
            );
        }
        self.stop_spinner_when_idle();
        match done.result {
            Ok(finished) => {
                if let Some(versions) = self.image_awaiting(finished.generation()) {
                    versions.finish(finished);
                }
            }
            Err(crate::image_edit::ImageError::Cancelled) => return,
            // A step that changes nothing ("Already that size or smaller"): a note, no version.
            Err(e) => {
                if self.image_awaiting(done.generation).is_none() {
                    return;
                }
                if let Some(bytes) = self.image_source() {
                    self.image_note = Some((bytes.allocation_id(), e.to_string()));
                }
            }
        }
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Keep a version's scan with it, and show it when the card shows that version.
    fn finish_version_scan(&mut self, picture: &SecretBytes, scan: Option<commands::ImageScan>) {
        let Some(scan) = scan else {
            return;
        };
        let kept = self.opened_image.remember_scan(picture, &scan)
            || self.history.remember_version_scan(picture, &scan);
        if kept && launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The picture versions for the card: `data` is the card for the picture they are of. A
    /// step's version shown replaces the picture, its facts and its scan; one not worked out
    /// (after Wipe of the far entries' pictures) is worked out again.
    fn attach_image(&mut self, data: &mut LaunchData) {
        if data.subject_kind != SubjectKind::Image {
            return;
        }
        let Some(bytes) = self.image_source() else {
            return;
        };
        let running = self.image_job.as_ref().map(|run| run.generation);
        let note = self
            .image_note
            .as_ref()
            .filter(|(entry, _)| *entry == bytes.allocation_id())
            .map(|(_, note)| note.clone());
        let Some(versions) = self.image_versions(&bytes) else {
            return;
        };
        let running_here = running.is_some_and(|generation| versions.awaits(generation));
        let load = if running_here {
            None
        } else {
            versions.load(&bytes)
        };
        let working = load.is_some() || running_here;
        data.image_edit = Some(commands::ImageShown {
            entry: bytes.allocation_id() as u64,
            labels: versions.labels(),
            version: versions.cursor(),
            working,
            note,
        });
        if let Some(shown) = versions.shown() {
            data.picture = Some(shown.picture.clone());
            data.image = Some(shown.facts.clone());
            data.image_scan = shown.scan.clone();
        }
        if let Some(job) = load {
            self.run_image_job(job);
        }
    }

    /// The version of the picture on the card when it is a step's and worked out.
    fn shown_image_version(&mut self) -> Option<crate::image_edit::VersionPicture> {
        let bytes = self.image_source()?;
        self.image_versions(&bytes)?.shown().cloned()
    }

    fn summon_popup(&mut self) {
        if !launcher::is_open() {
            self.card_view = CardView::Original;
            self.leave_opened();
            self.full_card = None;
        }
        launcher::summon(self.launch_for_popup());
    }

    fn toggle_popup_under_icon(&mut self) {
        if !launcher::is_open() {
            self.card_view = CardView::Original;
            self.leave_opened();
            self.full_card = None;
        }
        launcher::toggle_under_icon(self.launch_for_popup(), &self.tray);
    }

    fn refresh_popup(&mut self) {
        self.refresh_table_window();
        if !launcher::is_open() {
            return;
        }
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::set_card_shown(true);
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        let data = self.current_launch_data();
        self.sync_popup(data);
    }

    /// Update the open card. Keeps the "Show all" card while it belongs to this item and view;
    /// anything else drops it, so the card is back to its preview.
    fn sync_popup(&mut self, mut data: LaunchData) {
        let key = commands::content_key(&data);
        let full = self
            .full_card
            .as_ref()
            .filter(|full| full.key == key && full.view == data.view)
            .map(|full| full.card.clone());
        match full {
            Some(card) => {
                data.full = true;
                launcher::sync_with_card(data, card);
            }
            None => {
                self.full_card = None;
                let card = self.history_card(&data);
                launcher::sync_with_card(data, card);
            }
        }
    }

    /// The card for `data`. A copied text's preview card is built once per history entry and
    /// view, and kept with that entry (dropped and zeroized with it), so stepping back and
    /// forth through history does not detect, format and lay out the same copies again.
    fn history_card(&mut self, data: &LaunchData) -> commands::WorkCard {
        let entry = data
            .subject_text
            .as_deref()
            .filter(|_| {
                self.opened.is_none()
                    && !data.full
                    && data.subject_kind == SubjectKind::Text
                    && data.source_name.is_none()
            })
            .and_then(|text| self.history.text_index(text, self.history_cursor));
        // A table version's card changes with the version, its job and its errors.
        let versioned = data.view == CardView::Dataframe
            && data
                .table
                .as_ref()
                .is_some_and(|table| table.version > 0 || table.working || table.error.is_some());
        let Some(index) = entry.filter(|_| !versioned) else {
            return commands::work_card(data);
        };
        if let Some(card) = self.history.card(index, data.view) {
            return card.clone();
        }
        let card = commands::work_card(data);
        if !commands::is_checking(&card) {
            self.history.remember_card(index, data.view, card.clone());
        }
        card
    }

    /// Build the whole-text card on a background thread; the spinner shows if it is slow.
    fn show_all(&mut self) {
        if self.loading_all.is_some() || !launcher::is_open() {
            return;
        }
        let mut data = self.current_launch_data();
        let key = commands::content_key(&data);
        let view = data.view;
        if self
            .full_card
            .as_ref()
            .is_some_and(|full| full.key == key && full.view == view)
        {
            return;
        }
        data.full = true;
        let spawned = std::thread::Builder::new()
            .name("copycraft-show-all".into())
            .spawn(move || {
                let card = commands::work_card(&data);
                launcher::emit(UserEvent::FullCardReady(Box::new(launcher::FullCard {
                    key,
                    view,
                    card,
                })));
            });
        match spawned {
            Ok(_) => self.loading_all = Some(Background::now()),
            Err(e) => eprintln!("show all failed: {e}"),
        }
    }

    fn finish_show_all(&mut self, full: Box<launcher::FullCard>) {
        if let Some(run) = self.loading_all.take() {
            eprintln!("copycraft: show all built in {} ms", run.elapsed_ms());
        }
        self.stop_spinner_when_idle();
        if !launcher::is_open() {
            return;
        }
        let data = self.current_launch_data();
        // The card moved on (another copy or view) while the thread ran.
        if commands::content_key(&data) != full.key || data.view != full.view {
            return;
        }
        self.full_card = Some(full);
        self.sync_popup(data);
    }

    /// Clipboard launch, unless the card is holding a chosen file.
    fn launch_for_popup(&mut self) -> LaunchData {
        #[cfg(target_os = "macos")]
        self.card_opening();
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        self.current_launch_data()
    }

    /// The card is about to show: with Default or Ask the copy is read now, in one pass (one
    /// alert). The first time, the card explains that alert instead and the copy is read on the
    /// next tick, with the explanation on screen.
    #[cfg(target_os = "macos")]
    fn card_opening(&mut self) {
        crate::macos_pasteboard::set_card_shown(false);
        let waiting = self.opened.is_none()
            && crate::paste_access::explain_first(
                crate::macos_pasteboard::read_strategy(),
                self.paste_explained,
            )
            && matches!(ClipboardView::from_os(), ClipboardView::Pending);
        if waiting {
            self.paste_explaining = true;
            self.paste_explained = true;
            crate::settings::set_paste_alert_explained();
        } else {
            crate::macos_pasteboard::set_card_shown(true);
        }
    }

    fn current_launch_data(&mut self) -> LaunchData {
        let mut data = if self.opened.is_some() {
            self.launch_from_opened()
        } else {
            let view = ClipboardView::from_os();
            self.launch_data(&view)
        };
        self.attach_table(&mut data);
        self.attach_image(&mut data);
        data
    }

    fn choose_file(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if let Some(path) = crate::macos_open::choose_path() {
                self.show_source(crate::open_file::load(&path));
            }
            launcher::order_front();
        }
    }

    /// A dropped file, text or picture: into history like a copy (newest first, the same limit,
    /// zeroized when it falls off or on Wipe), then onto the card. The clipboard is left as it is.
    fn show_dropped(&mut self, opened: crate::open_file::OpenedFile) {
        let opened = self.look_at_dropped_image(opened);
        let base = self.clipboard_cursor.unwrap_or(self.history_cursor);
        let clipboard_at = if let Some(image) = opened.image.as_ref() {
            Some(
                self.history
                    .record_image_tracking(image.bytes.clone(), base),
            )
        } else if let Some(text) = opened.text.as_deref() {
            self.history.record_tracking(text.to_string(), base)
        } else {
            None
        };
        if let Some(clipboard_at) = clipboard_at {
            self.last_copy = Some(Instant::now());
            // The clipboard is not recorded again (back to the front) until it changes. A
            // copied picture is recorded once per pasteboard change anyway.
            if let Some(current) = ClipboardView::from_os().text() {
                self.skip_record = Some(Zeroizing::new(current.to_string()));
            }
            self.clipboard_cursor = Some(clipboard_at);
            self.history_cursor = 0;
            self.refresh_status_menu();
        }
        if let Some(image) = opened.image.as_ref() {
            self.scan_dropped_image(image.bytes.clone());
        }
        self.show_source(opened);
        launcher::order_front();
    }

    /// The format and size of a dropped picture, which the image card shows. One the system
    /// cannot read gets a note instead, and stays out of history.
    fn look_at_dropped_image(
        &self,
        mut opened: crate::open_file::OpenedFile,
    ) -> crate::open_file::OpenedFile {
        let Some(image) = opened.image.as_mut() else {
            return opened;
        };
        #[cfg(target_os = "macos")]
        let facts = image.bytes.with(crate::macos_pasteboard::image_bytes_facts);
        #[cfg(not(target_os = "macos"))]
        let facts = None;
        if facts.is_none() {
            return crate::open_file::noted(
                std::mem::take(&mut opened.name),
                crate::open_file::UNREADABLE_IMAGE,
            );
        }
        image.facts = facts;
        opened
    }

    /// Info text, data URL, text and barcodes of a dropped picture, on a background thread
    /// like a copied one's ([`UserEvent::DroppedImageScanned`]).
    fn scan_dropped_image(&self, image: SecretBytes) {
        #[cfg(target_os = "macos")]
        {
            let spawned = std::thread::Builder::new()
                .name("copycraft-drop-scan".into())
                .spawn(move || {
                    // Copied out first: the scan (text recognition) takes a while, and the card
                    // reads the same bytes to draw the picture.
                    let bytes = Zeroizing::new(image.with(<[u8]>::to_vec));
                    let scan = crate::macos_pasteboard::scan_image_bytes(&bytes);
                    launcher::emit(UserEvent::DroppedImageScanned { image, scan });
                });
            if let Err(e) = spawned {
                eprintln!("dropped image scan failed: {e}");
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = image;
    }

    fn finish_dropped_scan(&mut self, image: &SecretBytes, scan: Option<commands::ImageScan>) {
        // The card moved on (the clipboard, another drop, Wipe) while the thread ran.
        let Some(shown) = self
            .opened
            .as_mut()
            .and_then(|file| file.image.as_mut())
            .filter(|shown| shown.bytes.same_allocation(image))
        else {
            return;
        };
        shown.scan = scan;
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The card follows the clipboard again. After a drop, the history position goes back to
    /// the clipboard's entry.
    fn leave_opened(&mut self) {
        if self
            .table_window
            .as_ref()
            .is_some_and(|window| window.opened)
        {
            self.close_table_window();
        }
        self.opened = None;
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
        if let Some(cursor) = self.clipboard_cursor.take() {
            self.history_cursor = cursor;
        }
    }

    /// Show `opened` on the card instead of the clipboard, from its original view. Every way a
    /// file gets onto the card goes through here, and the card labels and blurs it like the
    /// clipboard. A chosen file is not recorded in history; a drop is (see `show_dropped`).
    fn show_source(&mut self, opened: crate::open_file::OpenedFile) {
        if self
            .table_window
            .as_ref()
            .is_some_and(|window| window.opened)
        {
            self.close_table_window();
        }
        self.opened = Some(opened);
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
        self.card_view = CardView::Original;
        self.refresh_popup();
    }

    fn launch_data(&mut self, view: &ClipboardView) -> LaunchData {
        if view.is_image() {
            self.ensure_image_scan();
        }
        let (subject_kind, subject_text) = match view {
            ClipboardView::Empty => (SubjectKind::Empty, None),
            ClipboardView::NoText => (SubjectKind::NoText, None),
            ClipboardView::Denied => (SubjectKind::Denied, None),
            ClipboardView::Pending if self.paste_explaining => (SubjectKind::PasteAsk, None),
            ClipboardView::Pending => (SubjectKind::Pending, None),
            ClipboardView::Hidden => (SubjectKind::Hidden, None),
            ClipboardView::Image => (SubjectKind::Image, None),
            ClipboardView::Text(text) => {
                self.card_view = commands::presented_view(text.as_str(), self.card_view);
                (SubjectKind::Text, Some(text.as_str().to_string()))
            }
        };
        let mut data = self.launch_shell();
        data.subject_kind = subject_kind;
        data.subject_text = subject_text;
        data.image = image_facts(view);
        data
    }

    fn launch_from_opened(&mut self) -> LaunchData {
        let name = self
            .opened
            .as_ref()
            .map(|file| file.name.clone())
            .unwrap_or_else(|| "File".to_string());
        let text = self.opened_text();
        let note = self.opened.as_ref().and_then(|file| file.note.clone());
        let image = self.opened.as_ref().and_then(|file| file.image.clone());
        if let Some(text) = text.as_deref() {
            self.card_view = commands::presented_view(text, self.card_view);
        }
        let mut data = self.launch_shell();
        data.source_name = Some(name);
        data.image = None;
        data.image_scan = None;
        if let Some(image) = image {
            // Drawn like a copied picture, from its own bytes instead of the clipboard's.
            data.subject_kind = SubjectKind::Image;
            data.image = image.facts;
            data.image_scan = image.scan;
            data.picture = Some(image.bytes);
        } else if let Some(text) = text {
            data.subject_kind = SubjectKind::Text;
            data.subject_text = Some(text);
        } else {
            data.subject_kind = SubjectKind::NoText;
            data.source_note = Some(note.unwrap_or_else(|| "Can't read this file".to_string()));
        }
        data
    }

    fn launch_shell(&self) -> LaunchData {
        let history = self
            .history
            .labels()
            .into_iter()
            .map(|(index, _)| Hist {
                index,
                title: history_title(&self.history, index),
                mark: self.history.mark(index).unwrap_or("").to_string(),
            })
            .collect();
        LaunchData {
            subject_kind: SubjectKind::Empty,
            subject_text: None,
            image: None,
            history,
            can_clear_history: !self.history.is_empty(),
            history_nav: commands::history_nav(self.history.len(), self.history_cursor),
            theme: appearance::load(),
            settings: crate::settings::load(),
            view: self.card_view,
            image_scan: self.image_scan.clone(),
            source_name: None,
            source_note: None,
            full: false,
            picture: None,
            table: None,
            image_edit: None,
        }
    }

    /// Text the card is showing: the chosen file, or the clipboard.
    fn opened_text(&self) -> Option<String> {
        self.opened
            .as_ref()
            .and_then(|file| file.text.as_ref().map(|value| value.as_str().to_string()))
    }

    fn source_text(&self) -> Option<String> {
        if self.opened.is_some() {
            self.opened_text()
        } else {
            ClipboardView::from_os().text().map(str::to_string)
        }
    }

    fn ensure_image_scan(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let change = crate::macos_pasteboard::change_count();
            if self.image_scan_change == Some(change) || self.image_scan_for == Some(change) {
                return;
            }
            self.image_scan = None;
            self.image_scan_change = None;
            if let Some(scan) = self
                .history_image_on_pasteboard(change)
                .and_then(|bytes| self.history.image_scan(&bytes))
            {
                self.image_scan_change = Some(change);
                self.image_scan = Some(scan);
                return;
            }
            self.image_scan_for = Some(change);
            // The picture is fetched here, on the main thread (a pasteboard read can show the
            // privacy alert, which is modal); the thread only decodes and scans it.
            let input = crate::macos_pasteboard::card_image_input();
            std::thread::spawn(move || {
                let scan = input.and_then(crate::macos_pasteboard::scan_card_input);
                launcher::emit(UserEvent::ImageScanned { change, scan });
            });
        }
    }

    fn finish_image_scan(&mut self, change: isize, scan: Option<commands::ImageScan>) {
        #[cfg(target_os = "macos")]
        let current = crate::macos_pasteboard::change_count();
        #[cfg(not(target_os = "macos"))]
        let current = change;
        if change != current {
            return;
        }
        self.image_scan_for = None;
        self.image_scan_change = Some(change);
        if let (Some(bytes), Some(scan)) = (self.history_image_on_pasteboard(change), scan.as_ref())
        {
            self.history.remember_image_scan(&bytes, scan);
        }
        self.image_scan = scan;
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// The history picture on the pasteboard at `change`, if copycraft knows which one it is.
    fn history_image_on_pasteboard(&self, change: isize) -> Option<SecretBytes> {
        self.image_on_pasteboard
            .as_ref()
            .filter(|(at, _)| *at == change)
            .map(|(_, bytes)| bytes.clone())
    }

    fn copy_current(&mut self) -> anyhow::Result<()> {
        let from_file = self.opened.is_some();
        let version = self.shown_image_version();
        if let Some(version) = version.as_ref()
            && self.card_view == CardView::Original
        {
            return self.copy_image_version(version);
        }
        if let Some(image) = self.opened.as_ref().and_then(|file| file.image.as_ref()) {
            // The dropped picture stays on the card; only the view's text is copied.
            let scan = match &version {
                Some(version) => version.scan.as_ref(),
                None => image.scan.as_ref(),
            };
            if let Some(text) = commands::image_view_text(scan, self.card_view).map(Zeroizing::new)
            {
                self.write_own(text.as_str())?;
                self.record_own_copy(text.as_str());
            }
            return Ok(());
        }
        if !from_file && ClipboardView::from_os().is_image() {
            return self.copy_image_view();
        }
        let Some(source) = self.source_text().map(Zeroizing::new) else {
            return Ok(());
        };
        let shown = commands::presented_view(source.as_str(), self.card_view);
        let version = (shown == CardView::Dataframe)
            .then(|| self.shown_table_version())
            .flatten();
        let body = match version {
            Some(frame) => crate::dataframe::frame_grid(&frame),
            None => commands::transformed_text(source.as_str(), shown),
        };
        let Some(body) = body.map(Zeroizing::new) else {
            return Ok(());
        };
        // The clipboard already holds this text. A file does not, so Copy still writes it.
        if !from_file && body.as_str() == source.as_str() {
            return Ok(());
        }
        self.write_own(body.as_str())?;
        self.record_own_copy(body.as_str());
        if from_file {
            return Ok(());
        }
        self.card_view = CardView::Original;
        self.refresh_popup();
        Ok(())
    }

    fn copy_image_view(&mut self) -> anyhow::Result<()> {
        let version = self.shown_image_version();
        let scan = match &version {
            Some(version) => version.scan.as_ref(),
            None => self.image_scan.as_ref(),
        };
        let Some(text) = commands::image_view_text(scan, self.card_view).map(Zeroizing::new) else {
            return Ok(());
        };
        self.card_view = CardView::Original;
        self.write_own(text.as_str())?;
        self.record_own_copy(text.as_str());
        self.refresh_popup();
        Ok(())
    }

    /// Copy the picture version shown: its PNG goes on the pasteboard (as `public.png`) and
    /// into history as a new copy, with the version's scan.
    fn copy_image_version(
        &mut self,
        version: &crate::image_edit::VersionPicture,
    ) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let png = Zeroizing::new(version.picture.with(<[u8]>::to_vec));
            crate::macos_pasteboard::write_history_image(&png)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            self.sensitive_clear = None;
            self.conceal_when_checked = None;
            let change = crate::macos_pasteboard::change_count();
            // A copy of its own: the version's bytes go when it is no longer kept.
            let recorded = self.history.record_image(png.to_vec());
            if let (Some(bytes), Some(scan)) = (recorded.as_ref(), version.scan.as_ref()) {
                self.history.remember_image_scan(bytes, scan);
                self.image_scan = Some(scan.clone());
                self.image_scan_change = Some(change);
            }
            self.recorded_image_change = Some(change);
            self.image_on_pasteboard = recorded.clone().map(|bytes| (change, bytes));
            self.current_image = recorded;
            self.last_copy = Some(Instant::now());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
            self.refresh_status_menu();
            self.refresh_popup();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = version;
        Ok(())
    }

    fn save_current(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.saving.is_some() {
                return;
            }
            let started = self.start_save();
            launcher::order_front();
            if let Err(e) = started {
                log_save_failure(&format!("{e:#}"));
            }
        }
    }

    /// Ask for a path on the main thread, then build and write the file on a save thread,
    /// which reports back with [`UserEvent::SaveFinished`]. `Ok` also covers a cancelled save
    /// panel and a card with nothing to save.
    #[cfg(target_os = "macos")]
    fn start_save(&mut self) -> anyhow::Result<()> {
        let version = (self.card_view == CardView::Dataframe)
            .then(|| self.shown_table_version())
            .flatten();
        let image_version = self.shown_image_version();
        let job = match version {
            Some(frame) => {
                let mut job = crate::macos_save::SaveJob::table(frame);
                if let Some(opened) = self.opened.as_ref() {
                    job.filename = crate::open_file::save_name(&opened.name, job.extension);
                }
                Some(job)
            }
            None if image_version.is_some() => image_version
                .as_ref()
                .map(|version| self.image_version_save_job(version)),
            None if self.opened.is_some() => self.opened_save_job(),
            None => self.clipboard_save_job(),
        };
        let Some(mut job) = job else {
            return Ok(());
        };
        let Some(path) = job.choose_path()? else {
            return Ok(());
        };
        let content = job.content;
        std::thread::Builder::new()
            .name("copycraft-save".into())
            .spawn(move || {
                let result = content.write_to(&path).map_err(|e| format!("{e:#}"));
                launcher::emit(UserEvent::SaveFinished(result));
            })
            .context("cannot start the save thread")?;
        self.saving = Some(Background::now());
        Ok(())
    }

    /// Show the card spinner once background work (a save, "Show all") runs longer than
    /// [`SPINNER_DELAY`]. Returns when the event loop should wake up next.
    fn spin_slow_work(&mut self, now: Instant) -> Instant {
        let wake = now + poll_interval(launcher::is_open());
        if self.spinner_on {
            return wake;
        }
        let table = self
            .table_job
            .as_ref()
            .filter(|run| !run.quiet)
            .map(|run| run.started);
        let image = self.image_job.as_ref().map(|run| run.started);
        let Some(started) = [self.saving, self.loading_all, table, image]
            .into_iter()
            .flatten()
            .map(|run| run.started)
            .min()
        else {
            return wake;
        };
        let due = started + SPINNER_DELAY;
        if now >= due {
            self.spinner_on = true;
            launcher::set_busy(true);
            eprintln!(
                "copycraft: spinner shown after {} ms",
                now.duration_since(started).as_millis()
            );
            wake
        } else {
            wake.min(due)
        }
    }

    /// Take the spinner away once no background work is left.
    fn stop_spinner_when_idle(&mut self) {
        if self.spinner_on
            && self.saving.is_none()
            && self.loading_all.is_none()
            && self.table_job.as_ref().is_none_or(|run| run.quiet)
            && self.image_job.is_none()
        {
            self.spinner_on = false;
            launcher::set_busy(false);
            eprintln!("copycraft: spinner hidden");
        }
    }

    fn finish_save(&mut self, result: Result<(), String>) {
        if let Some(run) = self.saving.take() {
            let shown = if self.spinner_on {
                "spinner shown"
            } else {
                "under the spinner delay, no spinner"
            };
            eprintln!(
                "copycraft: save finished in {} ms ({shown})",
                run.elapsed_ms()
            );
        }
        self.stop_spinner_when_idle();
        if let Err(message) = result {
            log_save_failure(&message);
        }
    }

    /// Save job for the picture version shown (an Image ▾ step's): the shown scan text, else
    /// the picture as PNG, JPEG or HEIC (the Format popup, the source's format first). Named
    /// after the dropped or chosen file.
    #[cfg(target_os = "macos")]
    fn image_version_save_job(
        &self,
        version: &crate::image_edit::VersionPicture,
    ) -> crate::macos_save::SaveJob {
        use crate::macos_save::{SaveContent, SaveJob};
        let name = self
            .opened
            .as_ref()
            .map_or("clipboard", |opened| opened.name.as_str());
        if let Some(text) = commands::image_view_text(version.scan.as_ref(), self.card_view) {
            return SaveJob {
                filename: crate::open_file::save_name(name, "txt"),
                extension: "txt",
                content: SaveContent::Bytes(Zeroizing::new(text.into_bytes())),
            };
        }
        let source = self
            .image_source()
            .map(|bytes| bytes.with(crate::macos_pasteboard::image_kind));
        SaveJob::image(
            name,
            Zeroizing::new(version.picture.with(<[u8]>::to_vec)),
            true,
            crate::image_edit::image_files(source, false),
        )
    }

    /// Save job for the chosen file's text, or a dropped picture, named after the file.
    #[cfg(target_os = "macos")]
    fn opened_save_job(&self) -> Option<crate::macos_save::SaveJob> {
        let opened = self.opened.as_ref()?;
        if let Some(image) = opened.image.as_ref() {
            return Some(dropped_image_save_job(&opened.name, image, self.card_view));
        }
        let source = opened.text.as_deref()?;
        let view = commands::presented_view(source, self.card_view);
        let mut job = crate::macos_save::SaveJob::text(source, view)?;
        job.filename = crate::open_file::save_name(&opened.name, job.extension);
        Some(job)
    }

    #[cfg(target_os = "macos")]
    fn clipboard_save_job(&self) -> Option<crate::macos_save::SaveJob> {
        let view = ClipboardView::from_os();
        if view.is_image() {
            return Some(self.image_save_job());
        }
        let source = view.text()?;
        crate::macos_save::SaveJob::text(source, commands::presented_view(source, self.card_view))
    }

    /// Save job for the image card: the shown scan text, else the history entry's picture (as
    /// it is, or as PNG, JPEG or HEIC), else the pasteboard image as PNG or JPEG, else the
    /// decoded preview encoded as PNG (on the save thread).
    #[cfg(target_os = "macos")]
    fn image_save_job(&self) -> crate::macos_save::SaveJob {
        use crate::macos_save::{SaveContent, SaveJob};
        let job = |filename: &str, extension, content| SaveJob {
            filename: filename.to_string(),
            extension,
            content,
        };
        if let Some(text) = commands::image_view_text(self.image_scan.as_ref(), self.card_view) {
            let bytes = Zeroizing::new(text.into_bytes());
            return job("clipboard.txt", "txt", SaveContent::Bytes(bytes));
        }
        // The history entry's picture: as it is, or another format from the Format popup.
        if let Some(original) = self.image_source() {
            let source = original.with(crate::macos_pasteboard::image_kind);
            let bytes = Zeroizing::new(original.with(<[u8]>::to_vec));
            let files = crate::image_edit::image_files(Some(source), true);
            return SaveJob::image("clipboard", bytes, false, files);
        }
        if let Some(bytes) = crate::macos_pasteboard::current_image_bytes().map(Zeroizing::new) {
            if bytes.starts_with(b"\x89PNG") {
                return job("clipboard.png", "png", SaveContent::Bytes(bytes));
            }
            if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                return job("clipboard.jpg", "jpg", SaveContent::Bytes(bytes));
            }
        }
        // Fetched here, on the main thread; the save thread only decodes it.
        let input = crate::macos_pasteboard::card_image_input();
        job("clipboard.png", "png", SaveContent::ClipboardPng(input))
    }

    fn visit_current(&self) {
        let Some(text) = self.source_text() else {
            return;
        };
        let raw = crate::page_preview::page_url(&text)
            .map(str::to_string)
            .or_else(|| crate::youtube::video_id(&text).map(|_| text.trim().to_string()));
        let Some(raw) = raw else {
            return;
        };
        let url = crate::format::format_text(&raw);
        let url = if crate::page_preview::url_has_userinfo(&url) {
            crate::page_preview::strip_userinfo(&url)
        } else {
            url
        };
        if !crate::url_policy::may_visit(&url) {
            eprintln!("visit refused: only http and https links are opened");
            return;
        }
        #[cfg(target_os = "macos")]
        if !crate::macos_open::open_web_url(&url) {
            eprintln!("visit failed: the system could not open the link");
        }
    }

    fn format_link_in_place(&mut self) -> bool {
        let Some(text) = self.source_text() else {
            return false;
        };
        let link = crate::page_preview::page_url(&text).is_some()
            || crate::youtube::video_id(&text).is_some();
        if !link {
            return false;
        }
        let formatted = crate::format::format_text(text.trim());
        if formatted != text.trim() {
            if self.opened.is_some() {
                if let Some(file) = self.opened.as_mut() {
                    file.text = Some(Zeroizing::new(formatted));
                }
                self.refresh_popup();
            } else if let Err(e) = self.write_own(&formatted) {
                eprintln!("format link failed: {e}");
            } else {
                self.record_own_copy(&formatted);
                self.refresh_popup();
            }
        }
        true
    }

    fn clear_history(&mut self) {
        let view = ClipboardView::from_os();
        self.skip_record = view.text().map(|text| Zeroizing::new(text.to_string()));
        if view.is_image() {
            #[cfg(target_os = "macos")]
            {
                self.skip_image_change = Some(crate::macos_pasteboard::change_count());
            }
        }
        self.current_image = None;
        self.image_on_pasteboard = None;
        self.cancel_image_job();
        self.image_note = None;
        self.cancel_table_job();
        self.table_error = None;
        self.close_table_window();
        self.history.clear();
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    /// Forget history once [`Self::history_minutes`] have passed since the last copy. Returns
    /// when the event loop should look again.
    fn forget_history_when_due(&mut self, now: Instant) -> Option<Instant> {
        let due = history_due(
            self.last_copy,
            self.history_minutes,
            self.history.is_empty(),
        )?;
        if now < due {
            return Some(due);
        }
        self.last_copy = None;
        self.clear_history();
        eprintln!(
            "copycraft: history forgotten {} min after the last copy",
            self.history_minutes
        );
        None
    }

    /// The screen locked, the Mac is going to sleep or another user took over: forget history
    /// whatever the setting says.
    fn session_ended(&mut self) {
        self.last_copy = None;
        self.full_card = None;
        // Closed and wiped whatever history holds (a chosen file's window too).
        self.close_table_window();
        if !self.history.is_empty() {
            self.clear_history();
            eprintln!("copycraft: history forgotten (lock, sleep or user switch)");
        }
    }

    /// Empty the OS pasteboard. History and the open card stay.
    fn clear_clipboard(&mut self) -> anyhow::Result<()> {
        clipboard::clear_clipboard().map_err(anyhow::Error::msg)?;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        self.skip_record = Some(Zeroizing::new(String::new()));
        Ok(())
    }

    /// Overwrite clipboard text and image bytes still held in this process,
    /// then empty the pasteboard.
    fn clear_secrets(&mut self) {
        self.close_table_window();
        self.opened = None;
        self.opened_table = crate::settings::preferred_table();
        self.opened_image = ImageVersions::default();
        self.cancel_image_job();
        self.image_note = None;
        self.cancel_table_job();
        self.table_error = None;
        self.full_card = None;
        self.history.clear();
        self.current_image = None;
        self.image_on_pasteboard = None;
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.recorded_image_change = None;
        self.skip_image_change = None;
        self.card_view = CardView::Original;
        self.image_scan = None;
        self.image_scan_change = None;
        self.image_scan_for = None;
        self.skip_record = None;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        crate::memo::forget_all();
        launcher::wipe_shown();
        if let Err(e) = clipboard::clear_clipboard() {
            eprintln!("clear clipboard failed: {e}");
        }
        self.signature = ClipSig::default();
        self.polled_change = None;
        let view = ClipboardView::from_os();
        self.sync_tooltip(&view);
        self.refresh_status_menu();
        if launcher::is_open() {
            self.record_current(&view);
            launcher::sync(self.launch_data(&view));
        }
    }

    fn should_record(&self, text: &str) -> bool {
        self.skip_record
            .as_ref()
            .is_none_or(|skipped| skipped.as_str() != text)
    }

    fn step_history(&mut self, older: bool) {
        let Some(next) = commands::step_history(self.history.len(), self.history_cursor, older)
        else {
            return;
        };
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let previous = self.history_cursor;
        self.history_cursor = next;
        let presented = self.present_history(next).unwrap_or_else(|e| {
            eprintln!("restore history failed: {e:#}");
            false
        });
        if presented {
            // The entry is on the clipboard now, so the card follows the clipboard there.
            self.clipboard_cursor = None;
            self.note_own_write();
        } else {
            self.history_cursor = previous;
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "copycraft: history step to {} in {} µs",
            self.history_cursor,
            started.elapsed().as_micros()
        );
    }

    /// A step put a history entry on the pasteboard, and the card already shows it. Note the
    /// pasteboard as seen, so the next pass of the event loop does not take that change for a
    /// new copy: that rebuilt the whole card a second time, rebuilt the menu bar menu (history
    /// did not change) and blinked the icon. Only the tooltip follows the entry.
    fn note_own_write(&mut self) {
        let view = ClipboardView::from_os();
        self.record_current(&view);
        self.signature = self.clip_signature(&view);
        // As `note_clipboard` does for a change: a scan of another picture no longer applies.
        if self
            .image_scan_change
            .is_some_and(|seen| seen != self.signature.image_change)
        {
            self.image_scan_change = None;
            self.image_scan = None;
        }
        self.sync_tooltip(&view);
    }

    fn clip_signature(&self, view: &ClipboardView) -> ClipSig {
        #[cfg(target_os = "macos")]
        let image_change = crate::macos_pasteboard::change_count();
        #[cfg(not(target_os = "macos"))]
        let image_change = 0;
        ClipSig {
            text_hash: hash_text(view.text().unwrap_or("")),
            image: view.is_image(),
            image_change,
            history_len: self.history.len(),
        }
    }

    // Put a history entry on the clipboard without moving it to the front.
    // `Ok(false)`: there is no entry to show at `index`.
    fn present_history(&mut self, index: usize) -> anyhow::Result<bool> {
        if self.history.image(index).is_some() {
            return self.present_history_image(index);
        }
        let Some(text) = self
            .history
            .get(index)
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            return Ok(false);
        };
        // The write carries copycraft's own pasteboard type, so it is not recorded: that would
        // move this entry to the front, and the other arrow could no longer walk back.
        self.card_view = CardView::Original;
        self.write_own(text.as_str())?;
        self.opened = None;
        if launcher::is_open() {
            self.refresh_popup();
        }
        Ok(true)
    }

    fn present_history_image(&mut self, index: usize) -> anyhow::Result<bool> {
        let Some(bytes) = self.history.image(index) else {
            return Ok(false);
        };
        #[cfg(target_os = "macos")]
        {
            bytes
                .with(crate::macos_pasteboard::write_history_image)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            self.image_on_pasteboard = Some((crate::macos_pasteboard::change_count(), bytes));
            self.card_view = CardView::Original;
            self.opened = None;
            if launcher::is_open() {
                self.refresh_popup();
            }
            Ok(true)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = bytes;
            Ok(false)
        }
    }

    fn restore_history(&mut self, index: usize) -> anyhow::Result<()> {
        self.card_view = CardView::Original;
        self.clipboard_cursor = None;
        if let Some(bytes) = self.history.image(index) {
            #[cfg(target_os = "macos")]
            bytes
                .with(crate::macos_pasteboard::write_history_image)
                .map_err(anyhow::Error::msg)
                .context("image")?;
            #[cfg(not(target_os = "macos"))]
            {
                let _ = bytes;
                return Ok(());
            }
            // Chosen from the menu: the entry moves to the front, as a new copy of it would.
            #[cfg(target_os = "macos")]
            {
                self.history.record_image_tracking(bytes.clone(), 0);
                self.image_on_pasteboard =
                    Some((crate::macos_pasteboard::change_count(), bytes.clone()));
                self.current_image = Some(bytes);
                self.history_cursor = 0;
                self.refresh_status_menu();
                self.opened = None;
                self.show_restored();
                return Ok(());
            }
        }
        let Some(text) = self
            .history
            .get(index)
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            return Ok(());
        };
        self.write_own(text.as_str())?;
        self.record_own_copy(text.as_str());
        self.opened = None;
        self.show_restored();
        Ok(())
    }

    /// Put text copycraft made or kept on the clipboard. Text labelled sensitive (credential,
    /// PII, financial) goes on with nspasteboard.org's Concealed type, and with that setting on
    /// it is cleared after [`SENSITIVE_CLEAR_AFTER`] unless something else was copied by then.
    /// A long copy whose labels are still being checked goes on plain; [`Self::labels_checked`]
    /// conceals it once they are in, if the pasteboard still holds it.
    fn write_own(&mut self, text: &str) -> anyhow::Result<()> {
        let labeling = crate::sensitivity::labeling(text);
        let concealed = matches!(&labeling, crate::sensitivity::Labeling::Known(found) if !found.labels.is_empty());
        clipboard::write_clipboard(text, concealed).map_err(anyhow::Error::msg)?;
        self.sensitive_clear = None;
        self.conceal_when_checked = None;
        #[cfg(target_os = "macos")]
        {
            let change = crate::macos_pasteboard::change_count();
            if concealed {
                self.clear_sensitive_later(change);
            } else if labeling == crate::sensitivity::Labeling::Checking {
                self.conceal_when_checked = Some(change);
            }
        }
        Ok(())
    }

    fn clear_sensitive_later(&mut self, change: isize) {
        if crate::settings::load().clear_sensitive {
            self.sensitive_clear = Some(SensitiveClear {
                change,
                due: Instant::now() + SENSITIVE_CLEAR_AFTER,
            });
        }
    }

    /// The checker has labels for a long copy: rebuild the open card (its meta line said
    /// "Checking…"), and conceal copycraft's own write that was waiting for them.
    fn labels_checked(&mut self) {
        #[cfg(target_os = "macos")]
        if let Some(change) = self.conceal_when_checked
            && crate::macos_pasteboard::change_count() == change
        {
            let view = ClipboardView::from_os();
            match view.text().map(crate::sensitivity::labeling) {
                Some(crate::sensitivity::Labeling::Checking) => {}
                Some(crate::sensitivity::Labeling::Known(found)) if !found.labels.is_empty() => {
                    self.conceal_when_checked = None;
                    crate::macos_pasteboard::add_concealed();
                    self.clear_sensitive_later(crate::macos_pasteboard::change_count());
                }
                _ => self.conceal_when_checked = None,
            }
        }
        if self
            .full_card
            .as_ref()
            .is_some_and(|full| commands::is_checking(&full.card))
        {
            self.full_card = None;
            self.refresh_popup();
            self.show_all();
            return;
        }
        self.refresh_popup();
    }

    /// Empty the pasteboard once the sensitive copy's minute is up, if it still holds that copy.
    /// Returns when the event loop should look again.
    fn clear_sensitive_when_due(&mut self, now: Instant) -> Option<Instant> {
        let pending = self.sensitive_clear?;
        if now < pending.due {
            return Some(pending.due);
        }
        self.sensitive_clear = None;
        #[cfg(not(target_os = "macos"))]
        let _ = pending.change;
        #[cfg(target_os = "macos")]
        if crate::macos_pasteboard::change_count() == pending.change {
            if let Err(e) = self.clear_clipboard() {
                eprintln!("clear sensitive copy failed: {e:#}");
            } else {
                eprintln!("copycraft: sensitive copy cleared after a minute");
            }
        }
        None
    }

    /// Text copycraft put on the clipboard (Copy, Format in place, a history entry chosen from
    /// the menu): the newest copy in history. The poller skips copycraft's own writes, so this
    /// is where they are recorded.
    fn record_own_copy(&mut self, text: &str) {
        self.last_copy = Some(Instant::now());
        self.history.record(text.to_string());
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
    }

    /// The history row stays a type mark. The card shows the copy that was chosen.
    fn show_restored(&mut self) {
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::set_card_shown(true);
        let view = ClipboardView::from_os();
        self.record_current(&view);
        let mut data = self.launch_data(&view);
        self.attach_image(&mut data);
        if launcher::is_open() {
            launcher::sync(data);
        } else {
            launcher::reveal(data);
        }
    }

    fn record_current(&mut self, view: &ClipboardView) {
        // Copycraft's own write: what belongs in history was recorded when it was written (see
        // `record_own_copy`), and a history step stays where it is.
        #[cfg(target_os = "macos")]
        if crate::macos_pasteboard::current_marks().own {
            return;
        }
        #[cfg(target_os = "macos")]
        if view.is_image() {
            let change = crate::macos_pasteboard::change_count();
            if self.skip_image_change == Some(change) || self.recorded_image_change == Some(change)
            {
                return;
            }
            self.recorded_image_change = Some(change);
            if let Some(bytes) = crate::macos_pasteboard::current_image_bytes() {
                self.last_copy = Some(Instant::now());
                self.current_image = self.history.record_image(bytes);
                self.image_on_pasteboard = self.current_image.clone().map(|bytes| (change, bytes));
                self.history_cursor = 0;
                self.clipboard_cursor = None;
            }
            return;
        }
        self.current_image = None;
        if let Some(text) = view.text()
            && self.should_record(text)
        {
            self.skip_record = None;
            self.last_copy = Some(Instant::now());
            self.history.record(text.to_string());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
        }
    }

    fn note_clipboard(&mut self, now: Instant) -> bool {
        // A quiet tick reads only the change count: no copy, hash or record of the text. A
        // promised file can put its picture on the pasteboard without a new change count, so an
        // empty or textless reading is looked at again.
        #[cfg(target_os = "macos")]
        {
            // With Default or Ask a copy is read only while the card is shown.
            let open = launcher::is_open();
            if !open {
                self.paste_explaining = false;
            }
            crate::macos_pasteboard::set_card_shown(open);
            let change = crate::macos_pasteboard::change_count();
            if self.polled_change == Some(change)
                && !self.poll_again
                && self.signature.history_len == self.history.len()
            {
                return false;
            }
            self.polled_change = Some(change);
        }
        let view = ClipboardView::from_os();
        // Looked at again every tick, but `current_view` reads a copy at most once
        // (`paste_access::plan`): no reads per tick. A copy waiting for the card is read on the
        // first tick the card is shown.
        self.poll_again = matches!(
            view,
            ClipboardView::Empty
                | ClipboardView::NoText
                | ClipboardView::Denied
                | ClipboardView::Pending
        );
        if !matches!(view, ClipboardView::Pending) {
            self.paste_explaining = false;
        }
        self.record_current(&view);
        let signature = self.clip_signature(&view);
        let image_change = signature.image_change;
        if signature == self.signature {
            return false;
        }
        #[cfg(target_os = "macos")]
        let own = crate::macos_pasteboard::current_marks().own;
        #[cfg(not(target_os = "macos"))]
        let own = false;
        if !own
            && self
                .signature
                .is_new_copy(&signature, matches!(view, ClipboardView::Empty))
        {
            self.blink.start(now);
        }
        // A chosen file stays on the card while the clipboard keeps its own history.
        if self.opened.is_none() {
            self.card_view = CardView::Original;
            if self
                .image_scan_change
                .is_some_and(|seen| seen != image_change)
            {
                self.image_scan_change = None;
                self.image_scan = None;
            }
        }
        self.signature = signature;
        if let Some(text) = view.text() {
            prewarm(text);
        }
        self.sync_tooltip(&view);
        self.refresh_status_menu();
        true
    }

    fn refresh_status_menu(&self) {
        let entries: Vec<(usize, String)> = self
            .history
            .labels()
            .into_iter()
            .map(|(index, _)| (index, history_title(&self.history, index)))
            .collect();
        self.tray.set_menu(Some(Box::new(status_menu(&entries))));
    }

    /// Swap the menu bar glyph when the blink asks for it. Returns when the blink needs the
    /// event loop again; `None` when it is idle.
    fn show_blink(&mut self, now: Instant) -> Option<Instant> {
        let step = self.blink.tick(now);
        if let Some(glyph) = step.swap
            && let Err(e) = self.icons.show(&self.tray, glyph)
        {
            eprintln!("menu bar icon blink failed: {e}");
        }
        step.wake
    }

    /// The icon is always the same template (apart from the blink), so the tooltip and the VoiceOver label are where
    /// the detected kind shows.
    fn sync_tooltip(&self, view: &ClipboardView) {
        let tip = icon_tip(view);
        if let Err(e) = self.tray.set_tooltip(Some(&tip)) {
            eprintln!("menu bar tooltip failed: {e}");
        }
        mac_ui::tray::set_accessibility_label(&self.tray, &tray_label(&tip));
    }
}

fn icon_tip(view: &ClipboardView) -> String {
    match view {
        ClipboardView::Image => "Image".to_string(),
        ClipboardView::Empty | ClipboardView::NoText => "Copycraft".to_string(),
        ClipboardView::Denied => crate::paste_access::DENIED_NOTE.to_string(),
        ClipboardView::Pending => crate::paste_access::PENDING_NOTE.to_string(),
        ClipboardView::Hidden => clipboard::HIDDEN_CONTENT.to_string(),
        ClipboardView::Text(text) => {
            if crate::youtube::video_id(text.as_str()).is_some() {
                "YouTube".to_string()
            } else if let Some(host) =
                crate::page_preview::page_url(text.as_str()).and_then(crate::page_preview::host)
            {
                host.to_string()
            } else {
                format::detect(text.as_str()).source_heading().to_string()
            }
        }
    }
}

/// VoiceOver label of the menu bar button: the app name, then the kind from the tooltip.
fn tray_label(tip: &str) -> String {
    if tip == "Copycraft" {
        tip.to_string()
    } else {
        format!("Copycraft, {tip}")
    }
}

fn image_facts(view: &ClipboardView) -> Option<commands::ImageFacts> {
    if !view.is_image() {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_pasteboard::image_facts()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// When history is due to be forgotten: `minutes` after the last copy. `None` with no time
/// limit (0) or nothing to forget.
fn history_due(last_copy: Option<Instant>, minutes: u32, empty: bool) -> Option<Instant> {
    if minutes == 0 || empty {
        return None;
    }
    Some(last_copy? + Duration::from_secs(u64::from(minutes) * 60))
}

/// The next poll: sooner while the card is open and follows the clipboard.
fn poll_interval(card_open: bool) -> Duration {
    if card_open { REFRESH } else { IDLE_REFRESH }
}

/// Classify a large copy on a background thread, so the card finds the format and the
/// sensitive-data labels remembered (see [`crate::memo`]) instead of scanning on the main thread.
fn prewarm(text: &str) {
    if text.len() < PREWARM_LEN {
        return;
    }
    let text = Zeroizing::new(text.to_string());
    // The labels: the background checker. The format and chips here, so the card finds them.
    crate::sensitivity::labeling(&text);
    let spawned = std::thread::Builder::new()
        .name("copycraft-prewarm".into())
        .spawn(move || {
            crate::format::detect(&text);
            crate::commands::warm_chips(&text);
        });
    if let Err(e) = spawned {
        eprintln!("prewarm skipped: {e}");
    }
}

/// The one place a failed save is reported, whether it failed before or on the save thread.
fn log_save_failure(message: &str) {
    eprintln!("save failed: {message}");
}

/// Save job for a dropped picture: the shown scan text, else the picture as it was dropped
/// (its own format, not converted), named after it.
#[cfg(target_os = "macos")]
fn dropped_image_save_job(
    name: &str,
    image: &crate::open_file::OpenedImage,
    view: CardView,
) -> crate::macos_save::SaveJob {
    use crate::macos_save::{SaveContent, SaveJob};
    if let Some(text) = commands::image_view_text(image.scan.as_ref(), view) {
        return SaveJob {
            filename: crate::open_file::save_name(name, "txt"),
            extension: "txt",
            content: SaveContent::Bytes(Zeroizing::new(text.into_bytes())),
        };
    }
    // As it is (its own format, the default), or as PNG, JPEG or HEIC.
    let source = image.bytes.with(crate::macos_pasteboard::image_kind);
    let bytes = Zeroizing::new(image.bytes.with(<[u8]>::to_vec));
    SaveJob::image(
        name,
        bytes,
        false,
        crate::image_edit::image_files(Some(source), true),
    )
}

fn history_title(history: &ClipboardHistory, index: usize) -> String {
    match (history.mark(index), history.byte_len(index)) {
        (Some(mark), Some(len)) => commands::history_label(mark, len),
        _ => String::new(),
    }
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn version_label() -> String {
    tray::version_label("Copycraft", env!("CARGO_PKG_VERSION"))
}

/// "Allow screenshots" (testing): the card and the table window can be captured until it is
/// switched off or Copycraft restarts.
const ALLOW_SCREENSHOTS_ID: &str = "allow-screenshots";

const SETTINGS_ID: &str = "settings";
const ABOUT_ID: &str = "about";

fn status_labels() -> [String; 3] {
    [
        crate::settings::hotkey_label(),
        version_label(),
        crate::locale::t("quit").to_string(),
    ]
}

#[cfg(test)]
fn status_rows(entries: &[(usize, String)]) -> Vec<String> {
    let [hotkey, version, quit] = status_labels();
    let mut rows = vec![hotkey];
    if !entries.is_empty() {
        rows.push(crate::locale::t("history").to_string());
    }
    rows.push(crate::locale::t("allow_screenshots").to_string());
    rows.push(crate::locale::t("settings").to_string());
    rows.push(crate::locale::t("about").to_string());
    rows.push(version);
    rows.push(quit);
    rows
}

fn status_menu(entries: &[(usize, String)]) -> Menu {
    let [hotkey, version, quit] = status_labels();
    let menu = Menu::new();
    let _ = menu.append(&tray::info_item(&hotkey));
    if !entries.is_empty() {
        let history = Submenu::new(crate::locale::t("history"), true);
        for (index, title) in entries {
            let _ = history.append(&MenuItem::with_id(
                format!("hist-{index}"),
                title,
                true,
                None,
            ));
        }
        let _ = menu.append(&history);
    }
    let _ = menu.append(&CheckMenuItem::with_id(
        ALLOW_SCREENSHOTS_ID,
        crate::locale::t("allow_screenshots"),
        true,
        launcher::allows_screenshots(),
        None,
    ));
    let _ = menu.append(&MenuItem::with_id(
        SETTINGS_ID,
        crate::locale::t("settings"),
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id(
        ABOUT_ID,
        crate::locale::t("about"),
        true,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&tray::info_item(&version));
    let _ = menu.append(&tray::quit_item(&quit));
    menu
}

fn register_format_hotkey()
-> Result<(GlobalHotKeyManager, global_hotkey::hotkey::HotKey, u32), Box<dyn std::error::Error>> {
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = crate::settings::hotkey();
    let id = hotkey.id();
    manager.register(hotkey)?;
    Ok((manager, hotkey, id))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    appearance::apply(appearance::load());
    // Nothing heavy on the main thread at launch: the sensitivity recognizers are built, and
    // the temporary files older versions left for history pictures removed, in the background.
    if let Err(e) = std::thread::Builder::new()
        .name("copycraft-warm".into())
        .spawn(|| {
            #[cfg(target_os = "macos")]
            crate::macos_pasteboard::remove_stale_history_files();
            crate::sensitivity::warm();
        })
    {
        eprintln!("background start-up skipped: {e}");
    }
    let (hotkeys, hotkey, format_hotkey_id) = register_format_hotkey()?;
    // Always the same template glyph. The kind is in the tooltip and the VoiceOver label.
    // A copy blinks it briefly with a filled variant, also a template.
    let icons = tray::Glyphs {
        normal: icon::menu_icon()?,
        flash: Some(icon::flash_icon()?),
        alert: None,
    };
    let tray = mac_ui::tray::with_icon(TrayIconBuilder::new(), icons.normal.clone(), true)
        .with_menu(Box::new(status_menu(&[])))
        .with_menu_on_left_click(false)
        .with_tooltip("Copycraft")
        .build()?;
    // Icon only: name the button for VoiceOver. `sync_tooltip` adds the kind.
    mac_ui::tray::set_accessibility_label(&tray, &tray_label("Copycraft"));

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    launcher::install_proxy(event_loop.create_proxy());
    crate::sensitivity::on_checked(|| launcher::emit(UserEvent::LabelsChecked));

    let mut app = App {
        tray,
        history: ClipboardHistory::default(),
        history_cursor: 0,
        clipboard_cursor: None,
        current_image: None,
        recorded_image_change: None,
        skip_image_change: None,
        skip_record: None,
        card_view: CardView::Original,
        opened: None,
        image_scan: None,
        image_scan_change: None,
        image_scan_for: None,
        signature: ClipSig::default(),
        saving: None,
        loading_all: None,
        full_card: None,
        spinner_on: false,
        blink: tray::Blink::default(),
        icons,
        hotkeys,
        format_hotkey: hotkey,
        format_hotkey_id,
        sensitive_clear: None,
        conceal_when_checked: None,
        polled_change: None,
        poll_again: false,
        paste_explaining: false,
        paste_explained: crate::settings::paste_alert_explained(),
        image_on_pasteboard: None,
        last_copy: None,
        history_minutes: crate::settings::load().history_minutes,
        opened_table: crate::settings::preferred_table(),
        table_job: None,
        table_error: None,
        table_window: None,
        opened_image: ImageVersions::default(),
        image_job: None,
        image_note: None,
    };
    #[cfg(target_os = "macos")]
    crate::macos_session::observe(|| launcher::emit(UserEvent::SessionEnded));
    // Pasteboard privacy (macOS 15.4+): the access setting, in the local log only.
    #[cfg(target_os = "macos")]
    crate::macos_pasteboard::log_access_behavior();
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ClipSig, history_due, icon_tip, status_labels, status_rows, tray_label, version_label,
    };
    use crate::clipboard::ClipboardView;
    use crate::format::FormatKind;
    use zeroize::Zeroizing;

    #[test]
    fn history_is_forgotten_minutes_after_the_last_copy() {
        let copied = std::time::Instant::now();
        assert_eq!(
            history_due(Some(copied), 15, false),
            Some(copied + std::time::Duration::from_secs(15 * 60))
        );
        assert_eq!(history_due(Some(copied), 0, false), None);
        assert_eq!(history_due(Some(copied), 5, true), None);
        assert_eq!(history_due(None, 5, false), None);
    }

    #[test]
    fn version_label_includes_package_version() {
        assert_eq!(
            version_label(),
            format!("Copycraft {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn tooltip_and_voiceover_label_name_the_kind() {
        let text = |s: &str| ClipboardView::Text(Zeroizing::new(s.into()));
        assert_eq!(icon_tip(&ClipboardView::Empty), "Copycraft");
        assert_eq!(icon_tip(&ClipboardView::NoText), "Copycraft");
        assert_eq!(icon_tip(&ClipboardView::Image), "Image");
        let kinds = [
            ("hello", FormatKind::Plain),
            (
                "Notes from the review.\nThe document continues on this line.\n",
                FormatKind::Text,
            ),
            (r#"{"a":1}"#, FormatKind::Json),
            ("fn main() {}", FormatKind::Rust),
        ];
        for (src, kind) in kinds {
            let tip = icon_tip(&text(src));
            assert_eq!(tip, kind.source_heading(), "{src}");
            assert_eq!(tray_label(&tip), format!("Copycraft, {tip}"));
        }
        assert_eq!(tray_label("Copycraft"), "Copycraft");
        assert_eq!(tray_label("Image"), "Copycraft, Image");
    }

    #[test]
    fn only_a_real_new_copy_blinks() {
        let seen = ClipSig {
            text_hash: 1,
            image: false,
            image_change: 7,
            history_len: 1,
        };
        let copy = ClipSig {
            text_hash: 2,
            image_change: 8,
            history_len: 2,
            ..seen
        };
        assert!(seen.is_new_copy(&copy, false));
        // The same text copied again still bumps the pasteboard change count.
        let again = ClipSig {
            image_change: 8,
            ..seen
        };
        assert!(seen.is_new_copy(&again, false));
        let image = ClipSig {
            image: true,
            image_change: 8,
            ..seen
        };
        assert!(seen.is_new_copy(&image, false));
        // First look after launch or a clear: no blink.
        assert!(!ClipSig::default().is_new_copy(&copy, false));
        // A cleared (empty) pasteboard: no blink.
        assert!(!seen.is_new_copy(&copy, true));
        // Only the history changed (cleared or trimmed): no blink.
        let history_only = ClipSig {
            history_len: 0,
            ..seen
        };
        assert!(!seen.is_new_copy(&history_only, false));
    }

    /// "Allow screenshots" is off at every start; on, the card and the table window are
    /// captured read-only, off again they are left out.
    #[test]
    fn allow_screenshots_starts_off_and_picks_the_sharing() {
        use crate::launcher::{Capture, capture_for};
        assert!(!crate::launcher::allows_screenshots());
        assert_eq!(crate::launcher::capture(), Capture::Excluded);
        assert_eq!(capture_for(false), Capture::Excluded);
        assert_eq!(capture_for(true), Capture::ReadOnly);
    }

    #[test]
    fn status_menu_lists_hotkey_then_version_then_quit() {
        assert_eq!(
            status_labels(),
            [
                crate::settings::hotkey_label(),
                version_label(),
                "Quit".to_string(),
            ]
        );
    }

    #[test]
    fn status_menu_lists_history_between_the_hotkey_and_version() {
        let entries = vec![
            (1, "older note".to_string()),
            (0, "just copied".to_string()),
        ];
        assert_eq!(
            status_rows(&entries),
            vec![
                crate::settings::hotkey_label(),
                crate::locale::t("history").to_string(),
                crate::locale::t("allow_screenshots").to_string(),
                crate::locale::t("settings").to_string(),
                crate::locale::t("about").to_string(),
                version_label(),
                crate::locale::t("quit").to_string(),
            ]
        );
    }
}
