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
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

use crate::appearance;
use crate::clipboard::{self, ClipboardHistory, ClipboardView, SecretBytes};
use crate::commands::{self, CardView, CommandId, Hist, LaunchData, SubjectKind};
use crate::format;
use crate::hotkey;
use crate::icon;
use crate::launcher::{self, UserEvent};

const REFRESH: Duration = Duration::from_millis(400);
/// A save that finishes sooner shows no spinner, so small files do not flicker.
const SPINNER_DELAY: Duration = Duration::from_millis(180);

struct App {
    tray: TrayIcon,
    shown_kind: Option<format::FormatKind>,
    history: ClipboardHistory,
    /// Index into history while the arrows are browsing. 0 is the newest.
    history_cursor: usize,
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
    saving: Option<SaveRun>,
    _hotkeys: GlobalHotKeyManager,
    format_hotkey_id: u32,
}

/// A save that is writing in the background.
#[derive(Debug, Clone, Copy)]
struct SaveRun {
    started: Instant,
    /// The card spinner is showing (only after [`SPINNER_DELAY`]).
    spinner: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClipSig {
    text_hash: u64,
    image: bool,
    image_change: isize,
    history_len: usize,
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
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A closed card forgets the file, so the next hotkey shows the clipboard.
        if self.opened.is_some() && !launcher::is_open() {
            self.opened = None;
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
                self.reveal_popup();
            }
        }

        let changed = self.note_clipboard();
        if changed && launcher::is_open() {
            launcher::sync(self.current_launch_data());
        }
        let wake = self.spin_slow_save(Instant::now());
        event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
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
                self.opened = None;
                self.card_view = CardView::Original;
                self.image_scan = None;
                self.image_scan_change = None;
                self.refresh_popup();
            }
            CommandId::Appearance(theme) => {
                appearance::save(theme);
                self.refresh_popup();
            }
            CommandId::Quit => event_loop.exit(),
        }
    }

    fn summon_popup(&mut self) {
        if !launcher::is_open() {
            self.card_view = CardView::Original;
            self.opened = None;
        }
        launcher::summon(self.launch_for_popup());
    }

    fn reveal_popup(&mut self) {
        if !launcher::is_open() {
            self.card_view = CardView::Original;
            self.opened = None;
        }
        launcher::reveal(self.launch_for_popup());
    }

    fn refresh_popup(&mut self) {
        if !launcher::is_open() {
            return;
        }
        if self.opened.is_none() {
            let view = ClipboardView::from_os();
            self.record_current(&view);
        }
        launcher::sync(self.current_launch_data());
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
            launcher::set_suppress_resign(true);
            let path = crate::macos_open::choose_path();
            launcher::set_suppress_resign(false);
            if let Some(path) = path {
                self.opened = Some(crate::open_file::load(&path));
                self.card_view = CardView::Original;
                self.refresh_popup();
            }
            launcher::order_front();
        }
    }

    fn launch_data(&mut self, view: &ClipboardView) -> LaunchData {
        if view.is_image() {
            self.ensure_image_scan();
        }
        let (subject_kind, subject_text) = match view {
            ClipboardView::Empty => (SubjectKind::Empty, None),
            ClipboardView::NoText => (SubjectKind::NoText, None),
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
        if let Some(text) = text.as_deref() {
            self.card_view = commands::presented_view(text, self.card_view);
        }
        let mut data = self.launch_shell();
        data.source_name = Some(name);
        data.image = None;
        data.image_scan = None;
        if let Some(text) = text {
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
            view: self.card_view,
            image_scan: self.image_scan.clone(),
            source_name: None,
            source_note: None,
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
        clipboard::write_clipboard(body.as_str()).map_err(anyhow::Error::msg)?;
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
        clipboard::write_clipboard(text.as_str()).map_err(anyhow::Error::msg)?;
        self.refresh_popup();
        Ok(())
    }

    fn save_current(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.saving.is_some() {
                return;
            }
            launcher::set_suppress_resign(true);
            let started = self.start_save();
            launcher::set_suppress_resign(false);
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
        self.saving = Some(SaveRun {
            started: Instant::now(),
            spinner: false,
        });
        Ok(())
    }

    /// Show the card spinner once a save runs longer than [`SPINNER_DELAY`]. Returns when the
    /// event loop should wake up next.
    fn spin_slow_save(&mut self, now: Instant) -> Instant {
        let wake = now + REFRESH;
        let Some(run) = self.saving.as_mut() else {
            return wake;
        };
        if run.spinner {
            return wake;
        }
        let due = run.started + SPINNER_DELAY;
        if now >= due {
            run.spinner = true;
            launcher::set_busy(true);
            wake
        } else {
            wake.min(due)
        }
    }

    fn finish_save(&mut self, result: Result<(), String>) {
        if self.saving.take().is_some_and(|run| run.spinner) {
            launcher::set_busy(false);
        }
        if let Err(message) = result {
            log_save_failure(&message);
        }
    }

    /// Save job for the chosen file's text, named after the file.
    #[cfg(target_os = "macos")]
    fn opened_save_job(&self) -> Option<crate::macos_save::SaveJob> {
        let opened = self.opened.as_ref()?;
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
            } else if let Err(e) = clipboard::write_clipboard(&formatted) {
                eprintln!("format link failed: {e}");
            } else {
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
        self.history.clear();
        self.current_image = None;
        self.history_cursor = 0;
        self.recorded_image_change = None;
        self.skip_image_change = None;
        self.card_view = CardView::Original;
        self.image_scan = None;
        self.image_scan_change = None;
        self.image_scan_for = None;
        self.skip_record = None;
        #[cfg(target_os = "macos")]
        crate::macos_pasteboard::zeroize_caches();
        launcher::wipe_shown();
        if let Err(e) = clipboard::clear_clipboard() {
            eprintln!("clear clipboard failed: {e}");
        }
        self.signature = ClipSig::default();
        let view = ClipboardView::from_os();
        self.sync_icon(detected_kind(&view));
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
        let previous = self.history_cursor;
        self.history_cursor = next;
        let presented = self.present_history(next).unwrap_or_else(|e| {
            eprintln!("restore history failed: {e:#}");
            false
        });
        if !presented {
            self.history_cursor = previous;
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
        // Recording would move this entry to the front, so the other arrow
        // could no longer walk back through the list.
        self.card_view = CardView::Original;
        self.skip_record = Some(text.clone());
        if let Err(e) = clipboard::write_clipboard(text.as_str()) {
            self.skip_record = None;
            return Err(anyhow::Error::msg(e));
        }
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
            self.skip_record = None;
            self.opened = None;
            self.skip_image_change = Some(crate::macos_pasteboard::change_count());
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
            self.opened = None;
            self.show_restored();
            return Ok(());
        }
        let Some(text) = self
            .history
            .get(index)
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            return Ok(());
        };
        clipboard::write_clipboard(text.as_str()).map_err(anyhow::Error::msg)?;
        self.opened = None;
        self.show_restored();
        Ok(())
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
        }
    }

    fn note_clipboard(&mut self) -> bool {
        let view = ClipboardView::from_os();
        self.record_current(&view);
        #[cfg(target_os = "macos")]
        let image_change = crate::macos_pasteboard::change_count();
        #[cfg(not(target_os = "macos"))]
        let image_change = 0;
        let signature = ClipSig {
            text_hash: hash_text(view.text().unwrap_or("")),
            image: view.is_image(),
            image_change,
            history_len: self.history.labels().len(),
        };
        if signature == self.signature {
            return false;
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
        self.sync_icon(detected_kind(&view));
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

    fn sync_icon(&mut self, kind: Option<format::FormatKind>) {
        if kind == self.shown_kind {
            return;
        }
        self.shown_kind = kind;
        match icon::menu_icon_tinted(icon::accent_for_kind(kind)) {
            Ok(icon) => {
                if let Err(e) = self.tray.set_icon(Some(icon)) {
                    eprintln!("menu bar icon failed: {e}");
                }
            }
            Err(e) => eprintln!("menu bar icon failed: {e}"),
        }
    }

    fn sync_tooltip(&self, view: &ClipboardView) {
        if let Err(e) = self.tray.set_tooltip(Some(icon_tip(view))) {
            eprintln!("menu bar tooltip failed: {e}");
        }
    }
}

fn icon_tip(view: &ClipboardView) -> String {
    match view {
        ClipboardView::Image => "Image".to_string(),
        ClipboardView::Empty | ClipboardView::NoText => "Copycraft".to_string(),
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

fn detected_kind(view: &ClipboardView) -> Option<format::FormatKind> {
    match view {
        ClipboardView::Image => Some(format::FormatKind::Image),
        ClipboardView::Text(text) => Some(format::detect(text.as_str())),
        ClipboardView::Empty | ClipboardView::NoText => None,
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

/// The one place a failed save is reported, whether it failed before or on the save thread.
fn log_save_failure(message: &str) {
    eprintln!("save failed: {message}");
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
    let (hotkeys, format_hotkey_id) = register_format_hotkey()?;
    let icon = icon::menu_icon()?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(status_menu(&[])))
        .with_menu_on_left_click(false)
        .with_tooltip("Copycraft")
        .build()?;

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    launcher::install_proxy(event_loop.create_proxy());

    let mut app = App {
        tray,
        shown_kind: None,
        history: ClipboardHistory::default(),
        history_cursor: 0,
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
        _hotkeys: hotkeys,
        format_hotkey_id,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{detected_kind, status_labels, status_rows, version_label};
    use crate::clipboard::ClipboardView;
    use crate::format::FormatKind;
    use crate::hotkey;
    use crate::icon;
    use zeroize::Zeroizing;

    #[test]
    fn version_label_includes_package_version() {
        assert_eq!(
            version_label(),
            format!("Copycraft {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn icon_accent_follows_detected_content() {
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Empty)),
            None
        );
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Text(Zeroizing::new(
                "hello".into(),
            )))),
            FormatKind::Plain.accent_rgba()
        );
        let agents = "Notes from the review.\nThe document continues on this line.\n";
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Text(Zeroizing::new(
                agents.into(),
            )))),
            FormatKind::Text.accent_rgba()
        );
        assert_ne!(
            FormatKind::Text.accent_rgba(),
            FormatKind::Plain.accent_rgba()
        );
        assert_ne!(
            FormatKind::Text.accent_rgba(),
            FormatKind::Rust.accent_rgba()
        );
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Text(Zeroizing::new(
                r#"{"a":1}"#.into(),
            )))),
            FormatKind::Json.accent_rgba()
        );
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Text(Zeroizing::new(
                "fn main() {}".into(),
            )))),
            FormatKind::Rust.accent_rgba()
        );
        assert_eq!(
            icon::accent_for_kind(detected_kind(&ClipboardView::Image)),
            FormatKind::Image.accent_rgba()
        );
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
