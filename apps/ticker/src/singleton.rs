use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process;

fn default_lock_file_path() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot find home directory");
    home.join(".ticker.lock")
}

fn lock_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("TICKER_LOCK_PATH") {
        PathBuf::from(path)
    } else {
        default_lock_file_path()
    }
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}

fn try_acquire_lock_at(lock_file: &PathBuf) -> Result<(), Box<dyn Error>> {
    let current_pid = process::id();

    if lock_file.exists() {
        if let Ok(contents) = fs::read_to_string(lock_file) {
            if let Ok(old_pid) = contents.trim().parse::<u32>() {
                if is_process_running(old_pid) {
                    eprintln!("❌ Ticker is already running (PID: {})!", old_pid);
                    return Err("Ticker is already running!".into());
                } else {
                    eprintln!(
                        "🧹 Cleaning up stale lock file (PID: {} not running)",
                        old_pid
                    );
                    let _ = fs::remove_file(lock_file);
                }
            } else {
                let _ = fs::remove_file(lock_file);
            }
        } else {
            let _ = fs::remove_file(lock_file);
        }
    }

    fs::write(lock_file, current_pid.to_string())?;
    eprintln!("🔒 Lock acquired (PID: {})", current_pid);
    Ok(())
}

fn release_lock_at(lock_file: &PathBuf) {
    if let Ok(contents) = fs::read_to_string(lock_file)
        && let Ok(pid) = contents.trim().parse::<u32>()
        && pid == process::id()
    {
        let _ = fs::remove_file(lock_file);
        eprintln!("🔓 Lock released");
    }
}

pub fn try_acquire_lock() -> Result<(), Box<dyn Error>> {
    let lock_file = lock_file_path();
    try_acquire_lock_at(&lock_file)
}

pub fn release_lock() {
    let lock_file = lock_file_path();
    release_lock_at(&lock_file);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_lock_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("ticker-{}-{}.lock", name, stamp))
    }

    #[test]
    fn test_acquire_lock_success() {
        let lock_file = unique_test_lock_path("acquire_success");
        let _ = fs::remove_file(&lock_file);

        assert!(try_acquire_lock_at(&lock_file).is_ok());

        release_lock_at(&lock_file);
        let _ = fs::remove_file(&lock_file);
    }

    #[test]
    fn test_lock_contains_pid() {
        let lock_file = unique_test_lock_path("contains_pid");
        let _ = fs::remove_file(&lock_file);

        assert!(try_acquire_lock_at(&lock_file).is_ok());

        let contents = fs::read_to_string(&lock_file).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();

        assert_eq!(pid, process::id());

        release_lock_at(&lock_file);
        let _ = fs::remove_file(&lock_file);
    }

    #[test]
    fn test_stale_lock_cleanup() {
        let lock_file = unique_test_lock_path("stale_cleanup");
        let _ = fs::remove_file(&lock_file);

        // Schrijf een fake PID
        fs::write(&lock_file, "999999").unwrap();

        assert!(try_acquire_lock_at(&lock_file).is_ok());

        let contents = fs::read_to_string(&lock_file).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, process::id());

        release_lock_at(&lock_file);
        let _ = fs::remove_file(&lock_file);
    }

    #[test]
    fn test_release_lock_removes_file() {
        let lock_file = unique_test_lock_path("release_removes");
        let _ = fs::remove_file(&lock_file);

        assert!(try_acquire_lock_at(&lock_file).is_ok());
        release_lock_at(&lock_file);

        assert!(!lock_file.exists());
        let _ = fs::remove_file(&lock_file);
    }
}
