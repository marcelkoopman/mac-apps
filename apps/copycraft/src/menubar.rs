use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use anyhow::Context;
use zeroize::Zeroizing;

use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use mac_ui::tray;
use mac_ui::tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
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
use crate::hotkey;
use crate::icon;
use crate::launcher::{self, UserEvent};

const REFRESH: Duration = Duration::from_millis(400);
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
    _hotkeys: GlobalHotKeyManager,
    format_hotkey_id: u32,
    /// The sensitive copy copycraft wrote last, to clear when its minute is up.
    sensitive_clear: Option<SensitiveClear>,
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
        let wake = self.spin_slow_work(now);
        let blink_wake = self.show_blink(now);
        let clear_wake = self.clear_sensitive_when_due(now);
        event_loop.set_control_flow(mac_ui::wake::control_flow([
            Some(wake),
            blink_wake,
            clear_wake,
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
            CommandId::Quit => event_loop.exit(),
        }
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
        if !launcher::is_open() {
            return;
        }
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
        let Some(index) = entry else {
            return commands::work_card(data);
        };
        if let Some(card) = self.history.card(index, data.view) {
            return card.clone();
        }
        let card = commands::work_card(data);
        self.history.remember_card(index, data.view, card.clone());
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
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        self.current_launch_data()
    }

    fn current_launch_data(&mut self) -> LaunchData {
        if self.opened.is_some() {
            self.launch_from_opened()
        } else {
            let view = ClipboardView::from_os();
            self.launch_data(&view)
        }
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
        self.opened = None;
        if let Some(cursor) = self.clipboard_cursor.take() {
            self.history_cursor = cursor;
        }
    }

    /// Show `opened` on the card instead of the clipboard, from its original view. Every way a
    /// file gets onto the card goes through here, and the card labels and blurs it like the
    /// clipboard. A chosen file is not recorded in history; a drop is (see `show_dropped`).
    fn show_source(&mut self, opened: crate::open_file::OpenedFile) {
        self.opened = Some(opened);
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
            self.image_scan_for = Some(change);
            std::thread::spawn(move || {
                let scan = crate::macos_pasteboard::scan_card_image();
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
        self.image_scan = scan;
        if launcher::is_open() {
            self.refresh_popup();
        }
    }

    fn copy_current(&mut self) -> anyhow::Result<()> {
        let from_file = self.opened.is_some();
        if let Some(image) = self.opened.as_ref().and_then(|file| file.image.as_ref()) {
            // The dropped picture stays on the card; only the view's text is copied.
            if let Some(text) =
                commands::image_view_text(image.scan.as_ref(), self.card_view).map(Zeroizing::new)
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
        let Some(body) = commands::transformed_text(source.as_str(), shown).map(Zeroizing::new)
        else {
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
        let Some(text) =
            commands::image_view_text(self.image_scan.as_ref(), self.card_view).map(Zeroizing::new)
        else {
            return Ok(());
        };
        self.card_view = CardView::Original;
        self.write_own(text.as_str())?;
        self.record_own_copy(text.as_str());
        self.refresh_popup();
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
        let job = if self.opened.is_some() {
            self.opened_save_job()
        } else {
            self.clipboard_save_job()
        };
        let Some(job) = job else {
            return Ok(());
        };
        let Some(path) = crate::macos_save::choose_path(&job.filename, job.extension)? else {
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
        let wake = now + REFRESH;
        if self.spinner_on {
            return wake;
        }
        let Some(started) = [self.saving, self.loading_all]
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
        if self.spinner_on && self.saving.is_none() && self.loading_all.is_none() {
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

    /// Save job for the image card: the shown scan text, else the pasteboard image as PNG or
    /// JPEG, else the decoded preview encoded as PNG (on the save thread).
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
        if let Some(bytes) = crate::macos_pasteboard::current_image_bytes().map(Zeroizing::new) {
            if bytes.starts_with(b"\x89PNG") {
                return job("clipboard.png", "png", SaveContent::Bytes(bytes));
            }
            if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                return job("clipboard.jpg", "jpg", SaveContent::Bytes(bytes));
            }
        }
        job("clipboard.png", "png", SaveContent::ClipboardPng)
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
        self.history.clear();
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
        if launcher::is_open() {
            self.refresh_popup();
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
        self.opened = None;
        self.full_card = None;
        self.history.clear();
        self.current_image = None;
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
    fn write_own(&mut self, text: &str) -> anyhow::Result<()> {
        let concealed = !crate::sensitivity::labels(text).is_empty();
        clipboard::write_clipboard(text, concealed).map_err(anyhow::Error::msg)?;
        self.sensitive_clear = None;
        #[cfg(target_os = "macos")]
        if concealed && crate::settings::load().clear_sensitive {
            self.sensitive_clear = Some(SensitiveClear {
                change: crate::macos_pasteboard::change_count(),
                due: Instant::now() + SENSITIVE_CLEAR_AFTER,
            });
        }
        Ok(())
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
        self.history.record(text.to_string());
        self.history_cursor = 0;
        self.clipboard_cursor = None;
        self.refresh_status_menu();
    }

    /// The history row stays a type mark. The card shows the copy that was chosen.
    fn show_restored(&mut self) {
        let view = ClipboardView::from_os();
        self.record_current(&view);
        if launcher::is_open() {
            launcher::sync(self.launch_data(&view));
        } else {
            launcher::reveal(self.launch_data(&view));
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
                self.current_image = self.history.record_image(bytes);
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
            self.history.record(text.to_string());
            self.history_cursor = 0;
            self.clipboard_cursor = None;
        }
    }

    fn note_clipboard(&mut self, now: Instant) -> bool {
        let view = ClipboardView::from_os();
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

/// Classify a large copy on a background thread, so the card finds the format and the
/// sensitive-data labels remembered (see [`crate::memo`]) instead of scanning on the main thread.
fn prewarm(text: &str) {
    if text.len() < PREWARM_LEN {
        return;
    }
    let text = Zeroizing::new(text.to_string());
    let spawned = std::thread::Builder::new()
        .name("copycraft-prewarm".into())
        .spawn(move || {
            crate::format::detect(&text);
            crate::sensitivity::labels(&text);
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
    let bytes = Zeroizing::new(image.bytes.with(<[u8]>::to_vec));
    let extension = infer::get(&bytes)
        .filter(|kind| kind.matcher_type() == infer::MatcherType::Image)
        .map_or("tiff", |kind| kind.extension());
    SaveJob {
        filename: crate::open_file::save_name(name, extension),
        extension,
        content: SaveContent::Bytes(bytes),
    }
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

fn status_labels() -> [String; 3] {
    [
        hotkey::LABEL.to_string(),
        version_label(),
        "Quit".to_string(),
    ]
}

#[cfg(test)]
fn status_rows(entries: &[(usize, String)]) -> Vec<String> {
    let [hotkey, version, quit] = status_labels();
    let mut rows = vec![hotkey];
    if !entries.is_empty() {
        rows.push("History".to_string());
    }
    rows.push(version);
    rows.push(quit);
    rows
}

fn status_menu(entries: &[(usize, String)]) -> Menu {
    let [hotkey, version, quit] = status_labels();
    let menu = Menu::new();
    let _ = menu.append(&tray::info_item(&hotkey));
    if !entries.is_empty() {
        let history = Submenu::new("History", true);
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
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&tray::info_item(&version));
    let _ = menu.append(&tray::quit_item(&quit));
    menu
}

fn register_format_hotkey() -> Result<(GlobalHotKeyManager, u32), Box<dyn std::error::Error>> {
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = hotkey::open();
    let id = hotkey.id();
    manager.register(hotkey)?;
    Ok((manager, id))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    appearance::apply(appearance::load());
    // History pictures used to go back on the pasteboard through temporary files.
    #[cfg(target_os = "macos")]
    crate::macos_pasteboard::remove_stale_history_files();
    let (hotkeys, format_hotkey_id) = register_format_hotkey()?;
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
        _hotkeys: hotkeys,
        format_hotkey_id,
        sensitive_clear: None,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ClipSig, icon_tip, status_labels, status_rows, tray_label, version_label};
    use crate::clipboard::ClipboardView;
    use crate::format::FormatKind;
    use crate::hotkey;
    use zeroize::Zeroizing;

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

    #[test]
    fn status_menu_lists_hotkey_then_version_then_quit() {
        assert_eq!(
            status_labels(),
            [
                hotkey::LABEL.to_string(),
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
                hotkey::LABEL.to_string(),
                "History".to_string(),
                version_label(),
                "Quit".to_string(),
            ]
        );
    }
}
