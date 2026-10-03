//! Modal open and save panels (`NSOpenPanel`, `NSSavePanel`).
//!
//! Every call needs the main thread and blocks in `runModal` until the panel is closed. A
//! cancelled panel is `Ok(None)`; only a confirmed panel without a file path is an error.
//! Reading or writing the chosen file is up to the caller.
//!
//! `allowed_extensions` are file name extensions such as `"txt"` or `".png"` (a leading dot and
//! surrounding whitespace are ignored, empty entries are skipped). Each one is mapped to its
//! content type (`UTType`); extensions the system has no type for are skipped. An empty list, or
//! one where no extension maps to a type, allows every file type.

use std::fmt;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSModalResponseOK, NSOpenPanel, NSPopUpButton, NSSavePanel, NSTextField, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};
use objc2_uniform_type_identifiers::UTType;

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
/// field (give it with its extension). Only extensions with a declared content type restrict the
/// panel. The panel can create directories and shows the extension. Returns `Ok(None)` when the
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
    // Only declared types: for a type the system made up from the extension (dynamic, such as
    // `parquet`), the panel adds the extension again to a name that has it ("x.parquet.parquet").
    // Without a restriction it keeps the name as suggested or typed.
    set_allowed_types(&panel, declared_types(allowed_extensions));
    run(&panel)
}

/// One file format in the popup of [`choose_save_path_with_format`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveFormat<'a> {
    /// The popup item, such as `"CSV"`.
    pub title: &'a str,
    /// Its file name extension, such as `"csv"`.
    pub extension: &'a str,
}

/// [`choose_save_path`] with a *Format* popup of `formats` under the panel, `selected` (an
/// index into `formats`) chosen first. Picking another format restricts the panel to it as
/// [`choose_save_path`] does and puts its extension on the name in place of a format's one
/// (`people.csv` → `people.parquet`). Give `suggested_name` with the selected format's
/// extension. Returns the path and the index of the format chosen; `Ok(None)` when the panel
/// is cancelled. With no formats this is [`choose_save_path`] with format 0.
///
/// # Errors
///
/// [`FilePanelError::NoPath`] when the panel is confirmed without a file path.
pub fn choose_save_path_with_format(
    mtm: MainThreadMarker,
    title: &str,
    suggested_name: &str,
    formats: &[SaveFormat<'_>],
    selected: usize,
) -> Result<Option<(PathBuf, usize)>, FilePanelError> {
    let Some(first) = formats.get(selected.min(formats.len().saturating_sub(1))) else {
        return Ok(choose_save_path(mtm, title, suggested_name, &[])?.map(|path| (path, 0)));
    };
    let selected = selected.min(formats.len() - 1);
    let panel = NSSavePanel::savePanel(mtm);
    panel.setCanCreateDirectories(true);
    panel.setExtensionHidden(false);
    panel.setTitle(Some(&NSString::from_str(title)));
    let extensions: Vec<String> = formats.iter().map(|f| f.extension.to_string()).collect();
    restrict_to(&panel, first.extension);
    panel.setNameFieldStringValue(&NSString::from_str(suggested_name));

    let label = NSTextField::labelWithString(&NSString::from_str("Format:"), mtm);
    label.sizeToFit();
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        NSRect::new(NSPoint::ZERO, NSSize::new(160.0, 26.0)),
        false,
    );
    for format in formats {
        popup.addItemWithTitle(&NSString::from_str(format.title));
    }
    popup.selectItemAtIndex(selected as isize);
    popup.sizeToFit();
    let (label_size, popup_size) = (label.frame().size, popup.frame().size);
    let gap = 8.0;
    let margin = 12.0;
    let width = label_size.width + gap + popup_size.width.max(120.0);
    let height = popup_size.height.max(label_size.height) + 2.0 * margin;
    let accessory = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::ZERO, NSSize::new(width + 2.0 * margin, height)),
    );
    label.setFrameOrigin(NSPoint::new(margin, (height - label_size.height) / 2.0));
    popup.setFrame(NSRect::new(
        NSPoint::new(
            margin + label_size.width + gap,
            (height - popup_size.height) / 2.0,
        ),
        NSSize::new(popup_size.width.max(120.0), popup_size.height),
    ));
    accessory.addSubview(&label);
    accessory.addSubview(&popup);
    panel.setAccessoryView(Some(&accessory));

    // The popup holds its target weakly: `switch` lives until the panel is closed.
    let switch = FormatSwitch::new(mtm, panel.clone(), extensions);
    // SAFETY: `FormatSwitch` implements `formatChosen:` taking the sender.
    unsafe {
        popup.setTarget(Some(&switch));
        popup.setAction(Some(sel!(formatChosen:)));
    }
    let path = run(&panel);
    // SAFETY: clears the target set above.
    unsafe { popup.setTarget(None) };
    let chosen = usize::try_from(popup.indexOfSelectedItem()).unwrap_or(selected);
    Ok(path?.map(|path| (path, chosen.min(formats.len() - 1))))
}

/// Restrict `panel` to `extension`'s declared content type, or to none when it has only a
/// dynamic one (see [`choose_save_path`]).
fn restrict_to(panel: &NSSavePanel, extension: &str) {
    let types = declared_types(&[extension]);
    let refs: Vec<&UTType> = types.iter().map(|t| &**t).collect();
    panel.setAllowedContentTypes(&NSArray::from_slice(&refs));
}

/// `name` with its extension, when it is one of `extensions` (any case), replaced by `to`;
/// otherwise `to` is added. A name that is only an extension keeps it (`.csv` → `.csv.parquet`).
pub fn switch_extension(name: &str, extensions: &[&str], to: &str) -> String {
    let to = to.trim().trim_start_matches('.');
    let stem = extensions
        .iter()
        .map(|ext| ext.trim().trim_start_matches('.'))
        .filter(|ext| !ext.is_empty())
        .find_map(|ext| {
            let cut = name.len().checked_sub(ext.len() + 1)?;
            let (stem, tail) = (name.get(..cut)?, name.get(cut..)?);
            (!stem.is_empty() && tail[1..].eq_ignore_ascii_case(ext) && tail.starts_with('.'))
                .then_some(stem)
        })
        .unwrap_or(name);
    if to.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{to}")
    }
}

struct SwitchIvars {
    panel: Retained<NSSavePanel>,
    extensions: Vec<String>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the ivars need no Objective-C cleanup.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MacUiSaveFormatSwitch"]
    #[ivars = SwitchIvars]
    struct FormatSwitch;

    impl FormatSwitch {
        #[unsafe(method(formatChosen:))]
        fn format_chosen(&self, sender: Option<&AnyObject>) {
            let Some(popup) = sender.and_then(|sender| sender.downcast_ref::<NSPopUpButton>())
            else {
                return;
            };
            let ivars = self.ivars();
            let Some(to) = usize::try_from(popup.indexOfSelectedItem())
                .ok()
                .and_then(|index| ivars.extensions.get(index))
            else {
                return;
            };
            let known: Vec<&str> = ivars.extensions.iter().map(String::as_str).collect();
            let name = switch_extension(&ivars.panel.nameFieldStringValue().to_string(), &known, to);
            // The types first: a name set under the old restriction could get its extension too.
            restrict_to(&ivars.panel, to);
            ivars.panel.setNameFieldStringValue(&NSString::from_str(&name));
        }
    }
);

impl FormatSwitch {
    fn new(
        mtm: MainThreadMarker,
        panel: Retained<NSSavePanel>,
        extensions: Vec<String>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SwitchIvars { panel, extensions });
        // SAFETY: `init` of NSObject.
        unsafe { msg_send![super(this), init] }
    }
}

/// The declared content types of `extensions` (after [`normalize_extensions`]); dynamic ones
/// are left out.
fn declared_types(extensions: &[&str]) -> Vec<Retained<UTType>> {
    normalize_extensions(extensions)
        .into_iter()
        .filter_map(|ext| UTType::typeWithFilenameExtension(&NSString::from_str(ext)))
        .filter(|kind| !kind.isDynamic())
        .collect()
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

/// Restrict `panel` to the content types of `extensions` (after [`normalize_extensions`]).
/// Extensions without a known `UTType` are skipped; when none remain the panel stays
/// unrestricted.
fn set_allowed_extensions(panel: &NSSavePanel, extensions: &[&str]) {
    let types: Vec<Retained<UTType>> = normalize_extensions(extensions)
        .into_iter()
        .filter_map(|ext| UTType::typeWithFilenameExtension(&NSString::from_str(ext)))
        .collect();
    set_allowed_types(panel, types);
}

/// Restrict `panel` to `types`; no restriction when there are none.
fn set_allowed_types(panel: &NSSavePanel, types: Vec<Retained<UTType>>) {
    if types.is_empty() {
        return;
    }
    let refs: Vec<&UTType> = types.iter().map(|t| &**t).collect();
    panel.setAllowedContentTypes(&NSArray::from_slice(&refs));
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
    fn switching_the_format_switches_a_known_extension_only() {
        let formats = ["csv", "parquet"];
        assert_eq!(
            switch_extension("people.csv", &formats, "parquet"),
            "people.parquet"
        );
        assert_eq!(
            switch_extension("people.parquet", &formats, "csv"),
            "people.csv"
        );
        assert_eq!(
            switch_extension("People.CSV", &formats, "parquet"),
            "People.parquet"
        );
        assert_eq!(
            switch_extension("Report v1.2", &formats, "csv"),
            "Report v1.2.csv"
        );
        assert_eq!(
            switch_extension("a.b.csv", &formats, "parquet"),
            "a.b.parquet"
        );
        assert_eq!(switch_extension("data", &formats, ".csv"), "data.csv");
        assert_eq!(
            switch_extension(".csv", &formats, "parquet"),
            ".csv.parquet"
        );
        assert_eq!(switch_extension("data.csv", &formats, ""), "data");
        let back = switch_extension(
            &switch_extension("t.csv", &formats, "parquet"),
            &formats,
            "csv",
        );
        assert_eq!(back, "t.csv");
    }

    #[test]
    fn error_message() {
        assert_eq!(
            FilePanelError::NoPath.to_string(),
            "the file panel returned no file path"
        );
    }
}
