use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

mod config;
mod menu_builder;
mod menubar;
mod price_fetcher;
mod price_history;
mod price_watch;
mod watch_cli;
mod watch_ui;

// Add dhat allocator (only when feature enabled)
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn log_file_path() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot find home directory");
    home.join(".ticker_debug.log")
}

fn log_message(message: &str) {
    eprintln!("{}", message);

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path())
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

fn main() {
    let log_path = log_file_path();
    let _ = std::fs::remove_file(&log_path);

    log_message("=== TICKER APP STARTED ===");
    log_message(&format!("Log file: {:?}", log_path));
    log_message(&format!("Working dir: {:?}", std::env::current_dir()));
    log_message(&format!("Executable: {:?}", std::env::current_exe()));

    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Check for CLI arguments for watch management
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
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

    match menubar::run_menubar() {
        Ok(_) => log_message("✓ Ticker app exited normally"),
        Err(e) => {
            let error_msg = format!("✗ FATAL ERROR: {}", e);
            log_message(&error_msg);
            std::process::exit(1);
        }
    }
}
