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
            "Remove duplicate rows",
            "Remove empty rows and columns",
            "Choose columns…",
            "Sort ascending ›",
            "Sort descending ›",
        ]
    );
    let sort_up: Vec<&str> = menu
        .iter()
        .filter(|c| menu_group(&c.id) == Some("Sort ascending"))
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
        .expect("Choose columns…");
    assert_eq!(choose.title, "Choose columns…");
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
