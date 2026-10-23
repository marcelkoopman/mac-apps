use super::*;

#[test]
fn clear_stays_on_the_open_card() {
    assert!(keeps_card_open(&CommandId::Clear));
}

#[test]
fn chip_width_follows_the_measured_pill() {
    assert_eq!(chip_width(71.2), 72.0);
    assert_eq!(chip_width(120.0), 120.0);
    assert_eq!(chip_width(30.0), crate::commands::CHIP_MIN_W);
    assert_eq!(chip_width(0.0), crate::commands::CHIP_MIN_W);
    assert_eq!(chip_width(f64::NAN), crate::commands::CHIP_MIN_W);
    assert_eq!(chip_width(-5.0), crate::commands::CHIP_MIN_W);
    assert_eq!(chip_width(900.0), crate::commands::CHIP_MAX_W);
    assert_eq!(chip_width(f64::INFINITY), crate::commands::CHIP_MAX_W);
}

#[test]
fn chips_keep_their_measured_widths() {
    let widths = [chip_width(61.5), chip_width(88.0), chip_width(40.0)];
    let frames = layout_chips(&widths, 412.0, 0.0);
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|frame| frame.row == 0));
    assert_eq!(frames[0].x, 0.0);
    assert_eq!(frames[0].width, 62.0);
    assert_eq!(frames[1].x, 62.0 + crate::commands::CHIP_GAP);
    assert_eq!(frames[1].width, 88.0);
    assert_eq!(frames[2].x, 62.0 + 88.0 + 2.0 * crate::commands::CHIP_GAP);
    assert_eq!(frames[2].width, crate::commands::CHIP_MIN_W);
}

#[test]
fn a_chip_wider_than_the_row_gets_its_own_row() {
    let frames = layout_chips(&[80.0, 220.0, 80.0], 200.0, 0.0);
    assert_eq!(frames[0].row, 0);
    assert_eq!((frames[1].row, frames[1].x), (1, 0.0));
    assert_eq!((frames[2].row, frames[2].x), (2, 0.0));
    assert!(layout_chips(&[], 200.0, 0.0).is_empty());
}

#[test]
fn chips_wrap_and_arrows_move_between_pills() {
    let widths = [78.0, 72.0, 86.0, 56.0, 74.0, 80.0];
    let frames = layout_chips(&widths, 220.0, 0.0);
    assert!(frames.len() == widths.len());
    assert_eq!(frames[0].row, 0);
    assert!(frames.last().unwrap().row > 0);
    assert_eq!(step_chip(&frames, 0, 1, 0), 1);
    assert_eq!(step_chip(&frames, 0, -1, 0), 0);
    let down = step_chip(&frames, 0, 0, 1);
    assert_ne!(frames[down].row, frames[0].row);
    assert_eq!(step_chip(&frames, down, 0, -1), 0);
    assert_eq!(step_chip(&[], 0, 1, 0), 0);
    // No row that way: ↑ / ↓ go to the previous / next chip, and stop at the ends.
    let last = frames.len() - 1;
    assert_eq!(step_chip(&frames, last, 0, 1), last);
    assert_eq!(step_chip(&frames, 0, 0, -1), 0);
    let one_row = layout_chips(&[60.0, 60.0, 60.0], 400.0, 0.0);
    assert!(one_row.iter().all(|frame| frame.row == 0));
    assert_eq!(step_chip(&one_row, 0, 0, 1), 1);
    assert_eq!(step_chip(&one_row, 1, 0, 1), 2);
    assert_eq!(step_chip(&one_row, 2, 0, 1), 2);
    assert_eq!(step_chip(&one_row, 2, 0, -1), 1);
}

#[test]
fn left_and_right_step_history_like_the_chevrons() {
    use ArrowAction::{Chip, History, Text};
    use ArrowFocus::{Card, TextWithContent};
    // ← is ‹ Previous (toward 1, the newest), → is › Next (toward the oldest).
    assert_eq!(
        arrow_action(Key::Left, false, Card, true),
        Some(History(PREVIOUS_CHEVRON))
    );
    assert_eq!(
        arrow_action(Key::Right, false, Card, true),
        Some(History(NEXT_CHEVRON))
    );
    assert_eq!(PREVIOUS_CHEVRON.command(), CommandId::HistoryNewer);
    assert_eq!(NEXT_CHEVRON.command(), CommandId::HistoryOlder);
    // Text in the search or find field: plain ← → move the caret.
    assert_eq!(
        arrow_action(Key::Left, false, TextWithContent, true),
        Some(Text)
    );
    assert_eq!(
        arrow_action(Key::Right, false, TextWithContent, false),
        Some(Text)
    );
    // ⌘← / ⌘→ always step history.
    for focus in [Card, TextWithContent] {
        for has_history in [true, false] {
            assert_eq!(
                arrow_action(Key::Left, true, focus, has_history),
                Some(History(PREVIOUS_CHEVRON))
            );
            assert_eq!(
                arrow_action(Key::Right, true, focus, has_history),
                Some(History(NEXT_CHEVRON))
            );
        }
    }
    // No history capsule: ← → move between the chips, as before.
    assert_eq!(
        arrow_action(Key::Left, false, Card, false),
        Some(Chip { dx: -1, dy: 0 })
    );
    assert_eq!(
        arrow_action(Key::Right, false, Card, false),
        Some(Chip { dx: 1, dy: 0 })
    );
    // ↑ ↓ move between the chips; other keys are not arrows.
    for (key, dy) in [(Key::Up, -1), (Key::Down, 1)] {
        for focus in [Card, TextWithContent] {
            assert_eq!(
                arrow_action(key, false, focus, true),
                Some(Chip { dx: 0, dy })
            );
        }
    }
    assert_eq!(arrow_action(Key::Return, false, Card, true), None);
    assert_eq!(arrow_action(Key::Escape, true, Card, true), None);
}

#[test]
fn history_nav_stays_on_the_card() {
    assert_eq!(history_nav(0, 0), None, "no history: no capsule");
    assert_eq!(history_nav(1, 0), None, "one copy: nowhere to go");
    let newest = history_nav(3, 0).unwrap();
    assert!(newest.can_older);
    assert!(!newest.can_newer);
    let middle = history_nav(3, 1).unwrap();
    assert!(middle.can_older && middle.can_newer);
    let oldest = history_nav(3, 2).unwrap();
    assert!(!oldest.can_older);
    assert!(oldest.can_newer);
    assert_eq!(newest.total, 3);
    assert_eq!(middle.total, 3);
    assert_eq!(step_history(3, 0, true), Some(1));
    assert_eq!(step_history(3, 0, false), None);
    assert_eq!(step_history(3, 2, true), None);
    assert_eq!(step_history(3, 2, false), Some(1));
    assert_eq!(step_history(0, 0, true), None);
    assert_eq!(step_history(1, 0, true), None);
    assert_eq!(step_history(1, 0, false), None);
}

#[test]
fn history_position_counts_from_the_newest() {
    assert_eq!(history_nav(2, 0).unwrap().position, 1);
    assert_eq!(history_nav(2, 1).unwrap().position, 2);
    // → (next) counts up, ← (previous) counts down.
    let next = step_history(5, 0, NEXT_CHEVRON.older).unwrap();
    assert_eq!(history_nav(5, next).unwrap().position, 2);
    let previous = step_history(5, next, PREVIOUS_CHEVRON.older).unwrap();
    assert_eq!(history_nav(5, previous).unwrap().position, 1);
    // A cursor past the end shows the oldest copy.
    assert_eq!(history_nav(3, 9).unwrap().position, 3);
    assert_eq!(history_nav(20, 19).unwrap().position, 20);
    assert_eq!(history_nav(20, 19).unwrap().total, 20);
}

#[test]
fn chevrons_follow_the_numbers() {
    assert_eq!(PREVIOUS_CHEVRON.command(), CommandId::HistoryNewer);
    assert_eq!(NEXT_CHEVRON.command(), CommandId::HistoryOlder);
    // At 1 / 3 only › works, at 3 / 3 only ‹, in between both.
    let first = history_nav(3, 0).unwrap();
    assert!(!PREVIOUS_CHEVRON.enabled(&first) && NEXT_CHEVRON.enabled(&first));
    let middle = history_nav(3, 1).unwrap();
    assert!(PREVIOUS_CHEVRON.enabled(&middle) && NEXT_CHEVRON.enabled(&middle));
    let last = history_nav(3, 2).unwrap();
    assert!(PREVIOUS_CHEVRON.enabled(&last) && !NEXT_CHEVRON.enabled(&last));
    // Walking with › from 1 reaches N, then stops; ‹ walks back to 1.
    let mut cursor = 0;
    let mut seen = vec![history_nav(3, cursor).unwrap().position];
    while let Some(next) = step_history(3, cursor, NEXT_CHEVRON.older) {
        cursor = next;
        seen.push(history_nav(3, cursor).unwrap().position);
    }
    assert_eq!(seen, vec![1, 2, 3]);
    while let Some(previous) = step_history(3, cursor, PREVIOUS_CHEVRON.older) {
        cursor = previous;
    }
    assert_eq!(history_nav(3, cursor).unwrap().position, 1);
}

#[test]
fn history_capsule_fits_the_longest_position() {
    // "20 / 20" in 12 pt monospaced digits is about 40 pt wide.
    const { assert!(NAV_COUNT_W >= 40.0) };
    // Square chevron hit areas as tall as a chip.
    const { assert!(NAV_BUTTON == CHIP_PILL_H) };
    assert_eq!(NAV_SPAN, NAV_BUTTON * 2.0 + NAV_COUNT_W + CHIP_GAP * 2.0);
}

#[test]
fn the_version_capsule_keeps_the_first_row_clear() {
    let widths = [62.0, 74.0, 78.0, 72.0, 86.0, 56.0];
    let width = 412.0;
    let frames = layout_chips(&widths, width, crate::commands::trailing_reserve(true));
    let capsule_left = crate::commands::version_capsule_x(width);
    assert!(frames.iter().any(|frame| frame.row > 0));
    for frame in frames.iter().filter(|frame| frame.row == 0) {
        assert!(frame.x + frame.width <= capsule_left - crate::commands::CHIP_GAP + 0.01);
    }
}

#[test]
fn at_most_four_chips_show_and_text_stays_visible() {
    use crate::commands::{Command, split_chips};
    use mac_ui::style::MAX_CARD_CHIPS;
    let chip = |id: CommandId| Command {
        id,
        title: String::new(),
        detail: String::new(),
        keywords: String::new(),
        enabled: true,
    };
    let ids = [
        CommandId::Convert,
        CommandId::Format,
        CommandId::Schema,
        CommandId::Sample,
        CommandId::Diff,
        CommandId::TextMenu,
    ];
    let (shown, more) = split_chips(ids.iter().map(|id| chip(id.clone())).collect());
    let shown: Vec<CommandId> = shown.into_iter().map(|c| c.id).collect();
    let more: Vec<CommandId> = more.into_iter().map(|c| c.id).collect();
    assert_eq!(shown.len(), MAX_CARD_CHIPS);
    assert_eq!(
        shown,
        [
            CommandId::Convert,
            CommandId::Format,
            CommandId::Schema,
            CommandId::TextMenu
        ]
    );
    assert_eq!(more, [CommandId::Sample, CommandId::Diff]);
    // Few chips: nothing moves.
    let (shown, more) = split_chips(vec![chip(CommandId::Convert), chip(CommandId::TextMenu)]);
    assert_eq!(shown.len(), 2);
    assert!(more.is_empty());
}

#[test]
fn chips_and_menu_steps_have_sf_symbols() {
    for id in [
        CommandId::TextMenu,
        CommandId::Copy,
        CommandId::Save,
        CommandId::TextFilter,
        CommandId::TextReverse,
        CommandId::TextCleanUrl,
        CommandId::TextJson,
        CommandId::TextCase(crate::text_ops::case_style::CaseStyle::Snake),
    ] {
        let name = crate::commands::symbol_for(&id).expect("a symbol");
        assert!(
            !name.is_empty() && name.is_ascii() && !name.contains(' '),
            "{name}"
        );
    }
    assert_eq!(crate::commands::symbol_for(&CommandId::History(0)), None);
}

#[test]
fn chip_symbols_are_plain_and_distinct_where_it_matters() {
    use crate::commands::symbol_for;
    for id in [
        CommandId::Info,
        CommandId::About,
        CommandId::Pause,
        CommandId::DeleteHistory,
        CommandId::DiffSwap,
    ] {
        let name = symbol_for(&id).expect("a symbol");
        assert!(!name.ends_with(".circle"), "{id:?}: {name}");
    }
    // Original no longer looks like Image ▾, Swap not like Reverse Lines, Decode not a lock.
    assert_ne!(
        symbol_for(&CommandId::Original),
        symbol_for(&CommandId::ImageMenu)
    );
    assert_ne!(
        symbol_for(&CommandId::DiffSwap),
        symbol_for(&CommandId::TextReverse)
    );
    assert_eq!(
        symbol_for(&CommandId::Original),
        Some("arrow.uturn.backward")
    );
    assert_eq!(
        symbol_for(&CommandId::Decode),
        Some("chevron.left.forwardslash.chevron.right")
    );
    assert_eq!(symbol_for(&CommandId::TextMenu), Some("textformat"));
    // One symbol, one action: Info is not About, Check AGENTS.md not Choose Columns.
    assert_ne!(symbol_for(&CommandId::Info), symbol_for(&CommandId::About));
    assert_ne!(
        symbol_for(&CommandId::AgentCheck),
        symbol_for(&CommandId::TableChooseColumns)
    );
}

#[test]
fn submenus_carry_the_icon_on_their_own_row() {
    use crate::commands::{menu_shows_icons, submenu_symbol};
    use crate::image_edit::ImageOp;
    // New ▸ is a plus, not History's clock (the old fallback).
    assert_eq!(
        submenu_symbol(&CommandId::Generate(crate::generate::Generator::UuidV4)),
        Some("plus")
    );
    assert_eq!(
        submenu_symbol(&CommandId::History(0)),
        Some("clock.arrow.circlepath")
    );
    assert_ne!(
        submenu_symbol(&CommandId::Generate(crate::generate::Generator::UuidV4)),
        submenu_symbol(&CommandId::History(0))
    );
    assert_eq!(submenu_symbol(&CommandId::Copy), None);
    // Image ▾ has no icons at all, undo and redo included; Text ▾ keeps them.
    assert!(!menu_shows_icons(&[
        CommandId::ImageStep(ImageOp::Grayscale),
        CommandId::TableUndo,
        CommandId::TableRedo
    ]));
    assert!(menu_shows_icons(&[
        CommandId::TextReverse,
        CommandId::TableUndo
    ]));
}

#[test]
fn overflow_menu_sections_follow_the_requested_order() {
    use crate::commands::{MenuSection, keep_history_short, menu_section, menu_submenu};
    let order = [
        CommandId::ChooseFile,
        CommandId::ClearClipboard,
        CommandId::DiffShown,
        CommandId::Pin,
        CommandId::DeleteHistory,
        CommandId::History(0),
        CommandId::KeepHistory(15),
        CommandId::Pause,
        CommandId::IgnoreNext,
        CommandId::IgnoreFrontApp,
        CommandId::ClearSensitive,
        CommandId::Appearance(crate::appearance::Theme::Dark),
        CommandId::WindowSize(crate::settings::WindowSize::Wide),
        CommandId::Settings,
        CommandId::About,
        CommandId::SuggestFeature,
        CommandId::ReportProblem,
        CommandId::Welcome,
        CommandId::Quit,
    ];
    let sections: Vec<MenuSection> = order.iter().map(menu_section).collect();
    assert!(
        sections.windows(2).all(|pair| pair[0] <= pair[1]),
        "{sections:?}"
    );
    assert_eq!(menu_section(&CommandId::ChooseFile), MenuSection::File);
    assert!(menu_submenu(&CommandId::KeepHistory(5)).is_some());
    assert!(menu_submenu(&CommandId::Appearance(crate::appearance::Theme::Light)).is_some());
    assert!(menu_submenu(&CommandId::WindowSize(crate::settings::WindowSize::Compact)).is_some());
    assert_eq!(menu_submenu(&CommandId::Pause), None);
    assert_eq!(keep_history_short(60), "60 min");
    assert!(crate::locale::t("menu_choose_file").ends_with('…'));
}

#[test]
fn placeholder_keeps_the_last_card_height() {
    use crate::commands::{building_card, is_building_card, stable_height};
    assert!(is_building_card(&building_card()));
    assert_eq!(stable_height(true, 120.0, Some(340.0)), 340.0);
    assert_eq!(stable_height(true, 120.0, None), 120.0);
    assert_eq!(stable_height(false, 280.0, Some(340.0)), 280.0);
}

#[test]
fn open_table_closes_the_card_only_after_reading_its_entry() {
    use crate::commands::{CommandId, closes_card_itself, keeps_card_open};
    // The launcher must not close the card first: the app reads the entry, then closes it.
    assert!(closes_card_itself(&CommandId::TableOpenWindow));
    assert!(!keeps_card_open(&CommandId::TableOpenWindow));
    assert!(!closes_card_itself(&CommandId::Save));
    assert!(!closes_card_itself(&CommandId::TableMenu));
}

#[test]
fn command_v_pastes_into_the_field_being_typed_in() {
    use crate::commands::{PasteTarget, paste_target};
    // A search, find or filter field with the caret: the field pastes.
    assert_eq!(paste_target(true, false), PasteTarget::Field);
    // The hidden search field (no search open) only waits for typing: the card pastes.
    assert_eq!(paste_target(true, true), PasteTarget::Card);
    // A button, the reveal cover or the read-only text in the well: the card pastes.
    assert_eq!(paste_target(false, false), PasteTarget::Card);
    assert_eq!(paste_target(false, true), PasteTarget::Card);
}
