#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use anyhow::Context;
use mac_ui::file_panel;
use mac_ui::objc2::MainThreadMarker;
use zeroize::Zeroizing;

use crate::commands::{self, CardView};

/// What the save icon writes: the name the panel suggests, and the bytes or the work that
/// builds them once a path is chosen.
pub struct SaveJob {
    pub filename: String,
    pub extension: &'static str,
    pub content: SaveContent,
}

/// Bytes for a [`SaveJob`]. Anything slow (formatting, Parquet, PNG) is left for
/// [`SaveContent::write_to`], which runs on the save thread.
pub enum SaveContent {
    /// Ready to write.
    Bytes(Zeroizing<Vec<u8>>),
    /// Card text, turned into the file for `view` (see [`commands::text_save_file`]).
    Text {
        source: Zeroizing<String>,
        view: CardView,
    },
    /// The clipboard picture, decoded and encoded as PNG.
    ClipboardPng,
    /// A table version, written as Parquet.
    Table(polars::prelude::DataFrame),
}

impl SaveJob {
    /// Save job for a table version (Dataframe view): Parquet, built on the save thread.
    pub fn table(frame: polars::prelude::DataFrame) -> Self {
        Self {
            filename: crate::format::FormatKind::Dataframe.suggested_filename(),
            extension: crate::format::FormatKind::Dataframe.suggested_extension(),
            content: SaveContent::Table(frame),
        }
    }

    /// Save job for the card text in `view`. `None` when the view has nothing to save.
    pub fn text(source: &str, view: CardView) -> Option<Self> {
        if let Some((filename, extension)) = commands::deferred_save_name(source, view) {
            return Some(Self {
                filename,
                extension,
                content: SaveContent::Text {
                    source: Zeroizing::new(source.to_string()),
                    view,
                },
            });
        }
        let mut file = commands::text_save_file(source, view)?;
        Some(Self {
            filename: std::mem::take(&mut file.filename),
            extension: file.extension,
            content: SaveContent::Bytes(Zeroizing::new(std::mem::take(&mut file.bytes))),
        })
    }
}

impl SaveContent {
    /// Build the bytes (when not ready yet) and write them to `path`. Safe to run off the main
    /// thread. `Err` when the card has nothing to save after all, or the write fails.
    pub fn write_to(self, path: &Path) -> anyhow::Result<()> {
        let bytes = match self {
            Self::Bytes(bytes) => bytes,
            Self::Text { source, view } => {
                let mut file = commands::text_save_file(&source, view)
                    .context("the card has nothing to save in this view")?;
                Zeroizing::new(std::mem::take(&mut file.bytes))
            }
            Self::Table(frame) => Zeroizing::new(
                crate::dataframe::frame_parquet(&frame).context("the table is empty")?,
            ),
            Self::ClipboardPng => {
                let decoded = crate::macos_pasteboard::decode_preview()
                    .context("no image on the clipboard")?;
                Zeroizing::new(decoded.image.png_bytes().map_err(anyhow::Error::msg)?)
            }
        };
        std::fs::write(path, bytes.as_slice())
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

/// Ask where to save `filename`. `Ok(None)` when the panel is cancelled, `Err` when the panel
/// cannot be shown.
pub fn choose_path(filename: &str, extension: &str) -> anyhow::Result<Option<PathBuf>> {
    let mtm = MainThreadMarker::new().context("the save panel needs the main thread")?;
    // Every save path comes here: the stem and exactly one extension ([`save_name`]).
    let name = crate::open_file::save_name(filename, extension);
    Ok(file_panel::choose_save_path(
        mtm,
        "Save clipboard",
        &name,
        &[extension],
    )?)
}
