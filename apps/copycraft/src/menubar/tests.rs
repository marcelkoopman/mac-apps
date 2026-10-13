use super::{
    ClipSig, ICON_CLICK_GAP, card_returns_after_window, forget_opened_file, history_due,
    icon_click_is_new, icon_tip, status_labels, status_rows, tray_label, version_label,
};
use crate::clipboard::ClipboardView;
use crate::format::FormatKind;
use zeroize::Zeroizing;

#[test]
fn a_second_delivery_of_one_icon_click_does_not_toggle_again() {
    let now = std::time::Instant::now();
    assert!(icon_click_is_new(None, now));
    assert!(!icon_click_is_new(Some(now), now));
    assert!(!icon_click_is_new(
        Some(now),
        now + ICON_CLICK_GAP - std::time::Duration::from_millis(1)
    ));
    assert!(icon_click_is_new(Some(now), now + ICON_CLICK_GAP));
}

#[test]
fn a_closed_card_keeps_a_file_while_its_table_window_is_open() {
    assert!(!forget_opened_file(true, true, false));
    assert!(forget_opened_file(false, true, false));
    // Open table hides the card and the window shows the chosen file.
    assert!(!forget_opened_file(false, true, true));
    assert!(!forget_opened_file(false, false, true));
}

#[test]
fn history_is_forgotten_minutes_after_the_last_copy() {
    let copied = std::time::Instant::now();
    assert_eq!(
        history_due(Some(copied), 15, true),
        Some(copied + std::time::Duration::from_secs(15 * 60))
    );
    assert_eq!(history_due(Some(copied), 0, true), None);
    assert_eq!(history_due(Some(copied), 5, false), None);
    assert_eq!(history_due(None, 5, true), None);
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
            crate::locale::t("pause_recording").to_string(),
            crate::locale::t("ignore_next").to_string(),
            crate::locale::t("history").to_string(),
            crate::locale::t("allow_screenshots").to_string(),
            crate::locale::t("settings").to_string(),
            crate::locale::t("welcome_menu").to_string(),
            crate::locale::t("about").to_string(),
            version_label(),
            crate::locale::t("quit").to_string(),
        ]
    );
}

#[test]
fn chosen_file_survives_open_table_and_returns_when_the_window_closes() {
    // Kies bestand…: the file is on the card (and in history, so its table lives there).
    // Open table closes the card; the window keeps the file, so it is not forgotten.
    let (file_open, window_keeps_file) = (true, true);
    assert!(!forget_opened_file(false, file_open, window_keeps_file));
    // The window closes: the card comes back with the file, before the file could be dropped.
    assert!(card_returns_after_window(
        window_keeps_file,
        file_open,
        false
    ));
    assert!(!forget_opened_file(true, file_open, false));
    // A window opened from a history entry (no file) does not bring the card back.
    assert!(!card_returns_after_window(false, false, false));
    // The card already open: nothing to do.
    assert!(!card_returns_after_window(true, true, true));
}

#[test]
fn every_table_on_the_card_has_a_home_for_open_table() {
    use super::app::{TableHome, pick_table_home};
    assert_eq!(pick_table_home(Some(3), false), TableHome::History(3));
    assert_eq!(pick_table_home(Some(0), true), TableHome::History(0));
    assert_eq!(pick_table_home(None, true), TableHome::Opened);
    // Not in history (paused, ignored app, sensitive copy): still a table to open.
    assert_eq!(pick_table_home(None, false), TableHome::Loose);
}

#[test]
fn a_browsed_version_is_shown_without_the_clipboard() {
    use super::app::browsed_entry;
    assert_eq!(browsed_entry(Some(2), false), Some(2));
    // A chosen file on the card wins; a version history let go shows the clipboard.
    assert_eq!(browsed_entry(Some(2), true), None);
    assert_eq!(browsed_entry(None, false), None);
}
