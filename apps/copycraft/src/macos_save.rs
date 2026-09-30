#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponseOK, NSSavePanel};
use objc2_foundation::{NSArray, NSString};

/// Ask where to write `bytes`. Returns false when the panel is cancelled or the write fails.
pub fn write_with_panel(filename: &str, extension: &str, bytes: &[u8]) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let panel = NSSavePanel::savePanel(mtm);
    panel.setCanCreateDirectories(true);
    panel.setExtensionHidden(false);
    panel.setNameFieldStringValue(&NSString::from_str(filename));
    panel.setTitle(Some(&NSString::from_str("Save clipboard")));
    let ext = NSString::from_str(extension);
    let types = NSArray::from_slice(&[&*ext]);
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&types));
    if panel.runModal() != NSModalResponseOK {
        return false;
    }
    let Some(url) = panel.URL() else {
        return false;
    };
    let Some(path) = url.path() else {
        return false;
    };
    if let Err(e) = std::fs::write(path.to_string(), bytes) {
        eprintln!("save failed: {e}");
        return false;
    }
    true
}
