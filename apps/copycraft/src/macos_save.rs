#![cfg(target_os = "macos")]

use anyhow::Context;
use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2_app_kit::{NSModalResponseOK, NSSavePanel};
use mac_ui::objc2_foundation::{NSArray, NSString};

/// Ask where to write `bytes`. `Ok(false)` when the panel is cancelled, `Err` when the panel
/// cannot be shown or the write fails.
pub fn write_with_panel(filename: &str, extension: &str, bytes: &[u8]) -> anyhow::Result<bool> {
    let mtm = MainThreadMarker::new().context("the save panel needs the main thread")?;
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
        return Ok(false);
    }
    let path = panel
        .URL()
        .and_then(|url| url.path())
        .context("the save panel returned no file path")?
        .to_string();
    std::fs::write(&path, bytes).with_context(|| format!("cannot write {path}"))?;
    Ok(true)
}
