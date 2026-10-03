//! Crash-safe file replacement for everything Ticker saves (config, menu bar pin, price history,
//! watches) and the `.bak` names corrupt files are moved to.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Replace `path` with `contents`: write a temporary file in the same directory, fsync it, then
/// rename it over `path` (and fsync the directory, best effort). A crash or full disk leaves
/// either the old file or the new one, never a truncated mix. A symlinked `path` (dotfiles
/// repo) is followed, so the link stays a link and its target is replaced.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let target = resolve_symlink(path);
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    // The pid keeps the app and a CLI run from sharing a temporary file.
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
        return written;
    }
    // Makes the rename itself durable; not every file system allows fsync on a directory.
    if let Ok(dir) = File::open(&dir) {
        let _ = dir.sync_all();
    }
    Ok(())
}

fn resolve_symlink(path: &Path) -> PathBuf {
    let is_link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    if is_link && let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    path.to_path_buf()
}

/// `<file>.bak`, or `<file>.<n>.bak` when earlier backups exist (never replaces one).
pub fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let candidate = path.with_file_name(format!("{name}.bak"));
    if !candidate.exists() {
        return candidate;
    }
    (1u32..)
        .map(|n| path.with_file_name(format!("{name}.{n}.bak")))
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

/// Move a file that could not be parsed to its [`backup_path`], so the next save cannot
/// overwrite what the user had; returns where it went.
pub fn move_aside(path: &Path) -> io::Result<PathBuf> {
    let backup = backup_path(path);
    fs::rename(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ticker-atomic-{tag}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn write_atomic_creates_and_replaces_without_leftovers() {
        let dir = temp_dir("replace");
        let path = dir.join("a.json");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "one");
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(entries(&dir), vec!["a.json"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_atomic_fails_cleanly_when_the_directory_is_missing() {
        let dir = temp_dir("missing");
        let path = dir.join("no-such-dir").join("a.json");
        assert!(write_atomic(&path, b"x").is_err());
        assert!(entries(&dir).is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_keeps_a_symlink() {
        let dir = temp_dir("link");
        let real = dir.join("real.toml");
        let link = dir.join("link.toml");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, b"new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn move_aside_never_replaces_a_backup() {
        let dir = temp_dir("bak");
        let path = dir.join("h.json");
        fs::write(&path, "first").unwrap();
        assert_eq!(move_aside(&path).unwrap(), dir.join("h.json.bak"));
        fs::write(&path, "second").unwrap();
        assert_eq!(move_aside(&path).unwrap(), dir.join("h.json.1.bak"));
        assert_eq!(fs::read_to_string(dir.join("h.json.bak")).unwrap(), "first");
        assert!(!path.exists());
        let _ = fs::remove_dir_all(dir);
    }
}
