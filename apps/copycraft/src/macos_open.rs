#![cfg(target_os = "macos")]

use mac_ui::objc2::{ClassType, MainThreadMarker};
use mac_ui::objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSWorkspace};
use mac_ui::objc2_foundation::{NSString, NSURL};

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

/// Open an `http(s)` URL in the default browser via `NSWorkspace` (no `open` subprocess, so the
/// text is never parsed as a path or an app). Other schemes are refused. False when refused or
/// when the system could not open it.
pub fn open_web_url(url: &str) -> bool {
    if !crate::url_policy::may_visit(url) {
        return false;
    }
    let Some(nsurl) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return false;
    };
    // Re-check what NSURL parsed, not only the string.
    let scheme = nsurl.scheme().map(|s| s.to_string().to_ascii_lowercase());
    if !matches!(scheme.as_deref(), Some("http" | "https")) || nsurl.host().is_none() {
        return false;
    }
    NSWorkspace::sharedWorkspace().openURL(&nsurl)
}
