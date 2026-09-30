//! Single-instance guard for the menubar app: an exclusive, non-blocking lock (`flock` on macOS,
//! via `std::fs::File::try_lock`) on a file next to the user config. The OS drops the lock when the
//! process exits or crashes, so a stale file never blocks the next start.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const LOCK_FILE_NAME: &str = ".ticker.lock";

/// Held for the lifetime of the menubar app; dropping it releases the lock.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Lock file in the directory of `.ticker_config.toml` (inside the sandbox container when
/// sandboxed; follows `TICKER_USER_CONFIG_PATH`).
pub fn default_lock_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let config = crate::config::user_config_path()?;
    let dir = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(dir.join(LOCK_FILE_NAME))
}

/// `Ok(Some(lock))`: this is the only instance. `Ok(None)`: another process holds the lock.
/// `Err`: the lock file could not be opened or locked for another reason.
pub fn try_acquire(path: &Path) -> io::Result<Option<InstanceLock>> {
    // No truncate on open: the file may belong to a running instance.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(e)) => return Err(e),
    }
    // Informational only (the lock is what counts).
    let _ = file.set_len(0);
    let _ = writeln!(file, "{}", std::process::id());
    Ok(Some(InstanceLock {
        _file: file,
        path: path.to_path_buf(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock_path(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ticker-lock-test-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(LOCK_FILE_NAME)
    }

    #[test]
    fn first_acquire_succeeds_and_writes_pid() {
        let path = temp_lock_path("first");
        let lock = try_acquire(&path).unwrap().expect("lock is free");
        assert_eq!(lock.path(), path);
        let pid = std::fs::read_to_string(&path).unwrap();
        assert_eq!(pid.trim(), std::process::id().to_string());
    }

    #[test]
    fn second_acquire_is_refused_until_first_is_dropped() {
        // flock locks belong to the open file description, so a second open in the same process
        // conflicts just like a second process would.
        let path = temp_lock_path("second");
        let first = try_acquire(&path).unwrap().expect("lock is free");
        assert!(try_acquire(&path).unwrap().is_none());
        // The refused attempt must not clobber the holder's pid.
        let pid = std::fs::read_to_string(&path).unwrap();
        assert_eq!(pid.trim(), std::process::id().to_string());
        drop(first);
        assert!(try_acquire(&path).unwrap().is_some());
    }

    #[test]
    fn stale_lock_file_without_holder_does_not_block() {
        let path = temp_lock_path("stale");
        std::fs::write(&path, "99999\n").unwrap();
        assert!(try_acquire(&path).unwrap().is_some());
    }

    #[test]
    fn missing_directory_is_an_error() {
        let path = temp_lock_path("missing").join("nope").join(LOCK_FILE_NAME);
        assert!(try_acquire(&path).is_err());
    }
}
