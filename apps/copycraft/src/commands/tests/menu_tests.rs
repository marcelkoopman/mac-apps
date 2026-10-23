use super::*;

#[test]
fn the_table_menu_has_five_items_and_sorts_by_each_column_in_submenus() {
    use crate::table::TableOp;
    let table = deduped("name,n\na,1\na,1");
    let menu = table_menu(Some(&table));
    // The top level, in order; the sorts are submenus. Open table is the card chip.
    let mut top: Vec<String> = Vec::new();
    for c in &menu {
        let item = menu_group(&c.id).map_or(c.title.clone(), |group| format!("{group} ›"));
        if !top.contains(&item) {
            top.push(item);
        }
    }
    assert_eq!(
        top,
        [
            "Remove Duplicate Rows",
            "Remove Empty Rows and Columns",
            "Choose Columns…",
            "Sort Ascending ›",
            "Sort Descending ›",
        ]
    );
    let sort_up: Vec<&str> = menu
        .iter()
        .filter(|c| menu_group(&c.id) == Some("Sort Ascending"))
        .map(|c| c.title.as_str())
        .collect();
    assert_eq!(sort_up, ["name", "n"]);
    assert_eq!(
        TableOp::Sort {
            column: "Price".into(),
            descending: true
        }
        .label(),
        "Sorted by Price ↓"
    );
    assert_eq!(menu_group(&CommandId::TableUndo), None);
    // Without a frame there are no columns to offer yet.
    let mut loading = table.clone();
    loading.frame = None;
    assert!(
        table_menu(Some(&loading))
            .iter()
            .all(|c| menu_group(&c.id).is_none())
    );
}

#[test]
fn choose_columns_lists_the_version_shown_and_applies_as_one_step() {
    use crate::column_picker::ColumnPicker;
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let table = read(src, crate::dataframe::ReadOptions::default());
    let items = table_menu(Some(&table));
    let choose = items
        .iter()
        .find(|c| c.id == CommandId::TableChooseColumns)
        .expect("Choose Columns…");
    assert_eq!(choose.title, "Choose Columns…");
    assert_eq!(menu_group(&choose.id), None);
    assert!(keeps_card_open(&CommandId::TableChooseColumns));
    let columns = picker_columns(&table).expect("columns");
    assert_eq!(columns.len(), 20);
    assert_eq!(columns[0].kind, "date");
    let pv1 = &columns[14];
    assert_eq!(pv1.name, "Sunbox 7 X1500 Max - PV1 Generation (kWh)");
    assert_eq!(pv1.shown, "… PV1 Generation (kWh)");
    assert_eq!(pv1.kind, "number");
    // Untick 12: one step, the 8 kept in the table's order.
    let mut picker = ColumnPicker::new(columns);
    for index in (1..20).step_by(2).chain([2, 4]) {
        picker.set_kept(index, false);
    }
    assert_eq!(picker.count_line(), "Keeping 8 of 20");
    let step = picker.step().expect("step");
    assert_eq!(step.label(), "Kept 8 of 20 columns");
    let frame = table.frame.as_ref().expect("frame");
    let kept = step.apply(frame).expect("apply").expect("changed");
    let names: Vec<String> = kept
        .get_column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(names.len(), 8);
    assert_eq!(names[0], "Date");
    let order: Vec<usize> = names
        .iter()
        .map(|name| frame.get_column_index(name).expect("column"))
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    // A one-column table has nothing to choose.
    let one = stepped(
        src,
        crate::table::TableOp::SelectColumns {
            columns: vec!["Date".into()],
            kept_of: None,
        },
    );
    assert!(
        !table_menu(Some(&one))
            .iter()
            .any(|c| c.id == CommandId::TableChooseColumns)
    );
}

#[test]
fn image_menu_items_run_the_step_they_are_titled() {
    use crate::commands::{Command, ImageShown, image_menu_items, popped_pick};
    use crate::image_edit::ImageOp;
    let edit = ImageShown {
        entry: 1,
        labels: vec!["Original".into()],
        version: 0,
        working: false,
        note: None,
    };
    let items: Vec<Command> = image_menu_items(&edit)
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    for op in ImageOp::MENU {
        let item = items
            .iter()
            .find(|c| c.title == op.title())
            .expect("in the menu");
        assert_eq!(item.id, CommandId::ImageStep(op), "{}", op.title());
    }
    let left = items
        .iter()
        .position(|c| c.id == CommandId::ImageStep(ImageOp::RotateLeft))
        .unwrap();
    // A pick runs the item with the title shown, even when the list moved meanwhile.
    let picked = popped_pick(&items, left as isize, "Rotate 90° Left").unwrap();
    assert_eq!(picked.id, CommandId::ImageStep(ImageOp::RotateLeft));
    let moved = popped_pick(&items, left as isize + 2, "Rotate 90° Left").unwrap();
    assert_eq!(moved.id, CommandId::ImageStep(ImageOp::RotateLeft));
    assert!(popped_pick(&items, left as isize, "Nothing like it").is_none());
    assert!(popped_pick(&items, -1, "Rotate 90° Left").is_some());
}

#[test]
fn history_submenu_puts_add_to_diff_under_option_and_the_clears_apart() {
    use crate::commands::{alternate_title, is_alternate, menu_submenu, separator_before};
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.history.push(history_item(0, "first", true));
    input.history.push(history_item(1, "second", false));
    input.can_clear_history = true;
    let menu = overflow(&input);
    // The rows of History ▸, in order, as pop_overflow walks them.
    let history: Vec<&CommandId> = menu
        .iter()
        .map(|cmd| &cmd.id)
        .filter(|id| menu_submenu(id) == Some(crate::locale::t("history")))
        .collect();
    let mut previous: Option<&CommandId> = None;
    let mut alternates = 0;
    let mut separators = 0;
    for id in &history {
        if let CommandId::DiffToggle(_) = id {
            assert!(is_alternate(previous, id), "Add to Diff follows its copy");
            alternates += 1;
        } else {
            assert!(!is_alternate(previous, id));
        }
        if separator_before(previous, id) {
            assert_eq!(**id, CommandId::ClearHistory);
            separators += 1;
        }
        previous = Some(id);
    }
    assert_eq!(alternates, 2, "one ⌥ alternate per copy");
    assert_eq!(separators, 1, "one line above the clears");

    assert!(separator_before(
        Some(&CommandId::DiffToggle(3)),
        &CommandId::ClearPins
    ));
    assert!(!separator_before(
        Some(&CommandId::ClearHistory),
        &CommandId::ClearPins
    ));
    assert!(!separator_before(None, &CommandId::ClearHistory));
    assert!(!is_alternate(None, &CommandId::DiffToggle(3)));
    assert_eq!(
        alternate_title("Add to Diff", "first"),
        "Add to Diff: first"
    );
}

#[test]
fn menu_rows_show_the_chords_the_card_runs() {
    use crate::commands::menu_key;
    assert_eq!(menu_key(&CommandId::ChooseFile), Some(("o", false)));
    assert_eq!(menu_key(&CommandId::Settings), Some((",", false)));
    assert_eq!(menu_key(&CommandId::ToggleStrip), Some(("y", false)));
    assert_eq!(menu_key(&CommandId::TableUndo), Some(("z", false)));
    assert_eq!(menu_key(&CommandId::TableRedo), Some(("z", true)));
    assert_eq!(menu_key(&CommandId::Copy), None);
    assert_eq!(menu_key(&CommandId::History(0)), None);
    // ⌘Z / ⇧⌘Z on Text ▾ and Image ▾ are the card's own undo and redo of that version.
    let edited = edited_picture(1, &["Original", "Resized", "Grayscale"]);
    let bar = VersionBar::of(&edited);
    assert_eq!(
        undo_key("z", true, false, false, bar.as_ref()),
        Some(CommandId::TableUndo)
    );
    assert_eq!(
        undo_key("z", true, true, false, bar.as_ref()),
        Some(CommandId::TableRedo)
    );
    let menu = crate::commands::image_menu_items(edited.image_edit.as_ref().unwrap());
    let keyed: Vec<&CommandId> = menu
        .iter()
        .map(|(cmd, _)| &cmd.id)
        .filter(|id| menu_key(id).is_some())
        .collect();
    assert_eq!(keyed, [&CommandId::TableUndo, &CommandId::TableRedo]);
}

#[test]
fn a_strip_card_right_click_pins_diffs_and_removes_that_copy() {
    use crate::commands::{separator_before, thumb_menu, thumb_menu_pick, thumb_menu_tag};
    let menu = thumb_menu(2, Some(41), false, true, &[]);
    assert_eq!(titles(&menu), ["Pin", "Add to Diff", "Remove from History"]);
    assert_eq!(
        ids(&menu),
        [
            CommandId::StripPin(2),
            CommandId::DiffToggle(41),
            CommandId::RemoveHistory(2)
        ]
    );
    assert!(menu.iter().all(|c| c.enabled));
    // Remove sits apart, below a line; Pin and the diff action do not.
    assert!(!separator_before(Some(&menu[0].id), &menu[1].id));
    assert!(separator_before(Some(&menu[1].id), &menu[2].id));
    // A pinned card offers Unpin; a picture's diff action is dimmed; one already picked is
    // Remove from Diff.
    let pinned = thumb_menu(0, Some(7), true, false, &[]);
    assert_eq!(pinned[0].title, "Unpin");
    assert!(!pinned[1].enabled);
    assert_eq!(
        thumb_menu(0, Some(7), false, true, &[7])[1].title,
        "Remove from Diff"
    );
    // No id: no diff action.
    assert_eq!(thumb_menu(0, None, false, true, &[]).len(), 2);
    // Tags say which card and row; nothing in them quotes the copy.
    for (index, slot) in [(0, 0), (2, 1), (29, 2)] {
        assert_eq!(
            thumb_menu_pick(thumb_menu_tag(index, slot)),
            Some((index, slot))
        );
    }
    assert_eq!(thumb_menu_pick(-1), None);
}

#[test]
fn a_right_click_on_the_table_grid_finds_its_cell() {
    use crate::commands::{grid_cell_at, table_cell_menu};
    let body = "alpha\t2\nbeta\t\ngamma\t7";
    assert_eq!(grid_cell_at(body, 0, false), Some((0, 0)));
    assert_eq!(grid_cell_at(body, 3, false), Some((0, 0)));
    assert_eq!(grid_cell_at(body, 7, false), Some((0, 1)));
    assert_eq!(grid_cell_at(body, 8, false), Some((1, 0)));
    assert_eq!(grid_cell_at(body, body.len(), false), Some((2, 1)));
    // A numeric first column starts each line with a tab.
    let numeric = "\t1\tx\n\t22\ty";
    assert_eq!(grid_cell_at(numeric, 1, true), Some((0, 0)));
    assert_eq!(grid_cell_at(numeric, 4, true), Some((0, 1)));
    assert_eq!(grid_cell_at(numeric, 7, true), Some((1, 0)));
    // UTF-16 offsets (AppKit's): é is one unit, 😀 two.
    assert_eq!(grid_cell_at("😀é\tz", 4, false), Some((0, 1)));
    assert_eq!(grid_cell_at("", 0, false), None);
    let menu = table_cell_menu(4, 1);
    assert_eq!(titles(&menu), ["Copy Cell", "Copy Row", "Copy Column"]);
    assert_eq!(
        ids(&menu),
        [
            CommandId::TableCopyCell(4, 1),
            CommandId::TableCopyRow(4),
            CommandId::TableCopyColumn(1)
        ]
    );
}
