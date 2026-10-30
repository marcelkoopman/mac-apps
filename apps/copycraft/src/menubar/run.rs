//! Starting the menu bar app.

use super::*;

/// The hotkey manager with the card's chord registered, or none (logged, and Settings says the
/// chord is taken) when another app or macOS holds it: the card still opens from the menu.
pub(crate) fn register_format_hotkey()
-> Result<(GlobalHotKeyManager, Option<global_hotkey::hotkey::HotKey>), Box<dyn std::error::Error>>
{
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = register_card_chord(&manager, crate::settings::hotkey());
    Ok((manager, hotkey))
}

/// Register the card's `chord` on `manager`: `Some` when that worked, else `None` (logged).
/// Either way [`crate::hotkey::set_taken`] records the outcome for Settings.
pub(crate) fn register_card_chord(
    manager: &GlobalHotKeyManager,
    chord: global_hotkey::hotkey::HotKey,
) -> Option<global_hotkey::hotkey::HotKey> {
    let registered = match manager.register(chord) {
        Ok(()) => {
            crate::diag::log(crate::diag::LogEvent::HotkeyRegistered);
            Some(chord)
        }
        Err(e) => {
            crate::diag::log(crate::diag::LogEvent::HotkeyRegisterFailed(
                crate::diag::ErrorKind::from_text(&e.to_string()),
            ));
            None
        }
    };
    crate::hotkey::set_taken(false, registered.is_none());
    registered
}

/// Register the screenshot hotkey on `manager`. `None` (logged, and marked taken for Settings)
/// when another app or macOS holds the chord: screenshots still run from the menu and search.
pub(crate) fn register_screenshot_hotkey(
    manager: &GlobalHotKeyManager,
) -> Option<global_hotkey::hotkey::HotKey> {
    let chord = crate::settings::screenshot_hotkey();
    // The card's hotkey was set to this chord before there were screenshots: the card keeps it.
    let registered = if chord == crate::settings::hotkey() {
        None
    } else {
        match manager.register(chord) {
            Ok(()) => Some(chord),
            Err(e) => {
                crate::diag::log(crate::diag::LogEvent::HotkeyRegisterFailed(
                    crate::diag::ErrorKind::from_text(&e.to_string()),
                ));
                None
            }
        }
    };
    crate::hotkey::set_taken(true, registered.is_none());
    registered
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    mac_ui::log::set_subsystem(crate::diag::SUBSYSTEM);
    crate::diag::log(crate::diag::LogEvent::Started);
    // First thing that touches the settings: a new install is told from an existing one by what
    // is stored before this build stores anything (see `history_mode::keep_history_default`).
    crate::settings::settle_clipboard_history();
    crate::locale::apply();
    // Before the event loop, so a click cannot race this wipe of a previous run's pictures.
    #[cfg(target_os = "macos")]
    crate::macos_open::remove_stale_preview_files();
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
        crate::diag::log(crate::diag::LogEvent::BackgroundStartSkipped(
            crate::diag::ErrorKind::from_text(&e.to_string()),
        ));
    }
    let (hotkeys, hotkey) = register_format_hotkey()?;
    let screenshot_hotkey = register_screenshot_hotkey(&hotkeys);
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
        .with_tooltip("CopyCraft")
        .build()?;
    // Icon only: name the button for VoiceOver. `sync_tooltip` adds the kind.
    mac_ui::tray::set_accessibility_label(&tray, &tray_label("CopyCraft"));
    // Above `tray-icon`'s view, so a click reaches us while that window is not key.
    #[cfg(target_os = "macos")]
    crate::macos_status_click::install(&tray);

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    // Do not set an activation policy here. The bundled app is an agent via LSUIElement in
    // Info.plist, and winit leaves that alone. Forcing Accessory on `cargo run` (no bundle)
    // orders the card but macOS keeps it occluded, so the click and the hotkey show nothing.
    let event_loop = builder.build()?;
    launcher::install_proxy(event_loop.create_proxy());
    crate::sensitivity::on_checked(|| launcher::emit(UserEvent::LabelsChecked));
    if crate::settings::welcome_due() {
        launcher::emit(UserEvent::ShowWelcome);
    }

    // Is Apple Intelligence usable? Asked in the background (cheap, but not on the main thread);
    // a warning follows once per session when it is not.
    if let Err(e) = std::thread::Builder::new()
        .name("copycraft-ai-check".into())
        .spawn(|| launcher::emit(UserEvent::AiChecked(mac_ai::availability())))
    {
        crate::diag::log(crate::diag::LogEvent::BackgroundStartSkipped(
            crate::diag::ErrorKind::from_text(&e.to_string()),
        ));
    }

    let mut app = App {
        tray,
        history: ClipboardHistory::with_gate(crate::capture::history_held),
        history_cursor: 0,
        clipboard_cursor: None,
        opened_superseded: false,
        current_image: None,
        recorded_image_change: None,
        reinstall_click: Cell::new(false),
        skip_image_change: None,
        skip_record: None,
        removal_skip_change: None,
        card_view: CardView::Original,
        opened: None,
        dropped_text_wait: Vec::new(),
        dropped_image_wait: Vec::new(),
        polled_image_wait: None,
        image_scan: None,
        image_scan_change: None,
        image_scan_for: None,
        signature: ClipSig::default(),
        saving: None,
        saving_diagnostics: false,
        loading_all: None,
        previewing: None,
        full_card: None,
        spinner_on: false,
        opening: commands::FileOpening::default(),
        opening_started: None,
        opening_shown: false,
        unzip_started: None,
        unzip_shown: false,
        blink: tray::Blink::default(),
        icons,
        hotkeys,
        format_hotkey: hotkey,
        screenshot_hotkey,
        shot_generation: 0,
        shot_change: None,
        sensitive_clear: None,
        conceal_when_checked: None,
        polled_change: None,
        poll_again: false,
        paste_explaining: false,
        paste_explained: crate::settings::paste_alert_explained(),
        strip_compare_used: crate::settings::strip_compare_used(),
        strip_hint_row: !crate::settings::strip_compare_used(),
        image_on_pasteboard: None,
        card_picture: None,
        card_picture_asked: None,
        unchecked_source: None,
        polled_unchecked: Vec::new(),
        image_gate_logged: None,
        last_copy: None,
        history_minutes: crate::settings::load().history_minutes,
        opened_table: crate::settings::preferred_table(),
        card_text: None,
        browsing: None,
        history_emptied: false,
        loose_table: crate::settings::preferred_table(),
        loose_text: None,
        table_job: None,
        table_error: None,
        table_window: None,
        decoded_picture: None,
        image_job: None,
        image_note: None,
        picture_probe: None,
        checksum: None,
        image_steps: None,
        text_steps: None,
        text_note: None,
        text_generation: 0,
        ai: None,
        ai_runs: 0,
        lanes: Default::default(),
        work_error: None,
        pin_refused: false,
        diff_sel: crate::diff::Selection::default(),
        dropped_pair: None,
        diff_generation: 0,
        diff_outcome: None,
        diffing: None,
        notices: Default::default(),
    };
    // Snippets kept from an earlier run come back as pinned snippets.
    app.load_snippets();
    #[cfg(target_os = "macos")]
    crate::macos_notify::init();
    #[cfg(target_os = "macos")]
    crate::macos_session::observe(
        || launcher::emit(UserEvent::SessionEnded),
        crate::appearance::restyle_open_windows,
    );
    // Pasteboard privacy (macOS 15.4+): the access setting, in the local log only.
    #[cfg(target_os = "macos")]
    crate::macos_pasteboard::log_access_behavior();
    event_loop.run_app(&mut app)?;
    Ok(())
}
