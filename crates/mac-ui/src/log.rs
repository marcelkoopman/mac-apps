//! Apple unified logging (`os_log`) for the apps: one subsystem (the app's bundle id) and a
//! few fixed categories. Console shows the lines with `subsystem:<bundle id>`.
//!
//! This module takes a ready line. It does not decide what is safe to log: an app keeps its
//! own typed events (copycraft's `diag::LogEvent`, only fixed names and numbers) and is the
//! only caller. The line goes out as one `%{public}s` argument, so the app must never pass
//! content (clipboard text, file names, paths) here.

use std::ffi::{CStr, CString};

/// The categories a line is filed under (Console's Category column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Clipboard,
    Table,
    Decode,
    Settings,
    Window,
    Login,
    Worker,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Clipboard,
        Category::Table,
        Category::Decode,
        Category::Settings,
        Category::Window,
        Category::Login,
        Category::Worker,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Clipboard => "clipboard",
            Category::Table => "table",
            Category::Decode => "decode",
            Category::Settings => "settings",
            Category::Window => "window",
            Category::Login => "login",
            Category::Worker => "worker",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// How a line is kept: debug lines are dropped unless debug logging is on for the subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Error => "error",
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Category, Level};
    use std::ffi::{CStr, c_char, c_void};
    use std::sync::OnceLock;

    unsafe extern "C" {
        fn os_log_create(subsystem: *const c_char, category: *const c_char) -> *mut c_void;
        fn _os_log_impl(
            dso: *const c_void,
            log: *mut c_void,
            kind: u8,
            format: *const c_char,
            buffer: *const u8,
            size: u32,
        );
        static __dso_handle: u8;
    }

    /// `os_log_t` handles, made once per category. They live as long as the process.
    struct Handles([usize; 7]);

    static HANDLES: OnceLock<Handles> = OnceLock::new();
    static SUBSYSTEM: OnceLock<std::ffi::CString> = OnceLock::new();

    /// `%{public}s`: the one argument is the line, already free of content.
    const FORMAT: &CStr = c"%{public}s";

    pub(super) fn set_subsystem(subsystem: std::ffi::CString) {
        let _ = SUBSYSTEM.set(subsystem);
    }

    pub(super) fn write(category: Category, level: Level, line: &CStr) {
        let subsystem = SUBSYSTEM.get_or_init(|| c"copycraft".to_owned());
        let kind = match level {
            Level::Debug => 0x02,
            Level::Info => 0x01,
            Level::Error => 0x10,
        };
        // The argument buffer of `os_log`: summary (bit 1: a non-scalar argument), count, then
        // per argument its descriptor (string 0x20, public 0x02), its size and the pointer.
        let mut buffer = [0u8; 12];
        buffer[0] = 0x02;
        buffer[1] = 1;
        buffer[2] = 0x22;
        buffer[3] = 8;
        buffer[4..12].copy_from_slice(&(line.as_ptr() as usize as u64).to_ne_bytes());
        // SAFETY: `os_log_create` takes two NUL-terminated strings and returns a handle that is
        // never freed (kept in HANDLES for the life of the process); `_os_log_impl` is what the
        // `os_log` macro calls, with this image's `__dso_handle`, a static NUL-terminated format
        // and an argument buffer in the documented layout whose one pointer (`line`) outlives
        // the call. Neither keeps a pointer to `buffer` or `line` after returning.
        unsafe {
            let handles = HANDLES.get_or_init(|| {
                let mut all = [0usize; 7];
                for category in Category::ALL {
                    let name = std::ffi::CString::new(category.name()).unwrap_or_default();
                    all[category.index()] =
                        os_log_create(subsystem.as_ptr(), name.as_ptr()) as usize;
                }
                Handles(all)
            });
            let log = handles.0[category.index()] as *mut c_void;
            if log.is_null() {
                return;
            }
            _os_log_impl(
                (&raw const __dso_handle).cast(),
                log,
                kind,
                FORMAT.as_ptr(),
                buffer.as_ptr(),
                buffer.len() as u32,
            );
        }
    }
}

/// The subsystem every line is filed under: the app's bundle id. Set once, before the first
/// line; later calls change nothing.
pub fn set_subsystem(bundle_id: &str) {
    #[cfg(target_os = "macos")]
    if let Ok(subsystem) = CString::new(bundle_id) {
        imp::set_subsystem(subsystem);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = bundle_id;
}

/// Send `line` to the unified log (on macOS; elsewhere to stderr, for development).
/// A line with a NUL byte is cut there.
pub fn write(category: Category, level: Level, line: &str) {
    let line = line.split('\0').next().unwrap_or("");
    #[cfg(target_os = "macos")]
    if let Ok(line) = CString::new(line) {
        let line: &CStr = &line;
        imp::write(category, level, line);
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("[{}] {}: {line}", category.name(), level.name());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_names_are_the_console_categories() {
        let names: Vec<&str> = Category::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(
            names,
            [
                "clipboard",
                "table",
                "decode",
                "settings",
                "window",
                "login",
                "worker"
            ]
        );
        for (index, category) in Category::ALL.iter().enumerate() {
            assert_eq!(category.index(), index);
        }
    }
}
