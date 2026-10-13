//! The table window's pop-up menus: Table ▾ and the versions.

use super::*;

/// Pop `items` (a check mark on the `true` ones) under `anchor`; a pick goes to the app as
/// the window's command. Items of one group go into a submenu under its name, as on the card.
pub(super) fn pop_menu(items: Vec<(Command, bool)>, anchor: &NSView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if items.is_empty() {
        NSBeep();
        return;
    }
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
    menu.setAutoenablesItems(false);
    let mut group: Option<(&str, Retained<NSMenu>)> = None;
    let delegate = DELEGATE.with(|slot| slot.borrow().clone());
    for (index, (cmd, checked)) in items.iter().enumerate() {
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
        item.setTag(index as isize);
        if *checked {
            item.setState(NSControlStateValueOn);
        }
        if let Some(delegate) = delegate.as_ref() {
            mac_ui::actions::set_menu_target(&item, delegate);
        }
        target.addItem(&item);
    }
    POPPED.with(|slot| {
        let mut popped = slot.borrow_mut();
        for cmd in popped.iter_mut() {
            cmd.wipe();
        }
        *popped = items.into_iter().map(|(cmd, _)| cmd).collect();
    });
    let below = NSPoint::new(0.0, anchor.frame().size.height + 4.0);
    menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(anchor));
}

pub(super) fn pop_table_menu() {
    let items = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|view| view.menu.clone())
            .unwrap_or_default()
    });
    let anchor = VIEWS.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|views| views.table_button.button().retain())
    });
    if let Some(anchor) = anchor {
        pop_menu(items, &anchor);
    }
}

pub(super) fn pop_versions_menu() {
    let bar = SHOWN.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|view| !view.versions.labels.is_empty())
            .map(|view| VersionBar {
                labels: view.versions.labels.clone(),
                current: view.versions.current.min(view.versions.labels.len() - 1),
            })
    });
    let Some(bar) = bar else {
        return;
    };
    let anchor = VIEWS.with(|slot| slot.borrow().as_ref().map(|views| views.title.clone()));
    if let Some(anchor) = anchor {
        pop_menu(bar.menu(), &anchor);
    }
}

include!("macos_table_paint.rs");
