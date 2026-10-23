//! The table window's toolbar: a real `NSToolbar` in the unified compact style, not
//! customisable. *Table ▾* (an `NSMenuToolbarItem`, the one text button), *Copy* and *Save*
//! (symbols), the hint while the grid is masked, and the version group (↶, the version
//! shown with its list, ↷) as an `NSToolbarItemGroup`. The items are made once and handed
//! out by [`TableWindowDelegate`]'s toolbar methods; their menus are rebuilt on every paint.

use super::*;
use mac_ui::objc2::runtime::ProtocolObject;
use mac_ui::objc2_app_kit::{
    NSImage, NSMenuToolbarItem, NSToolbar, NSToolbarDisplayMode,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSToolbarItemGroup,
    NSToolbarItemGroupControlRepresentation, NSToolbarItemVisibilityPriorityLow,
    NSWindowToolbarStyle,
};

const TOOLBAR_ID: &str = "CopycraftTableToolbar";
const TABLE_ID: &str = "CopycraftTableMenu";
const COPY_ID: &str = "CopycraftTableCopy";
const SAVE_ID: &str = "CopycraftTableSave";
const HINT_ID: &str = "CopycraftTableHint";
const UNDO_ID: &str = "CopycraftTableUndo";
const VERSIONS_ID: &str = "CopycraftTableVersions";
const REDO_ID: &str = "CopycraftTableRedo";
const GROUP_ID: &str = "CopycraftTableVersionGroup";

/// The toolbar and its items, kept for the life of the window (in `TOOLBAR`).
pub(super) struct ToolbarItems {
    table: Retained<NSMenuToolbarItem>,
    copy: Retained<NSToolbarItem>,
    save: Retained<NSToolbarItem>,
    hint: Retained<NSToolbarItem>,
    undo: Retained<NSToolbarItem>,
    versions: Retained<NSMenuToolbarItem>,
    redo: Retained<NSToolbarItem>,
    group: Retained<NSToolbarItemGroup>,
}

/// Build the items (`hint` is the label shown while masked) and put the toolbar on `window`.
/// Call after `DELEGATE` is set: it is the toolbar's delegate and the items' target.
pub(super) fn install(mtm: MainThreadMarker, window: &NSWindow, hint: &NSTextField) {
    let table = NSMenuToolbarItem::initWithItemIdentifier(
        NSMenuToolbarItem::alloc(mtm),
        &NSString::from_str(TABLE_ID),
    );
    table.setShowsIndicator(true);
    let copy = symbol_item(mtm, COPY_ID, "doc.on.doc", sel!(tableWindowCopyClicked:));
    let save = symbol_item(
        mtm,
        SAVE_ID,
        "square.and.arrow.down",
        sel!(tableWindowSaveClicked:),
    );
    let hint_item = NSToolbarItem::initWithItemIdentifier(
        NSToolbarItem::alloc(mtm),
        &NSString::from_str(HINT_ID),
    );
    hint_item.setView(Some(hint));
    hint_item.setVisibilityPriority(NSToolbarItemVisibilityPriorityLow);
    let undo = symbol_item(
        mtm,
        UNDO_ID,
        "arrow.uturn.backward",
        sel!(tableWindowUndoClicked:),
    );
    let redo = symbol_item(
        mtm,
        REDO_ID,
        "arrow.uturn.forward",
        sel!(tableWindowRedoClicked:),
    );
    let versions = NSMenuToolbarItem::initWithItemIdentifier(
        NSMenuToolbarItem::alloc(mtm),
        &NSString::from_str(VERSIONS_ID),
    );
    versions.setShowsIndicator(false);
    versions.setAutovalidates(false);
    table.setAutovalidates(false);
    let group = NSToolbarItemGroup::initWithItemIdentifier(
        NSToolbarItemGroup::alloc(mtm),
        &NSString::from_str(GROUP_ID),
    );
    let subitems: [&NSToolbarItem; 3] = [&undo, &versions, &redo];
    group.setSubitems(&NSArray::from_slice(&subitems));
    group.setControlRepresentation(NSToolbarItemGroupControlRepresentation::Expanded);
    let items = ToolbarItems {
        table,
        copy,
        save,
        hint: hint_item,
        undo,
        versions,
        redo,
        group,
    };
    relabel_items(&items);
    TOOLBAR.with(|slot| slot.replace(Some(items)));

    let toolbar =
        NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str(TOOLBAR_ID));
    toolbar.setAllowsUserCustomization(false);
    toolbar.setAutosavesConfiguration(false);
    toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            toolbar.setDelegate(Some(ProtocolObject::from_ref(&**delegate)));
        }
    });
    window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);
    window.setToolbar(Some(&toolbar));
}

/// A plain (symbol, no title) item sending `action` to the delegate.
fn symbol_item(
    mtm: MainThreadMarker,
    id: &str,
    symbol: &str,
    action: Sel,
) -> Retained<NSToolbarItem> {
    let item =
        NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), &NSString::from_str(id));
    // No such symbol on this macOS: `relabel_items` gives the item a title instead of a gap.
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(symbol),
        None,
    ) {
        item.setImage(Some(&image));
    }
    item.setBordered(true);
    item.setAutovalidates(false);
    DELEGATE.with(|slot| {
        if let Some(delegate) = slot.borrow().as_ref() {
            let target: &AnyObject = delegate;
            // SAFETY: the target is the window's delegate, which DELEGATE keeps alive for the
            // rest of the process (the item holds it weakly), and `action` is one of its
            // methods taking the sender.
            unsafe {
                item.setTarget(Some(target));
                item.setAction(Some(action));
            }
        }
    });
    item
}

/// Labels, titles and tooltips in the current language. A label names the item in the
/// overflow menu (a narrow window) and for VoiceOver.
fn relabel_items(items: &ToolbarItems) {
    let t = crate::locale::t;
    let s = |text: &str| NSString::from_str(text);
    items.table.setTitle(&s(t("table_window_title")));
    items.table.setLabel(&s(t("table_steps_a11y")));
    items.table.setToolTip(Some(&s(t("table_steps_a11y"))));
    items.copy.setLabel(&s(t("copy_table_a11y")));
    items.copy.setToolTip(Some(&s(t("copy_table_tip"))));
    items.save.setLabel(&s(t("save_table_a11y")));
    items.save.setToolTip(Some(&s(t("save_table_tip"))));
    for (item, name, keys) in [
        (&items.undo, t("undo_table_step"), "⌘Z"),
        (&items.redo, t("redo_table_step"), "⇧⌘Z"),
    ] {
        item.setLabel(&s(name));
        item.setToolTip(Some(&s(&commands::keyed_tip(name, keys))));
        if item.image().is_none() {
            item.setTitle(&s(if keys == "⌘Z" { "↶" } else { "↷" }));
        }
    }
    for item in [&items.copy, &items.save] {
        if item.image().is_none() {
            item.setTitle(&item.label());
        }
    }
    items.versions.setLabel(&s(t("version")));
    items.group.setLabel(&s(t("version")));
}

/// The toolbar's labels in the current language (Settings › Language).
pub(super) fn relabel() {
    TOOLBAR.with(|slot| {
        if let Some(items) = slot.borrow().as_ref() {
            relabel_items(items);
        }
    });
}

/// Enable the items for `view` and rebuild Table ▾ and the version list from it.
pub(super) fn paint(view: &TableWindowView) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    TOOLBAR.with(|slot| {
        let borrowed = slot.borrow();
        let Some(items) = borrowed.as_ref() else {
            return;
        };
        // Enabled once the version's grid is there (not while "Working on the table…").
        let ready = !view.grid.is_empty();
        items.copy.setEnabled(ready);
        items.save.setEnabled(ready);
        items.undo.setEnabled(view.can_undo());
        items.redo.setEnabled(view.can_redo());
        // Never off: an empty menu shows its steps dimmed (`table_window_menu_shown`).
        items.table.setEnabled(true);
        items
            .versions
            .setTitle(&NSString::from_str(&view.version_title()));
        let mut popped = Vec::new();
        let table_menu = menus::build_menu(
            mtm,
            commands::table_window_menu_shown(&view.menu),
            &mut popped,
        );
        items.table.setMenu(&table_menu);
        if view.versions.labels.is_empty() {
            items.versions.setEnabled(false);
            items.versions.setMenu(&menus::empty_menu(mtm));
            items.versions.setToolTip(None);
        } else {
            let bar = VersionBar {
                labels: view.versions.labels.clone(),
                current: view.versions.current.min(view.versions.labels.len() - 1),
            };
            items.versions.setEnabled(true);
            items
                .versions
                .setMenu(&menus::build_menu(mtm, bar.menu(), &mut popped));
            items
                .versions
                .setToolTip(Some(&NSString::from_str(&bar.spoken())));
        }
        menus::keep_popped(popped);
    });
}

/// Empty both menus and the version title and tooltip (the window is wiped).
pub(super) fn wipe() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    TOOLBAR.with(|slot| {
        if let Some(items) = slot.borrow().as_ref() {
            items.table.setMenu(&menus::empty_menu(mtm));
            items.versions.setMenu(&menus::empty_menu(mtm));
            items.versions.setTitle(&NSString::from_str(""));
            items.versions.setToolTip(None);
        }
    });
}

/// The item for `id` (the delegate's `toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:`).
pub(super) fn item(id: &NSString) -> Option<Retained<NSToolbarItem>> {
    let id = id.to_string();
    TOOLBAR.with(|slot| {
        let borrowed = slot.borrow();
        let items = borrowed.as_ref()?;
        let item: Retained<NSToolbarItem> = match id.as_str() {
            TABLE_ID => Retained::into_super(items.table.clone()),
            COPY_ID => items.copy.clone(),
            SAVE_ID => items.save.clone(),
            HINT_ID => items.hint.clone(),
            GROUP_ID => Retained::into_super(items.group.clone()),
            _ => return None,
        };
        Some(item)
    })
}

/// Table ▾, Copy, Save, the hint centred between flexible spaces, the version group.
pub(super) fn identifiers() -> Retained<NSArray<NSString>> {
    // SAFETY: an AppKit constant (an immutable NSString), read on the main thread after
    // AppKit is loaded.
    let flexible: &NSString = unsafe { NSToolbarFlexibleSpaceItemIdentifier };
    let ids: Vec<Retained<NSString>> = [TABLE_ID, COPY_ID, SAVE_ID]
        .iter()
        .map(|id| NSString::from_str(id))
        .chain([
            flexible.retain(),
            NSString::from_str(HINT_ID),
            flexible.retain(),
        ])
        .chain([NSString::from_str(GROUP_ID)])
        .collect();
    NSArray::from_retained_slice(&ids)
}
