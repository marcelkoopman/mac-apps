use super::{
    ClipSig, ICON_CLICK_GAP, card_returns_after_window, end_session_history, forget_opened_file,
    history_due, history_forgettable, history_submenu_enabled, icon_click_is_new, icon_tip,
    removal_skip_ends, status_labels, status_rows, tray_label, version_label,
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
    assert!(!forget_opened_file(true, true, false, true));
    assert!(forget_opened_file(false, true, false, true));
    // Open table hides the card and the window shows the chosen file.
    assert!(!forget_opened_file(false, true, true, true));
    assert!(!forget_opened_file(false, false, true, true));
}

#[test]
fn a_hidden_file_card_stays_until_new_clipboard_content_comes_in() {
    // Hidden by the icon, the hotkey or Esc: the poll runs, nothing new on the clipboard.
    assert!(!forget_opened_file(false, true, false, false));
    // Another app copies something: the file gives way when the card is closed.
    assert!(forget_opened_file(false, true, false, true));
    // Still open: never dropped under the user.
    assert!(!forget_opened_file(true, true, false, true));
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
fn a_lock_forgets_the_copy_kept_for_undo_also_with_history_empty() {
    // One copy, removed with ⋯ › Remove from History: history is empty, ⌘Z could bring it back.
    let mut history = crate::clipboard::ClipboardHistory::default();
    history.record("only".into());
    let only = history.id_at(0).expect("only");
    history.take_id(only);
    assert!(history.is_empty() && history.can_restore());
    // The screen locks: nothing left to wipe, and ⌘Z restores nothing afterwards.
    assert!(!end_session_history(&mut history));
    assert!(!history.can_restore());
    assert_eq!(history.restore_removed(), None);
    assert!(history.is_empty());
    // With entries left, they are wiped too ([`App::forget_entries`]), and the kept copy is gone.
    history.record("one".into());
    history.record("two".into());
    let two = history.id_at(0).expect("two");
    history.take_id(two);
    assert!(end_session_history(&mut history));
    assert!(!history.can_restore());
}

#[test]
fn a_removed_newest_copy_is_recorded_again_after_a_new_copy() {
    // Removed at change 7: the poll still sees 7 and does not record it again.
    assert!(!removal_skip_ends(Some(7), 7));
    // The same text copied again is change 8: the skip ends and the copy is recorded.
    assert!(removal_skip_ends(Some(7), 8));
    // No removal: nothing to end (another skip, e.g. after Wipe, stays as it was).
    assert!(!removal_skip_ends(None, 8));
}

#[test]
fn the_time_limit_clears_a_removed_copy_next_to_pins_only() {
    let copied = std::time::Instant::now();
    let mut history = crate::clipboard::ClipboardHistory::default();
    history.record("pinned".into());
    history.record("gone".into());
    let pinned = history.id_at(1).expect("pinned");
    assert!(history.pin(pinned));
    let gone = history.id_at(0).expect("gone");
    history.take_id(gone);
    // Only a pin and the copy kept for ⌘Z: the timer still runs.
    assert!(!history.has_unpinned());
    assert!(history_forgettable(&history));
    assert!(history_due(Some(copied), 5, history_forgettable(&history)).is_some());
    // When it fires (App::clear_history keeps pins), the kept copy goes and the pin stays.
    history.clear_unpinned();
    assert!(!history.can_restore());
    assert_eq!(history.restore_removed(), None);
    assert_eq!(history.len(), 1);
    assert!(history.is_pinned(0));
    // Then nothing is left to forget: no timer.
    assert!(!history_forgettable(&history));
    assert_eq!(
        history_due(Some(copied), 5, history_forgettable(&history)),
        None
    );
}

#[test]
fn version_label_includes_package_version() {
    assert_eq!(
        version_label(),
        format!("CopyCraft {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn tooltip_and_voiceover_label_name_the_kind() {
    let text = |s: &str| ClipboardView::Text(Zeroizing::new(s.into()));
    assert_eq!(icon_tip(&ClipboardView::Empty), "CopyCraft");
    assert_eq!(icon_tip(&ClipboardView::NoText), "CopyCraft");
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
        assert_eq!(tray_label(&tip), format!("CopyCraft, {tip}"));
    }
    assert_eq!(tray_label("CopyCraft"), "CopyCraft");
    assert_eq!(tray_label("Image"), "CopyCraft, Image");
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

/// "Allow Screenshots" is off at every start; on, the card and the table window are
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
            "Quit CopyCraft".to_string(),
        ]
    );
}

#[test]
fn status_menu_lists_history_between_the_hotkey_and_version() {
    assert_eq!(
        status_rows(),
        vec![
            crate::settings::hotkey_label(),
            crate::locale::t("pause_recording").to_string(),
            crate::locale::t("ignore_next").to_string(),
            crate::locale::t("history").to_string(),
            crate::locale::t("allow_screenshots").to_string(),
            crate::locale::t("welcome_menu").to_string(),
            crate::locale::t("settings").to_string(),
            crate::locale::t("about").to_string(),
            version_label(),
            crate::locale::t("detail_quit").to_string(),
        ]
    );
}

/// *History ▸* is always in the menu; it opens only with entries and history on.
#[test]
fn the_history_submenu_is_greyed_out_when_empty_or_off() {
    assert!(history_submenu_enabled(2, true));
    assert!(!history_submenu_enabled(0, true));
    assert!(!history_submenu_enabled(2, false));
    assert!(!history_submenu_enabled(0, false));
}

#[test]
fn chosen_file_survives_open_table_and_returns_when_the_window_closes() {
    // Kies bestand…: the file is on the card (and in history, so its table lives there).
    // Open table closes the card; the window keeps the file, so it is not forgotten.
    let (file_open, window_keeps_file) = (true, true);
    assert!(!forget_opened_file(
        false,
        file_open,
        window_keeps_file,
        true
    ));
    // The window closes: the card comes back with the file, before the file could be dropped.
    assert!(card_returns_after_window(
        window_keeps_file,
        file_open,
        false
    ));
    assert!(!forget_opened_file(true, file_open, false, true));
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

#[test]
fn browsing_history_ends_only_with_a_new_copy() {
    use super::state::browse_ends;
    // The same pasteboard (a poll, a background refresh): the card keeps the browsed item.
    assert!(!browse_ends(Some(7), 7, false));
    // CopyCraft's own write (Copy) is not a new copy from elsewhere.
    assert!(!browse_ends(Some(7), 8, true));
    // Another app copied: the card follows the clipboard again.
    assert!(browse_ends(Some(7), 8, false));
    assert!(!browse_ends(None, 8, false));
}

#[test]
fn history_steps_through_three_entries_both_ways() {
    use super::state::is_new_copy;
    use crate::commands::{history_nav, step_history};
    let label = |cursor: usize| {
        let nav = history_nav(3, cursor).expect("nav");
        format!("{} / {}", nav.position, nav.total)
    };
    // The clipboard's item read again by a poll is not a new copy: browsing stays put.
    assert!(!is_new_copy(Some("newest"), "newest"));
    assert!(is_new_copy(Some("newest"), "another"));
    let mut cursor = 0;
    assert_eq!(label(cursor), "1 / 3");
    for (want, shown) in [(1, "2 / 3"), (2, "3 / 3")] {
        cursor = step_history(3, cursor, true).expect("older");
        assert_eq!((cursor, label(cursor).as_str()), (want, shown));
    }
    assert_eq!(step_history(3, cursor, true), None, "clamped at the oldest");
    for (want, shown) in [(1, "2 / 3"), (0, "1 / 3")] {
        cursor = step_history(3, cursor, false).expect("newer");
        assert_eq!((cursor, label(cursor).as_str()), (want, shown));
    }
    assert_eq!(
        step_history(3, cursor, false),
        None,
        "clamped at the newest"
    );
}

#[test]
fn a_save_confirms_like_copy_and_a_failure_says_why() {
    use super::state::save_toast;
    assert_eq!(save_toast(&Ok(())), "Saved");
    assert_eq!(
        save_toast(&Err("Permission denied (os error 13)\ncaused by…".into())),
        "Save failed: Permission denied (os error 13)"
    );
    assert_eq!(save_toast(&Err(String::new())), "Save failed");
}

#[test]
fn lines_works_on_the_text_the_card_was_built_from() {
    use crate::commands::SubjectKind;
    let text = "fn main() {\n    let v = 1;\n}\n";
    let data = crate::commands::tests::data(SubjectKind::Text, Some(text));
    assert_eq!(
        super::text::card_text_of(&data)
            .as_deref()
            .map(String::as_str),
        Some(text)
    );
    // A card still waiting for its copy has no text for Text ▾ yet; the next build sets it.
    let pending = crate::commands::tests::data(SubjectKind::Pending, None);
    assert!(super::text::card_text_of(&pending).is_none());
}

#[test]
fn steps_on_an_opened_file_are_versions_of_the_file() {
    use super::text::OpenedSteps;
    let mut steps = OpenedSteps::new("fn a() {}\nfn b() {}\n");
    steps.push("First 1 lines".into(), "fn a() {}\n");
    steps.push("Reversed".into(), "fn a() {}");
    assert_eq!(steps.cursor, 2);
    // ⌘Z twice: back to the file's own text; once more does nothing.
    assert_eq!(steps.go(|c| c.checked_sub(1)), Some("fn a() {}\n"));
    assert_eq!(
        steps.go(|c| c.checked_sub(1)),
        Some("fn a() {}\nfn b() {}\n")
    );
    assert_eq!(steps.go(|c| c.checked_sub(1)), None);
    // A new step from the original drops the redo.
    steps.push("Reversed".into(), "fn b() {}\nfn a() {}\n");
    assert_eq!(steps.texts.len(), 2);
    assert_eq!(steps.labels[0], crate::locale::t("original"));
    assert_eq!(steps.go(|c| Some(c + 1)), None);
    assert_eq!(steps.go(|_| Some(0)), Some("fn a() {}\nfn b() {}\n"));
}
