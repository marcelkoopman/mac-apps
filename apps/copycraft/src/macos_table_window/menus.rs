//! The table window's menus: Table ▾ and the versions (on the toolbar's menu items).

use super::*;

/// A menu of `items` (a check mark on the `true` ones); a pick goes to the app as the
/// window's command. Items of one group go into a submenu under its name, as on the card.
/// The commands are appended to `popped`, each item's tag its index there.
pub(super) fn build_menu(
    mtm: MainThreadMarker,
    items: Vec<(Command, bool)>,
    popped: &mut Vec<Command>,
) -> Retained<NSMenu> {
    let menu = empty_menu(mtm);
    let mut group: Option<(&str, Retained<NSMenu>)> = None;
    let delegate = DELEGATE.with(|slot| slot.borrow().clone());
    for (cmd, checked) in items {
        let target = match commands::menu_group(&cmd.id) {
            None => {
                group = None;
                menu.clone()
            }
            Some(name) => match &group {
                Some((shown, submenu)) if *shown == name => submenu.clone(),
                _ => {
                    let parent = mac_ui::widgets::menu_item(mtm, name);
                    let submenu =
                        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(name));
                    submenu.setAutoenablesItems(false);
                    parent.setSubmenu(Some(&submenu));
                    menu.addItem(&parent);
                    group = Some((name, submenu.clone()));
                    submenu
                }
            },
        };
        let item = mac_ui::actions::menu_item_with_action(
            mtm,
            &cmd.title,
            sel!(tableWindowPoppedClicked:),
        );
        item.setTag(popped.len() as isize);
        if checked {
            item.setState(NSControlStateValueOn);
        }
        if let Some(delegate) = delegate.as_ref() {
            mac_ui::actions::set_menu_target(&item, delegate);
        }
        target.addItem(&item);
        popped.push(cmd);
    }
    menu
}

/// A menu without items that does not enable them itself.
pub(super) fn empty_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    menu
}

/// The commands behind the toolbar's menus now, by item tag; the previous ones are wiped.
pub(super) fn keep_popped(commands: Vec<Command>) {
    POPPED.with(|slot| {
        let mut popped = slot.borrow_mut();
        for cmd in popped.iter_mut() {
            cmd.wipe();
        }
        *popped = commands;
    });
}
