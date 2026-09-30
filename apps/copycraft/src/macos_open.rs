#![cfg(target_os = "macos")]

use objc2::{ClassType, MainThreadMarker};
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::NSString;

/// Ask for one file. Returns the path, or nothing when the panel is cancelled.
pub fn choose_path() -> Option<String> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    let save = panel.as_super();
    save.setTitle(Some(&NSString::from_str("Choose file")));
    if save.runModal() != NSModalResponseOK {
        return None;
    }
    let url = save.URL()?;
    let path = url.path()?;
    Some(path.to_string())
}
