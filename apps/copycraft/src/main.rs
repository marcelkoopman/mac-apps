mod agent;
mod appearance;
mod avro_schema;
mod brackets;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod capture;
mod card_layout;
mod card_theme;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod chart_data;
mod checksum;
mod clipboard;
mod color;
mod column_picker;
mod commands;
mod complexity;
mod convert;
mod dataframe;
mod decode;
mod diag;
mod diff;
mod docx;
#[cfg(test)]
mod error_display_tests;
mod format;
mod generate;
mod highlight;
mod history_mode;
mod hotkey;
mod icon;
mod image_edit;
mod image_ops;
mod item_find;
mod launcher;
mod link_card;
mod link_meaning;
mod link_view;
mod locale;
mod log_text;
mod mailto;
mod memo;
mod menubar;
mod notifications;
mod open_file;
mod page_preview;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod paste_access;
mod pdf;
mod preview_wait;
mod python;
mod qr_code;
mod recognize;
mod screenshot;
mod sensitivity;
mod settings;
mod settings_tabs;
mod snippet;
mod snippet_store;
mod symbols;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod system_settings;
mod table;
mod table_ops;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod table_profile;
#[cfg(test)]
mod testdata_manifest;
mod text_ops;
mod toolbar_visibility;
mod transform;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod url_policy;
mod validate;
mod wipe_alloc;
mod work;
mod xsd_schema;
mod youtube;
mod zip_archive;

/// Freed heap memory is zeroed first ([`wipe_alloc`]), so copied text left in buffers of
/// Polars, AppKit or any other library does not stay readable after use.
#[global_allocator]
static ALLOC: wipe_alloc::WipeOnFree = wipe_alloc::WipeOnFree;

#[cfg(target_os = "macos")]
mod macos_about;
#[cfg(target_os = "macos")]
mod macos_card_text;
#[cfg(target_os = "macos")]
mod macos_chart;
#[cfg(target_os = "macos")]
mod macos_color_pick;
#[cfg(target_os = "macos")]
mod macos_feedback;
#[cfg(target_os = "macos")]
mod macos_image_edit;
#[cfg(target_os = "macos")]
mod macos_image_io;
#[cfg(target_os = "macos")]
mod macos_launcher;
#[cfg(target_os = "macos")]
mod macos_login;
#[cfg(target_os = "macos")]
mod macos_main_menu;
#[cfg(target_os = "macos")]
mod macos_notify;
#[cfg(target_os = "macos")]
mod macos_open;
#[cfg(target_os = "macos")]
mod macos_pasteboard;
#[cfg(target_os = "macos")]
mod macos_pdf;
#[cfg(target_os = "macos")]
mod macos_pin;
#[cfg(target_os = "macos")]
mod macos_preview_image;
#[cfg(target_os = "macos")]
mod macos_qr;
#[cfg(target_os = "macos")]
mod macos_reopen;
#[cfg(target_os = "macos")]
mod macos_save;
#[cfg(target_os = "macos")]
mod macos_session;
#[cfg(target_os = "macos")]
mod macos_settings;
#[cfg(target_os = "macos")]
mod macos_status_click;
#[cfg(target_os = "macos")]
mod macos_strip_parts;
#[cfg(target_os = "macos")]
mod macos_symbols;
#[cfg(target_os = "macos")]
mod macos_table_window;
#[cfg(target_os = "macos")]
mod macos_vision;
#[cfg(target_os = "macos")]
mod macos_welcome;

/// The app's panic hook: silent. Rust's default hook writes the panic's message and its
/// source location to stderr, and a message can quote copied text (an index into a string, a
/// value that did not parse). This one writes nothing at all, nowhere: no stderr, no log. Where
/// a panic is caught ([`std::panic::catch_unwind`]) the caller logs its own content-free event
/// and shows its own message. Installed by `main` only, so `cargo test` keeps its panic output.
fn silent_panic(_info: &std::panic::PanicHookInfo<'_>) {}

fn main() {
    // Polars prints 10 rows and 8 columns of a table, in at most 100 characters, by default.
    // The card limits rows itself (`commands::PREVIEW_ROWS`) and wants every column.
    for (name, value) in dataframe::DISPLAY_ENV {
        if std::env::var_os(name).is_none() {
            // SAFETY: first thing in `main`, before any other thread exists.
            unsafe { std::env::set_var(name, value) };
        }
    }
    // Before any other thread exists: no panic of the app ever writes its message.
    std::panic::set_hook(Box::new(silent_panic));
    if let Err(e) = menubar::run() {
        crate::diag::log(crate::diag::LogEvent::StartFailed(
            crate::diag::ErrorKind::from_text(&e.to_string()),
        ));
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    /// `main` installs the silent hook before it starts the app, and the hook writes nothing:
    /// its body is empty. Tests never install it (they need their panic messages).
    #[test]
    fn main_installs_a_silent_panic_hook() {
        let source = include_str!("main.rs");
        let main_at = source.find("\nfn main() {").expect("fn main");
        let main_body = &source[main_at..];
        let hook = main_body
            .find("std::panic::set_hook(Box::new(silent_panic));")
            .expect("main installs the hook");
        assert!(hook < main_body.find("menubar::run()").expect("the app starts"));
        assert!(
            source.contains("\nfn silent_panic(_info: &std::panic::PanicHookInfo<'_>) {}\n"),
            "the hook is empty"
        );
        // It has the signature set_hook takes.
        let _: fn(&std::panic::PanicHookInfo<'_>) = super::silent_panic;
    }

    #[test]
    fn crate_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "copycraft");
    }

    /// The signed app gets exactly these rights: the sandbox and files the user picks. No
    /// network (copycraft works offline), no hardened-runtime exceptions
    /// (`com.apple.security.cs.*`), no Apple Events. scripts/sign_app.sh and the release
    /// workflow check the signed app the same way.
    #[test]
    fn entitlements_are_the_sandbox_and_user_selected_files_only() {
        let plist = include_str!("../assets/entitlements.plist");
        let body = &plist[plist.find("<plist").expect("plist element")..];
        let keys: Vec<&str> = body
            .split("<key>")
            .skip(1)
            .filter_map(|rest| rest.split_once("</key>").map(|(key, _)| key.trim()))
            .collect();
        assert_eq!(
            keys,
            [
                "com.apple.security.app-sandbox",
                "com.apple.security.files.user-selected.read-write",
            ]
        );
        assert_eq!(body.matches("<true/>").count(), 2);
        assert!(!body.contains("<false/>") && !body.contains("<array>"));
    }

    /// The Mac App Store build (scripts/build_store_pkg.sh) adds only the two identifier keys
    /// the provisioning profile must match, with the TEAMID placeholder the script fills in.
    #[test]
    fn store_entitlements_add_only_the_identifiers() {
        let plist = include_str!("../assets/entitlements.store.plist");
        let body = &plist[plist.find("<plist").expect("plist element")..];
        let keys: Vec<&str> = body
            .split("<key>")
            .skip(1)
            .filter_map(|rest| rest.split_once("</key>").map(|(key, _)| key.trim()))
            .collect();
        assert_eq!(
            keys,
            [
                "com.apple.security.app-sandbox",
                "com.apple.security.files.user-selected.read-write",
                "com.apple.application-identifier",
                "com.apple.developer.team-identifier",
            ]
        );
        assert_eq!(body.matches("<true/>").count(), 2);
        assert!(body.contains("<string>TEAMID.nl.marcelkoopman.copycraft</string>"));
        assert!(body.contains("<string>TEAMID</string>"));
        assert!(!body.contains("<false/>") && !body.contains("<array>"));
    }

    /// While a large table's search runs on its thread, the grid on screen may be of the
    /// version before (a switch, a × remove): no row may be read from it. The wait forgets the
    /// drawn rows' mapping and dims the grid, the row menu stays shut, and the drawn result
    /// (or a closed window) ends the wait.
    #[test]
    fn a_pending_table_search_maps_no_rows() {
        let paint = include_str!("macos_table_paint.rs");
        let grid = &paint[paint.find("fn paint_table_grid").unwrap()..];
        let wait = &grid[grid.find("let Ok(filtered) = search_sheet").unwrap()..];
        let wait = &wait[..wait.find("return;").unwrap()];
        assert!(wait.contains("SHOWN_ROWS.with(|slot| slot.replace(None));"));
        assert!(wait.contains("switching::search_pending(true);"));
        assert!(grid.contains("switching::search_pending(false);"));
        let forget = &paint[paint.find("fn forget_thread_search").unwrap()..];
        assert!(forget[..forget.find("\n}\n").unwrap()].contains("search_pending(false)"));
        let window = include_str!("macos_table_window/mod.rs");
        let menu = &window[window.find("fn cell_menu").unwrap()..];
        let menu = &menu[..menu.find("SHOWN_ROWS").unwrap()];
        assert!(menu.contains("switching::searching()"));
        let switching = include_str!("macos_table_window/switching.rs");
        let reset = &switching[switching.find("pub(super) fn reset()").unwrap()..];
        assert!(reset[..reset.find("\n}\n").unwrap()].contains("SEARCHING.set(false);"));
    }

    /// Quit must not crash (EXC_BAD_ACCESS in `-[NSThemeFrame dealloc]` on ⌘Q): the titled
    /// windows hand their last reference to `release_after_close` instead of dropping it in
    /// `windowWillClose:`, and ⌘Q and the quit Apple event quit through the event loop, not
    /// through `terminate:` (winit then closes every window of the app).
    #[test]
    fn quit_and_close_do_not_release_a_closing_window() {
        for (name, src) in [
            ("macos_about.rs", include_str!("macos_about.rs")),
            ("macos_settings.rs", include_str!("macos_settings.rs")),
            ("macos_welcome.rs", include_str!("macos_welcome.rs")),
        ] {
            let close = &src[src.find("fn window_will_close").expect(name)..];
            let close = &close[..close.find("\n    }\n").expect(name)];
            assert!(close.contains("release_after_close("), "{name}");
            assert!(!close.contains("= None;\n            });"), "{name}");
        }
        let menu = include_str!("macos_main_menu.rs");
        assert!(menu.contains("aim_quit(&main);"));
        let reopen = include_str!("macos_reopen.rs");
        assert!(reopen.contains("#[unsafe(method(terminate:))]"));
        assert!(reopen.contains("andEventID: QUIT_APPLICATION"));
        assert!(!reopen.contains(".terminate("));
        // Every exit holds the windows through winit's close-all first.
        for (name, src) in [
            ("dispatch.rs", include_str!("menubar/dispatch.rs")),
            ("handler.rs", include_str!("menubar/handler.rs")),
        ] {
            assert_eq!(
                src.matches("event_loop.exit();").count(),
                src.matches("before_quit();\n").count(),
                "{name}"
            );
        }
        let state = include_str!("menubar/state.rs");
        assert!(state.contains("mac_ui::panel::hold_windows_for_exit(mtm);"));
    }

    /// The card's preview, the table header and the frozen column are wiped with the shared
    /// helper and filled again after: it must put the storage's layout managers back from a
    /// copy taken before any came off, or those views draw nothing any more (the card showed
    /// an empty preview after a large table closed).
    #[test]
    fn the_wipe_helper_puts_the_layout_managers_back() {
        let src = include_str!("../../../crates/mac-ui/src/widgets.rs");
        let wipe = &src[src.find("pub fn wipe_text_view").unwrap()..];
        let wipe = &wipe[..wipe.find("\n}\n").unwrap()];
        let copy = wipe.find("storage.layoutManagers().to_vec();").unwrap();
        let detached = wipe.find("crate::detach::while_detached(").unwrap();
        assert!(copy < detached);
        assert_eq!(wipe.matches("removeLayoutManager").count(), 1);
        assert_eq!(wipe.matches("addLayoutManager").count(), 1);
        assert!(!wipe.contains(".iter()"));
        for (name, src) in [
            ("events.rs", include_str!("macos_launcher/events.rs")),
            ("panels.rs", include_str!("macos_launcher/panels.rs")),
        ] {
            assert!(src.contains("widgets::wipe_text_view(view);"), "{name}");
        }
    }

    /// Closing a large table answers at once: the window leaves the screen first, keeps its
    /// window (no release in `windowWillClose:`, see above), and its view and laid-out sheet
    /// are wiped off the main thread.
    #[test]
    fn the_table_window_closes_first_and_wipes_after() {
        let src = include_str!("macos_table_window/mod.rs");
        let close = &src[src.find("fn window_will_close").unwrap()..];
        let close = &close[..close.find("\n        }\n").unwrap()];
        let out = close.find("order_out();").unwrap();
        assert!(out < close.find("forget();").unwrap());
        assert!(!close.contains("VIEWS.with"));
        let forget = &src[src.find("\nfn forget()").unwrap()..];
        let forget = &forget[..forget.find("\n}\n").unwrap()];
        assert!(forget.contains("crate::work::wipe_later((view, sheet));"));
        assert!(!forget.contains("\n    SHOWN.with(") && !forget.contains("\n    PREPARED.with("));
    }
}
