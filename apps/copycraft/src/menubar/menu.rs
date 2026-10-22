//! The status item menu.

use super::*;

/// "Allow screenshots" (testing): the card and the table window can be captured until it is
/// switched off or CopyCraft restarts.
pub(crate) const ALLOW_SCREENSHOTS_ID: &str = "allow-screenshots";
pub(crate) const PAUSE_ID: &str = "pause-recording";
pub(crate) const IGNORE_NEXT_ID: &str = "ignore-next";

pub(crate) const SETTINGS_ID: &str = "settings";
pub(crate) const WELCOME_ID: &str = "welcome";
pub(crate) const ABOUT_ID: &str = "about";

pub(crate) fn status_labels() -> [String; 3] {
    [
        crate::settings::hotkey_label(),
        version_label(),
        crate::locale::t("detail_quit").to_string(),
    ]
}

#[cfg(test)]
pub(crate) fn status_rows() -> Vec<String> {
    let [hotkey, version, quit] = status_labels();
    vec![
        hotkey,
        crate::locale::t("pause_recording").to_string(),
        crate::locale::t("ignore_next").to_string(),
        crate::locale::t("history").to_string(),
        crate::locale::t("allow_screenshots").to_string(),
        crate::locale::t("welcome_menu").to_string(),
        crate::locale::t("settings").to_string(),
        crate::locale::t("about").to_string(),
        version,
        quit,
    ]
}

/// *History ▸* can be opened when there is something in it and *Keep clipboard history* is on.
pub(crate) fn history_submenu_enabled(entries: usize, history_on: bool) -> bool {
    entries > 0 && history_on
}

/// The status menu, in groups with a separator between them (as the HIG asks): the hotkey
/// hint; recording (*Pause*, *Ignore Next Copy*, *History*); *Allow Screenshots*; the
/// windows (*Welcome Screen*, *Settings…* ⌘,); *About* and the version; *Quit CopyCraft* ⌘Q.
pub(crate) fn status_menu(entries: &[(usize, String)]) -> Menu {
    let [hotkey, version, quit] = status_labels();
    let menu = Menu::new();
    let _ = menu.append(&tray::info_item(&hotkey));
    let _ = menu.append(&PredefinedMenuItem::separator());
    // Greyed out while "Keep clipboard history" is off (nothing is read on its own then).
    let active = crate::history_mode::capture_controls_active(crate::settings::clipboard_history());
    let _ = menu.append(&CheckMenuItem::with_id(
        PAUSE_ID,
        crate::locale::t("pause_recording"),
        active,
        crate::capture::paused(),
        None,
    ));
    let _ = menu.append(&CheckMenuItem::with_id(
        IGNORE_NEXT_ID,
        crate::locale::t("ignore_next"),
        active,
        crate::capture::ignore_next(),
        None,
    ));
    // Always there, so the menu keeps its shape; greyed out with nothing to pick.
    let history_on = crate::settings::clipboard_history();
    let history = Submenu::new(
        crate::locale::t("history"),
        history_submenu_enabled(entries.len(), history_on),
    );
    if history_on {
        for (index, title) in entries {
            let _ = history.append(&MenuItem::with_id(
                format!("hist-{index}"),
                title,
                true,
                None,
            ));
        }
    }
    let _ = menu.append(&history);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&CheckMenuItem::with_id(
        ALLOW_SCREENSHOTS_ID,
        crate::locale::t("allow_screenshots"),
        true,
        launcher::allows_screenshots(),
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        WELCOME_ID,
        crate::locale::t("welcome_menu"),
        true,
        None,
    ));
    let _ = menu.append(&tray::settings_item(
        SETTINGS_ID,
        crate::locale::t("settings"),
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        ABOUT_ID,
        crate::locale::t("about"),
        true,
        None,
    ));
    let _ = menu.append(&tray::info_item(&version));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&tray::quit_item(&quit));
    menu
}
