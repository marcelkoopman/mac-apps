#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

use anyhow::Context;
use mac_ui::file_panel;
use mac_ui::objc2::MainThreadMarker;
use zeroize::Zeroizing;

use crate::commands::{self, CardView};
use crate::dataframe::TableFile;

/// What the save icon writes: the name the panel suggests, and the bytes or the work that
/// builds them once a path is chosen.
pub struct SaveJob {
    pub filename: String,
    pub extension: &'static str,
    pub content: SaveContent,
}

/// Bytes for a [`SaveJob`]. Anything slow (formatting, CSV or Parquet, PNG) is left for
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
    /// The table of the Dataframe view, as `format` (CSV unless Parquet is picked in the panel).
    Table { table: TableData, format: TableFile },
}

/// The table to save: the version shown, or the copied text when its frame is not there yet
/// (read on the save thread, as the view reads it).
pub enum TableData {
    Frame(polars::prelude::DataFrame),
    Text(Zeroizing<String>),
}

impl SaveJob {
    /// Save job for a table version (Dataframe view): CSV by default, built on the save thread.
    pub fn table(frame: polars::prelude::DataFrame) -> Self {
        Self::table_of(TableData::Frame(frame))
    }

    fn table_of(table: TableData) -> Self {
        Self {
            filename: crate::format::FormatKind::Dataframe.suggested_filename(),
            extension: TableFile::Csv.extension(),
            content: SaveContent::Table {
                table,
                format: TableFile::Csv,
            },
        }
    }

    /// Save job for the card text in `view`. `None` when the view has nothing to save.
    pub fn text(source: &str, view: CardView) -> Option<Self> {
        if view == CardView::Dataframe {
            return Some(Self::table_of(TableData::Text(Zeroizing::new(
                source.to_string(),
            ))));
        }
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

    /// Ask where to save it (a table also asks CSV or Parquet). `Ok(None)` when the panel is
    /// cancelled.
    pub fn choose_path(&mut self) -> anyhow::Result<Option<PathBuf>> {
        let SaveContent::Table { format, .. } = &mut self.content else {
            return choose_path(&self.filename, self.extension);
        };
        let Some((path, chosen)) = choose_table_path(&self.filename, *format)? else {
            return Ok(None);
        };
        *format = chosen;
        Ok(Some(path))
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
            Self::Table { table, format } => {
                let frame = match table {
                    TableData::Frame(frame) => frame,
                    TableData::Text(source) => crate::dataframe::parse_table(&source)
                        .context("the card has no table to save")?,
                };
                Zeroizing::new(format.bytes(&frame).context("the table is empty")?)
            }
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

/// Ask where to save a table, with a CSV / Parquet popup under the panel (`format` picked
/// first). Switching it switches the name's extension. `Ok(None)` when the panel is cancelled.
pub fn choose_table_path(
    filename: &str,
    format: TableFile,
) -> anyhow::Result<Option<(PathBuf, TableFile)>> {
    let mtm = MainThreadMarker::new().context("the save panel needs the main thread")?;
    let name = crate::open_file::save_name(filename, format.extension());
    let formats: Vec<file_panel::SaveFormat> = TableFile::ALL
        .iter()
        .map(|file| file_panel::SaveFormat {
            title: file.title(),
            extension: file.extension(),
        })
        .collect();
    let selected = TableFile::ALL
        .iter()
        .position(|file| *file == format)
        .unwrap_or(0);
    let chosen =
        file_panel::choose_save_path_with_format(mtm, "Save table", &name, &formats, selected)?;
    Ok(chosen.map(|(path, index)| (path, TableFile::ALL[index.min(TableFile::ALL.len() - 1)])))
}
