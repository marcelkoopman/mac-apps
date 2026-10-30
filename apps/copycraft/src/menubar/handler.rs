//! The winit application handler: event loop callbacks.

use super::*;

impl ApplicationHandler<UserEvent> for App {
    /// Once, right after launch: winit's app menu exists by now, so Edit and Window go next
    /// to it.
    fn resumed(&mut self, _: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        {
            // The reopen handler first: the main menu aims Quit at it.
            crate::macos_reopen::install();
            crate::macos_main_menu::install();
        }
    }

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
            UserEvent::ToggleUnderIcon => self.toggle_popup_under_icon(),
            UserEvent::ShowStatusMenu => self.show_status_menu(),
            UserEvent::Reopen => self.reopen_card(),
            UserEvent::ShowWelcome => launcher::show_welcome(),
            UserEvent::AiChecked(availability) => self.ai_startup_checked(availability),
            UserEvent::ImageScanned { change, scan } => self.finish_image_scan(change, scan),
            UserEvent::SaveFinished(result) => self.finish_save(result),
            UserEvent::PreviewReady { generation, result } => {
                self.finish_preview(generation, result);
            }
            UserEvent::PreviewOpened { generation, opened } => {
                self.preview_opened(generation, opened);
            }
            UserEvent::FullCardReady(card) => self.finish_show_all(card),
            UserEvent::DroppedFile(path) => self.open_path(path, false),
            UserEvent::DroppedFiles(paths) => self.compare_dropped_files(paths),
            UserEvent::DroppedPromisedFile(path) => self.open_path(path, true),
            UserEvent::DroppedImage(bytes) => {
                self.show_dropped(crate::open_file::from_image_data(dropped_image(), bytes));
            }
            UserEvent::DroppedText(text) => {
                self.show_dropped(crate::open_file::from_text(dropped_text(), text));
            }
            UserEvent::ColorPicked(value) => {
                self.new_value(crate::locale::t("generate_color"), value);
                // The loupe can close the card (it takes the focus): open it again on the value.
                if !launcher::is_open() {
                    self.reopen_card();
                }
            }
            UserEvent::Pasted(pasted) => self.show_pasted(pasted),
            UserEvent::ClipboardHistoryChanged => self.clipboard_history_changed(),
            UserEvent::DecodedPicture { key, picture } => self.finish_decoded_picture(key, picture),
            UserEvent::DroppedImageScanned { image, scan } => {
                self.finish_dropped_scan(&image, scan);
            }
            UserEvent::PdfPageRendered {
                source,
                index,
                page,
            } => self.finish_pdf_page(&source, index, page),
            UserEvent::PdfZoomRendered {
                source,
                index,
                zoom,
                page,
            } => self.finish_pdf_zoom(&source, index, zoom, page),
            UserEvent::LabelsChecked => self.labels_checked(),
            // Only the tooltip: an open card reads the copy anyway.
            UserEvent::PasteHint => self.sync_tooltip(&ClipboardView::from_os()),
            UserEvent::SessionEnded => self.session_ended(),
            UserEvent::ScreenshotTaken { generation, result } => {
                self.screenshot_taken(generation, result);
            }
            UserEvent::ScreenshotOpened => self.open_screenshot(),
            UserEvent::TableDone(done) => self.finish_table_job(*done),
            UserEvent::TableWindow(event) => self.table_window_event(event),
            UserEvent::SettingsChanged => {
                if launcher::is_open() {
                    self.refresh_popup();
                }
            }
            UserEvent::PrivacyFilterChanged => launcher::privacy_filter_changed(),
            UserEvent::HotkeyChanged => self.rebind_hotkey(),
            UserEvent::LanguageChanged => self.language_changed(),
            UserEvent::ImageDone(done) => self.finish_image_job(*done),
            UserEvent::DiffReady {
                generation,
                outcome,
            } => self.finish_diff(generation, *outcome),
            UserEvent::WorkDone(done) => self.finish_work(done),
            UserEvent::TextReady {
                generation,
                entry,
                result,
                found,
                source_hash,
            } => self.finish_text(generation, entry, *result, found, source_hash),
            UserEvent::Notification(event) => self.notification_event(event),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A right-click menu clears the status button's action. Put it back before the next
        // click, only after a menu was shown (not on every tick).
        #[cfg(target_os = "macos")]
        if self.reinstall_click.take() {
            crate::macos_status_click::install(&self.tray);
        }
        self.clear_notices_when_card_opens();
        // A closed card forgets the file, so the next hotkey shows the clipboard.
        // Open table closes the card on purpose; that window keeps the file until it closes.
        if forget_opened_file(
            launcher::is_open(),
            self.opened.is_some(),
            self.table_window
                .as_ref()
                .is_some_and(|window| window.opened || window.keeps_file),
            self.opened_superseded,
        ) {
            self.leave_opened();
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                tray::QUIT_ID => {
                    self.end_unzip_for_quit();
                    before_quit();
                    event_loop.exit();
                    return;
                }
                ALLOW_SCREENSHOTS_ID => {
                    let on = !launcher::allows_screenshots();
                    launcher::set_allow_screenshots(on);
                    crate::diag::log(crate::diag::LogEvent::ScreenshotsAllowed(on));
                    self.refresh_status_menu();
                }
                SCREENSHOT_ID => self.run_command(event_loop, CommandId::TakeScreenshot),
                PAUSE_ID => self.run_command(event_loop, CommandId::Pause),
                IGNORE_NEXT_ID => self.run_command(event_loop, CommandId::IgnoreNext),
                SETTINGS_ID => {
                    self.run_command(event_loop, CommandId::Settings);
                }
                WELCOME_ID => launcher::show_welcome(),
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
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if Some(event.id) == self.format_hotkey.map(|chord| chord.id()) {
                self.summon_popup();
            } else if Some(event.id) == self.screenshot_hotkey.map(|chord| chord.id()) {
                self.take_screenshot();
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
        if changed && self.opening.active() {
            // A file still on its way is stale now: the new copy takes the card.
            self.cancel_opening();
        }
        if changed && launcher::is_open() {
            let data = self.current_launch_data();
            self.sync_popup(data);
        }
        if changed {
            // A new copy can push the window's entry off the end of history.
            self.refresh_table_window();
        }
        let wake = self.spin_slow_work(now);
        let preview_wake = self.check_preview_wait(now);
        let blink_wake = self.show_blink(now);
        let clear_wake = self.clear_sensitive_when_due(now);
        let forget_wake = self.forget_history_when_due(now);
        let opening_wake = self.spin_opening(now);
        let unzip_wake = self.spin_unzip(now);
        event_loop.set_control_flow(mac_ui::wake::control_flow([
            Some(wake),
            opening_wake,
            unzip_wake,
            blink_wake,
            clear_wake,
            forget_wake,
            preview_wake,
        ]));
    }
}
