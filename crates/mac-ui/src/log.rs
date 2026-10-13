//! Apple unified logging (`os_log`) for the apps: one subsystem (the app's bundle id) and a
//! few fixed categories. Console shows the lines with `subsystem:<bundle id>`.
//!
//! This module takes a ready line. It does not decide what is safe to log: an app keeps its
//! own typed events (copycraft's `diag::LogEvent`, only fixed names and numbers) and is the
//! only caller. The line goes out as one `%{public}s` argument, so the app must never pass
//! content (clipboard text, file names, paths) here.

#[cfg(target_os = "macos")]
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

    /// How many categories there are: one `os_log_t` handle each.
    pub const COUNT: usize = Category::ALL.len();

    fn index(self) -> usize {
        self as usize
    }
}

/// How a line is kept: debug lines are dropped unless debug logging is on for the subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    /// A real failure.
    Error,
    /// A bug or broken invariant in the app itself.
    Fault,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Error => "error",
            Level::Fault => "fault",
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Category, Level};
    use std::ffi::{CStr, c_char, c_void};
    use std::sync::OnceLock;

    // Why `_os_log_impl`: `os_log(…)` is a C macro, not a function, so Rust cannot call it; the
    // macro expands to `_os_log_impl` (libSystem, exported since macOS 10.12). It is only called
    // on macOS 14 or later: `set_subsystem` checks the running version at startup against
    // `MIN_MACOS` (copycraft's `minimum_system_version`) and on an older Mac no line goes to the
    // system log at all, the rest of the app works on. The alternative without an
    // internal symbol is a small C or Swift shim calling `os_log` (a build step and a C
    // compiler, which this workspace avoids) or a logging crate (a new dependency). CI checks
    // that a line written here reads back with `log show` (`os_log_round_trip`), so a change
    // in the private ABI fails there instead of on a user's Mac.
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
    struct Handles([usize; Category::COUNT]);

    static HANDLES: OnceLock<Handles> = OnceLock::new();
    static SUBSYSTEM: OnceLock<std::ffi::CString> = OnceLock::new();
    /// Whether this macOS is new enough for `_os_log_impl` (see `MIN_MACOS`). Read once.
    static ENABLED: OnceLock<bool> = OnceLock::new();

    /// The running macOS as (major, minor, patch), from `NSProcessInfo`.
    fn running_version() -> (i64, i64, i64) {
        let version = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
        (
            version.majorVersion as i64,
            version.minorVersion as i64,
            version.patchVersion as i64,
        )
    }

    /// Checks the macOS version once. On a Mac older than `MIN_MACOS` the system log stays off
    /// and one line with only the version number goes to stderr.
    pub(super) fn enabled() -> bool {
        *ENABLED.get_or_init(|| {
            let (major, minor, patch) = running_version();
            let ok = super::supported(major, minor);
            if !ok {
                eprintln!(
                    "system log off: macOS {major}.{minor}.{patch} is older than {}.{}",
                    super::MIN_MACOS.0,
                    super::MIN_MACOS.1
                );
            }
            ok
        })
    }

    /// `%{public}s`: the one argument is the line, already free of content.
    const FORMAT: &CStr = c"%{public}s";

    pub(super) fn set_subsystem(subsystem: std::ffi::CString) {
        let _ = SUBSYSTEM.set(subsystem);
        enabled();
    }

    pub(super) fn write(category: Category, level: Level, line: &CStr) {
        // When in doubt, drop the line.
        if line.to_bytes().len() > super::MAX_LINE
            || category.index() >= Category::COUNT
            || !enabled()
        {
            return;
        }
        let subsystem = SUBSYSTEM.get_or_init(|| c"copycraft".to_owned());
        let kind = match level {
            Level::Debug => 0x02,
            Level::Info => 0x01,
            Level::Error => 0x10,
            Level::Fault => 0x11,
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
                let mut all = [0usize; Category::COUNT];
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
/// Never panics: a line that cannot be written (a panic while preparing it, a handle that
/// could not be made) is dropped silently. Logging must not take the app down.
pub fn write(category: Category, level: Level, line: &str) {
    let _ = std::panic::catch_unwind(|| write_line(category, level, line));
}

/// The first [`MAX_LINE`] bytes of `line` up to a NUL, cut on a character boundary.
fn clean(line: &str) -> &str {
    let line = line.split('\0').next().unwrap_or("");
    if line.len() <= MAX_LINE {
        return line;
    }
    let mut end = MAX_LINE;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

/// The oldest macOS (major, minor) that gets lines in the system log; copycraft's
/// `minimum_system_version`. `_os_log_impl` itself exists since 10.12; older Macs than this are
/// not tested, so the log stays off there.
pub const MIN_MACOS: (i64, i64) = (14, 0);

/// Whether macOS `major.minor` is at least [`MIN_MACOS`]. Unreadable versions (0 or negative)
/// count as not supported.
fn supported(major: i64, minor: i64) -> bool {
    major > 0 && minor >= 0 && (major, minor) >= MIN_MACOS
}

/// Lines are cut to this many bytes.
pub const MAX_LINE: usize = 1000;

fn write_line(category: Category, level: Level, line: &str) {
    let line = clean(line);
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

    #[test]
    fn version_check_compares_major_then_minor() {
        assert!(supported(14, 0));
        assert!(supported(14, 6));
        assert!(supported(15, 0));
        assert!(supported(26, 1));
        assert!(!supported(13, 7));
        assert!(!supported(13, 99));
        assert!(!supported(12, 0));
        assert!(!supported(10, 15));
        assert!(!supported(0, 0));
        assert!(!supported(-1, 20));
        assert!(!supported(14, -1));
    }

    /// Adding a category means updating this match (it does not compile otherwise), `ALL` (the
    /// count below fails otherwise) and with it the handle table, which is sized by `COUNT`.
    #[test]
    fn every_category_has_a_handle_slot() {
        fn listed(category: Category) -> bool {
            match category {
                Category::Clipboard
                | Category::Table
                | Category::Decode
                | Category::Settings
                | Category::Window
                | Category::Login
                | Category::Worker => Category::ALL.contains(&category),
            }
        }
        let variants = [
            Category::Clipboard,
            Category::Table,
            Category::Decode,
            Category::Settings,
            Category::Window,
            Category::Login,
            Category::Worker,
        ];
        assert!(variants.iter().all(|c| listed(*c)));
        assert_eq!(Category::COUNT, variants.len());
        let highest = variants.iter().map(|c| c.index()).max().unwrap_or(0);
        assert_eq!(highest + 1, Category::COUNT);
    }

    #[test]
    fn odd_lines_never_panic_and_are_cut() {
        let long = "é".repeat(2000);
        assert!(clean(&long).len() <= MAX_LINE);
        assert!(clean(&long).is_char_boundary(clean(&long).len()));
        assert_eq!(clean("a\0b"), "a");
        for category in Category::ALL {
            for level in [Level::Debug, Level::Info, Level::Error, Level::Fault] {
                write(category, level, "");
                write(category, level, &long);
                write(category, level, "\0");
            }
        }
    }

    /// Writes one fixed line without content. CI reads it back with `log show` (ci.yml,
    /// "os_log round trip"): if nothing arrives, the private `_os_log_impl` ABI changed.
    /// Ignored locally; run with `--ignored` on a Mac.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn os_log_round_trip() {
        set_subsystem("com.github.marcelkoopman.mac-apps.oslog-test");
        write(Category::Worker, Level::Error, "os_log round trip 7f3a");
    }
}
