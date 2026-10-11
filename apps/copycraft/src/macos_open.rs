#![cfg(target_os = "macos")]

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Context;
use mac_ui::file_panel;
use mac_ui::objc2::MainThreadMarker;
use mac_ui::objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
use mac_ui::objc2_foundation::{NSArray, NSString, NSURL};
use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;

/// Picture bytes to hand to Preview. Built on the main thread; written on the preview thread.
pub(crate) enum PreviewBytes {
    /// A picture already in memory (the history entry, a dropped file, a version, a decoded
    /// Base64 picture). The extension is read from the bytes.
    Shared(SecretBytes),
    /// Pasteboard bytes copied on the main thread.
    Owned(Zeroizing<Vec<u8>>),
    /// No encoded bytes yet: decode the pasteboard grab to PNG on the preview thread.
    Clipboard(crate::macos_pasteboard::CardImageInput),
}

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

const PREVIEW_DIR: &str = "copycraft-preview";
const PREVIEW_PREFIX: &str = "copycraft-preview-";

/// Which preview the card asked for. Wipe, lock and a newer click move it on, so a finished
/// write of an older one is shredded instead of opened.
static PREVIEW_GENERATION: AtomicU64 = AtomicU64::new(0);
static PREVIEW_FILE_ID: AtomicU64 = AtomicU64::new(0);
/// Files Preview is showing. The next one replaces these; Wipe and quit shred them.
static PREVIEW_OPEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The id of this click. A later click, Wipe or quit makes [`preview_is_current`] false.
pub(crate) fn begin_preview() -> u64 {
    PREVIEW_GENERATION.fetch_add(1, Ordering::AcqRel) + 1
}

pub(crate) fn preview_is_current(generation: u64) -> bool {
    PREVIEW_GENERATION.load(Ordering::Acquire) == generation
}

/// Write `source` where Preview can read it. The bytes are zeroized afterwards. Does not open
/// Preview: that stays on the main thread ([`open_in_preview`]).
pub(crate) fn write_preview(source: PreviewBytes) -> anyhow::Result<PathBuf> {
    let (bytes, extension) = materialize(source)?;
    write_private(&bytes, extension)
}

fn materialize(source: PreviewBytes) -> anyhow::Result<(Zeroizing<Vec<u8>>, &'static str)> {
    let bytes = match source {
        PreviewBytes::Shared(secret) => Zeroizing::new(secret.with(<[u8]>::to_vec)),
        PreviewBytes::Owned(bytes) => bytes,
        PreviewBytes::Clipboard(input) => {
            let decoded =
                crate::macos_pasteboard::decode_card_input(input).context("no picture to open")?;
            Zeroizing::new(decoded.image.png_bytes().map_err(anyhow::Error::msg)?)
        }
    };
    if bytes.is_empty() {
        anyhow::bail!("no picture to open");
    }
    let kind = crate::macos_pasteboard::image_kind(&bytes).1;
    if let Some(extension) = preview_extension(kind) {
        return Ok((bytes, extension));
    }
    // Preview does not open this format (AVIF, camera RAW, …). A PNG does.
    let image = crate::macos_image_edit::decode(&bytes).context("cannot read the picture")?;
    drop(bytes);
    let png = crate::macos_image_edit::encode(&image, crate::macos_image_edit::Encoding::Png)
        .context("cannot write the picture")?;
    Ok((Zeroizing::new(png), "png"))
}

/// Extensions Preview opens on macOS 14. Anything else is written as PNG.
fn preview_extension(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "png" => "png",
        "jpg" | "jpeg" => "jpg",
        "gif" => "gif",
        "tif" | "tiff" => "tiff",
        "bmp" => "bmp",
        "heic" | "heif" => "heic",
        "webp" => "webp",
        "jp2" => "jp2",
        "ico" => "ico",
        _ => return None,
    })
}

fn write_private(bytes: &[u8], extension: &str) -> anyhow::Result<PathBuf> {
    let dir = preview_dir()?;
    let id = PREVIEW_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let name = format!("{PREVIEW_PREFIX}{}-{id}.{extension}", std::process::id());
    let path = dir.join(name);
    let write = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(err) = write {
        shred_preview(&path);
        return Err(err).with_context(|| format!("cannot write {}", path.display()));
    }
    Ok(path)
}

fn preview_dir() -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join(PREVIEW_DIR);
    std::fs::create_dir_all(&dir).context("cannot create the preview folder")?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    Ok(dir)
}

/// Preview shows `path`. `NSWorkspace` gives Preview a sandbox extension for the file, so this
/// needs no extra entitlement and no subprocess. False when Preview cannot be found or the
/// call is not on the main thread. Recent Items stays clear of the clipboard picture.
pub(crate) fn open_in_preview(path: &Path) -> bool {
    if MainThreadMarker::new().is_none() || !is_preview_file(path) {
        return false;
    }
    let Some(path_str) = path.to_str() else {
        return false;
    };
    let file = NSURL::fileURLWithPath(&NSString::from_str(path_str));
    let workspace = NSWorkspace::sharedWorkspace();
    let Some(app) =
        workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str("com.apple.Preview"))
    else {
        return false;
    };
    let config = NSWorkspaceOpenConfiguration::configuration();
    config.setActivates(true);
    config.setAddsToRecentItems(false);
    let urls = NSArray::from_slice(&[&*file]);
    workspace
        .openURLs_withApplicationAtURL_configuration_completionHandler(&urls, &app, &config, None);
    true
}

/// Remember `path` as the picture Preview is showing, and shred the one it replaced.
pub(crate) fn keep_preview(path: PathBuf) {
    if !is_preview_file(&path) {
        return;
    }
    let previous = {
        let mut open = PREVIEW_OPEN.lock().unwrap_or_else(|err| err.into_inner());
        std::mem::replace(&mut *open, vec![path])
    };
    for path in previous {
        shred_preview(&path);
    }
}

/// Forget every picture handed to Preview (Wipe, lock, sleep, quit, Clear history). A write
/// that is still running sees [`preview_is_current`] fail and shreds its file too.
pub(crate) fn discard_previews() {
    PREVIEW_GENERATION.fetch_add(1, Ordering::AcqRel);
    let previous = {
        let mut open = PREVIEW_OPEN.lock().unwrap_or_else(|err| err.into_inner());
        std::mem::take(&mut *open)
    };
    for path in previous {
        shred_preview(&path);
    }
}

/// Pictures a previous run left behind. Called once at launch, before a click can write one.
pub(crate) fn remove_stale_preview_files() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir().join(PREVIEW_DIR)) else {
        return;
    };
    for entry in entries.flatten() {
        shred_preview(&entry.path());
    }
}

/// Overwrite a preview file and delete it. Paths outside the preview folder are left alone.
pub(crate) fn shred_preview(path: &Path) {
    if !is_preview_file(path) {
        return;
    }
    if let Ok(meta) = std::fs::metadata(path)
        && meta.is_file()
        && let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path)
    {
        let zeros = [0u8; 65_536];
        let mut left = meta.len();
        while left > 0 {
            let n = usize::try_from(left)
                .unwrap_or(zeros.len())
                .min(zeros.len());
            if file.write_all(&zeros[..n]).is_err() {
                break;
            }
            left -= n as u64;
        }
        let _ = file.sync_all();
    }
    let _ = std::fs::remove_file(path);
}

fn is_preview_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(parent) = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
    else {
        return false;
    };
    parent == PREVIEW_DIR && name.starts_with(PREVIEW_PREFIX)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::{PreviewBytes, discard_previews, keep_preview, shred_preview, write_preview};
    use zeroize::Zeroizing;

    #[test]
    fn preview_files_stay_private_and_are_wiped() {
        let jpeg = write_preview(PreviewBytes::Owned(Zeroizing::new(vec![
            0xFF, 0xD8, 0xFF, 0xD9,
        ])))
        .expect("jpeg");
        assert_eq!(jpeg.extension().and_then(|ext| ext.to_str()), Some("jpg"));
        let mode = std::fs::metadata(&jpeg).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let png = write_preview(PreviewBytes::Owned(Zeroizing::new(
            b"\x89PNG\r\n\x1a\n".to_vec(),
        )))
        .expect("png");
        assert_eq!(png.extension().and_then(|ext| ext.to_str()), Some("png"));
        keep_preview(jpeg.clone());
        keep_preview(png.clone());
        assert!(!jpeg.exists(), "the previous preview file is wiped");
        assert!(png.exists());
        discard_previews();
        assert!(!png.exists());
    }

    #[test]
    fn shred_ignores_files_outside_the_preview_folder() {
        let path = std::env::temp_dir().join("copycraft-preview-not-ours.png");
        std::fs::write(&path, b"keep").expect("write");
        shred_preview(&path);
        assert_eq!(std::fs::read(&path).expect("read"), b"keep");
        std::fs::remove_file(&path).expect("remove");
    }
}
