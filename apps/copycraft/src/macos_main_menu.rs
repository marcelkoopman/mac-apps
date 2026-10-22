//! The hidden main menu's Edit and Window menus, added to the app menu winit builds at launch
//! (About, Services, Hide ⌘H, Hide Others ⌥⌘H, Show All, Quit ⌘Q), which stays as it is.
//!
//! CopyCraft is an agent app (`LSUIElement`), so the menu bar never shows these menus, but
//! AppKit still matches their key equivalents: ⌘Z ⇧⌘Z ⌘X ⌘C ⌘V ⌘A reach the text field or view
//! with the caret through the standard `undo:` `redo:` `cut:` `copy:` `paste:` `selectAll:`
//! actions, and ⌘W ⌘M close or minimise the key window (Settings, Welcome, About, the table
//! window). A window's own `performKeyEquivalent:` runs first, so the card keeps its chords
//! (its ⌘Z version stack, ⌘V into the card) and hands a chord on only when a field should
//! take it. Every item has a nil target: AppKit sends the action along the responder chain and
//! disables the item when nothing there implements it.

use mac_ui::objc2::rc::Retained;
use mac_ui::objc2::runtime::Sel;
use mac_ui::objc2::{MainThreadMarker, MainThreadOnly, sel};
use mac_ui::objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};
use mac_ui::objc2_foundation::NSString;

/// One item: title key, action, key, and whether Shift is part of the chord.
type Item = (&'static str, Sel, &'static str, bool);

fn edit_items() -> [Item; 6] {
    [
        (crate::locale::t("menu_undo"), sel!(undo:), "z", false),
        (crate::locale::t("menu_redo"), sel!(redo:), "z", true),
        (crate::locale::t("menu_cut"), sel!(cut:), "x", false),
        (crate::locale::t("copy"), sel!(copy:), "c", false),
        (crate::locale::t("menu_paste"), sel!(paste:), "v", false),
        (
            crate::locale::t("menu_select_all"),
            sel!(selectAll:),
            "a",
            false,
        ),
    ]
}

fn window_items() -> [Item; 2] {
    [
        (
            crate::locale::t("menu_close"),
            sel!(performClose:),
            "w",
            false,
        ),
        (
            crate::locale::t("menu_minimize"),
            sel!(performMiniaturize:),
            "m",
            false,
        ),
    ]
}

fn submenu(mtm: MainThreadMarker, title: &str, items: &[Item]) -> Retained<NSMenuItem> {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for (at, (label, action, key, shift)) in items.iter().enumerate() {
        // Undo and Redo apart from the clipboard actions, as in every Edit menu.
        if at == 2 && items.len() == 6 {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
        let item = mac_ui::actions::menu_item_with_action(mtm, label, *action);
        item.setKeyEquivalent(&NSString::from_str(key));
        let mut mask = NSEventModifierFlags::Command;
        if *shift {
            mask |= NSEventModifierFlags::Shift;
        }
        item.setKeyEquivalentModifierMask(mask);
        menu.addItem(&item);
    }
    let parent = mac_ui::widgets::menu_item(mtm, title);
    parent.setSubmenu(Some(&menu));
    parent
}

/// Add Edit and Window after the app menu. Runs once, after launch (winit builds its menu in
/// `applicationDidFinishLaunching:`); a second call or a missing main menu does nothing.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main) = app.mainMenu() else {
        return;
    };
    let edit_title = crate::locale::t("menu_edit");
    if main
        .itemWithTitle(&NSString::from_str(edit_title))
        .is_some()
    {
        return;
    }
    main.addItem(&submenu(mtm, edit_title, &edit_items()));
    let window = submenu(mtm, crate::locale::t("menu_window"), &window_items());
    main.addItem(&window);
    // AppKit lists the open windows under it.
    app.setWindowsMenu(window.submenu().as_deref());
}
