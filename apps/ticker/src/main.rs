use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

mod config;
mod dialogs;
mod freshness;
mod instance_lock;
mod menu_builder;
mod menu_ids;
mod menubar;
mod poll_gate;
mod price_fetcher;
mod price_history;
mod price_input;
mod price_watch;
mod watch_cli;
mod watch_ui;

// Add dhat allocator (only when feature enabled)
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// `None` without a home directory: then the log goes to stderr only.
fn log_file_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ticker_debug.log"))
}

pub(crate) fn log_message(message: &str) {
    eprintln!("{}", message);

    if let Some(path) = log_file_path()
        && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cli_mode = args.len() > 1;

    // Menubar mode only: a second instance exits before touching the running one's log file.
    // CLI subcommands (watch_cli) never take the lock (the watch file has its own lock).
    let instance_guard = if cli_mode {
        None
    } else {
        match instance_lock::default_lock_path()
            .map_err(|e| e.to_string())
            .and_then(|path| instance_lock::try_acquire(&path).map_err(|e| e.to_string()))
        {
            Ok(Some(lock)) => Some(lock),
            Ok(None) => {
                log_message("Another Ticker instance is already running; exiting.");
                return;
            }
            Err(e) => {
                log_message(&format!(
                    "⚠️  Single-instance lock unavailable ({e}); continuing"
                ));
                None
            }
        }
    };

    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // CLI subcommands print to the terminal and leave the menu bar app's log alone (they run
    // while the app does, without its instance lock).
    if cli_mode {
        match watch_cli::handle_watch_command(&args[1..]) {
            Ok(output) => {
                println!("{}", output);
                return;
            }
            Err(e) => {
                eprintln!("❌ Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Menu bar app only, and only once it holds the instance lock: start a fresh log. Without
    // the lock another instance may be writing to it, so append instead.
    let log_path = log_file_path();
    if instance_guard.is_some()
        && let Some(path) = &log_path
    {
        let _ = std::fs::remove_file(path);
    }

    log_message("=== TICKER APP STARTED ===");
    match &log_path {
        Some(path) => log_message(&format!("Log file: {path:?}")),
        None => log_message("No home directory: logging to stderr only"),
    }
    log_message(&format!("Working dir: {:?}", std::env::current_dir()));
    log_message(&format!("Executable: {:?}", std::env::current_exe()));

    if let Some(lock) = &instance_guard {
        log_message(&format!("Instance lock: {:?}", lock.path()));
    }

    match menubar::run_menubar() {
        Ok(_) => log_message("✓ Ticker app exited normally"),
        Err(e) => {
            let error_msg = format!("✗ FATAL ERROR: {}", e);
            log_message(&error_msg);
            std::process::exit(1);
        }
    }
}
