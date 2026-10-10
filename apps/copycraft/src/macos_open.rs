#![cfg(target_os = "macos")]

use std::path::PathBuf;

use mac_ui::file_panel;
use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2_app_kit::NSWorkspace;
use mac_ui::objc2_foundation::{NSString, NSURL};

/// Ask for one file. Returns the path, or nothing when the panel is cancelled (or fails, which
/// is logged).
pub fn choose_path() -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    file_panel::choose_file(mtm, crate::locale::t("chip_choose_file"), &[]).unwrap_or_else(|e| {
        eprintln!("open panel failed: {e}");
        None
    })
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
