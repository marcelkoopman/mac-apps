//! Modal open and save panels (`NSOpenPanel`, `NSSavePanel`).
//!
//! Every call needs the main thread and blocks in `runModal` until the panel is closed. A
//! cancelled panel is `Ok(None)`; only a confirmed panel without a file path is an error.
//! Reading or writing the chosen file is up to the caller.
//!
//! `allowed_extensions` are file name extensions such as `"txt"` or `".png"` (a leading dot and
//! surrounding whitespace are ignored, empty entries are skipped). An empty list allows every
//! file type.

use std::fmt;
use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
use objc2_foundation::{NSArray, NSString};

/// Why [`choose_file`] or [`choose_save_path`] failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum FilePanelError {
    /// The panel was confirmed but returned no file URL, or a URL without a file path.
    NoPath,
}

impl fmt::Display for FilePanelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPath => f.write_str("the file panel returned no file path"),
        }
    }
}

impl std::error::Error for FilePanelError {}

/// Ask for one existing file (no directories, no multiple selection) in an open panel titled
/// `title`. Returns `Ok(None)` when the panel is cancelled.
///
/// # Errors
///
/// [`FilePanelError::NoPath`] when the panel is confirmed without a file path.
pub fn choose_file(
    mtm: MainThreadMarker,
    title: &str,
    allowed_extensions: &[&str],
) -> Result<Option<PathBuf>, FilePanelError> {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setTitle(Some(&NSString::from_str(title)));
    set_allowed_extensions(&panel, allowed_extensions);
    run(&panel)
}

/// Ask where to save a file in a save panel titled `title`, with `suggested_name` in the name
/// field. The panel can create directories and shows the extension. Returns `Ok(None)` when the
/// panel is cancelled. AppKit has already asked to confirm overwriting an existing file.
///
/// # Errors
///
/// [`FilePanelError::NoPath`] when the panel is confirmed without a file path.
pub fn choose_save_path(
    mtm: MainThreadMarker,
    title: &str,
    suggested_name: &str,
    allowed_extensions: &[&str],
) -> Result<Option<PathBuf>, FilePanelError> {
    let panel = NSSavePanel::savePanel(mtm);
    panel.setCanCreateDirectories(true);
    panel.setExtensionHidden(false);
    panel.setNameFieldStringValue(&NSString::from_str(suggested_name));
    panel.setTitle(Some(&NSString::from_str(title)));
    set_allowed_extensions(&panel, allowed_extensions);
    run(&panel)
}

/// Run `panel` modally: `Ok(None)` unless it is confirmed, then the chosen file path.
fn run(panel: &NSSavePanel) -> Result<Option<PathBuf>, FilePanelError> {
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let path = panel
        .URL()
        .and_then(|url| url.path())
        .ok_or(FilePanelError::NoPath)?;
    Ok(Some(PathBuf::from(path.to_string())))
}

/// Restrict `panel` to `extensions` (after [`normalize_extensions`]); leaves it unrestricted when
/// none remain.
fn set_allowed_extensions(panel: &NSSavePanel, extensions: &[&str]) {
    let extensions = normalize_extensions(extensions);
    if extensions.is_empty() {
        return;
    }
    let strings: Vec<Retained<NSString>> =
        extensions.iter().map(|e| NSString::from_str(e)).collect();
    let refs: Vec<&NSString> = strings.iter().map(|s| &**s).collect();
    let types = NSArray::from_slice(&refs);
    // `setAllowedFileTypes` is deprecated (macOS 12) in favour of `setAllowedContentTypes`, which
    // takes `UTType`s from the `objc2-uniform-type-identifiers` crate. That crate is not a
    // dependency (yet), so keep the extension-based setter, which still works.
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&types));
}

/// Trim `extensions`, strip leading dots and drop empty entries and duplicates (first one
/// wins, case kept).
fn normalize_extensions<'a>(extensions: &[&'a str]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::with_capacity(extensions.len());
    for ext in extensions {
        let ext = ext.trim().trim_start_matches('.').trim();
        if !ext.is_empty() && !out.contains(&ext) {
            out.push(ext);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_extensions_pass_through() {
        assert_eq!(normalize_extensions(&["txt", "png"]), ["txt", "png"]);
    }

    #[test]
    fn leading_dots_and_whitespace_are_stripped() {
        assert_eq!(
            normalize_extensions(&[".txt", " png ", "..jpg", ". csv"]),
            ["txt", "png", "jpg", "csv"]
        );
    }

    #[test]
    fn empty_entries_are_dropped() {
        assert!(normalize_extensions(&["", " ", ".", " . "]).is_empty());
        assert!(normalize_extensions(&[]).is_empty());
    }

    #[test]
    fn duplicates_are_dropped_keeping_the_first() {
        assert_eq!(
            normalize_extensions(&["txt", ".txt", "md", "txt"]),
            ["txt", "md"]
        );
    }

    #[test]
    fn case_is_kept() {
        assert_eq!(normalize_extensions(&["PNG", "png"]), ["PNG", "png"]);
    }

    #[test]
    fn error_message() {
        assert_eq!(
            FilePanelError::NoPath.to_string(),
            "the file panel returned no file path"
        );
    }
}
