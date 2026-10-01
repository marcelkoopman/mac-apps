#![cfg(target_os = "macos")]

use anyhow::Context;
use mac_ui::file_panel;
use mac_ui::objc2::MainThreadMarker;

/// Ask where to write `bytes`. `Ok(false)` when the panel is cancelled, `Err` when the panel
/// cannot be shown or the write fails.
pub fn write_with_panel(filename: &str, extension: &str, bytes: &[u8]) -> anyhow::Result<bool> {
    let mtm = MainThreadMarker::new().context("the save panel needs the main thread")?;
    let Some(path) = file_panel::choose_save_path(mtm, "Save clipboard", filename, &[extension])?
    else {
        return Ok(false);
    };
    std::fs::write(&path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(true)
}
