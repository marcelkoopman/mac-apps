//! Snippets on disk: the only thing CopyCraft keeps between runs besides its settings. Only
//! pinned texts marked as a snippet are stored, as their name and text, in
//! `~/Library/Application Support/copycraft/snippets.json` (in the App Sandbox that is the
//! app's own container), written atomically (a temporary file renamed over it) with mode 0600.
//! A snippet whose name or text holds a secret (a token, key or URL password, the check that
//! keeps a copy out of history) is never written, and one that does when it is read back is
//! dropped. A missing, corrupt or unreadable file is an empty list. Nothing here logs a name,
//! a text or a path.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

/// The file in [`dir`].
pub const FILE: &str = "snippets.json";
/// The format's version, in the file.
const VERSION: u64 = 1;
/// A larger file is not read (20 pins of at most a few MB each fit easily).
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// One stored snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub name: Zeroizing<String>,
    pub text: Zeroizing<String>,
}

/// Does the snippet hold a secret? Its name and its text are checked, here and now.
pub fn holds_secret(name: &str, text: &str) -> bool {
    use crate::sensitivity::{Label, scan_labels};
    [name, text]
        .iter()
        .any(|part| scan_labels(part).contains(&Label::Credential))
}

/// Where the file lives: `Application Support/copycraft` in the home folder (the container's
/// in the App Sandbox). `None` without a home folder.
pub fn dir() -> Option<PathBuf> {
    // Tests never touch the real home folder.
    if cfg!(test) {
        return None;
    }
    let home = std::env::home_dir()?;
    Some(home.join("Library/Application Support/copycraft"))
}

/// The file's text for `snippets`, without any that holds a secret.
pub fn encode(snippets: &[Stored]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(format!("{{\"version\":{VERSION},\"snippets\":["));
    let mut first = true;
    for snippet in snippets {
        if holds_secret(&snippet.name, &snippet.text) {
            continue;
        }
        let name = Zeroizing::new(serde_json::to_string(snippet.name.as_str()).unwrap_or_default());
        let text = Zeroizing::new(serde_json::to_string(snippet.text.as_str()).unwrap_or_default());
        if name.is_empty() || text.is_empty() {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"name\":");
        out.push_str(&name);
        out.push_str(",\"text\":");
        out.push_str(&text);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// The snippets in a file's bytes: empty when it is not this format; a snippet with a blank
/// name or text, or one that holds a secret, is dropped.
pub fn decode(bytes: &[u8]) -> Vec<Stored> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(VERSION) {
        return Vec::new();
    }
    let Some(list) = value.get("snippets").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.trim();
            let text = item.get("text")?.as_str()?;
            (!name.is_empty() && !text.trim().is_empty() && !holds_secret(name, text)).then(|| {
                Stored {
                    name: Zeroizing::new(name.to_string()),
                    text: Zeroizing::new(text.to_string()),
                }
            })
        })
        .take(crate::clipboard::PIN_MAX)
        .collect()
}

/// Write `snippets` (those without a secret) to `dir`: a temporary file with mode 0600, synced,
/// then renamed over [`FILE`]. No snippet left: the file is removed.
pub fn save(dir: &Path, snippets: &[Stored]) -> std::io::Result<()> {
    let path = dir.join(FILE);
    let body = encode(snippets);
    if body.as_str() == encode(&[]).as_str() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let temp = dir.join(format!(".{FILE}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let write = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    })();
    if write.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    write
}

/// The snippets stored in `dir`: empty when there is no file, or it is too large, unreadable or
/// corrupt.
pub fn load(dir: &Path) -> Vec<Stored> {
    let path = dir.join(FILE);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Vec::new();
    };
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Vec::new();
    }
    match std::fs::read(&path) {
        Ok(bytes) => decode(&Zeroizing::new(bytes)),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
#[path = "snippet_store_tests.rs"]
mod tests;
