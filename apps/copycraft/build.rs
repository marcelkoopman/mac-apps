//! Two things, on Apple silicon macOS only:
//!
//! - Compiles the table window's chart (`swift/CopyCraftCharts/ChartView.swift`, SwiftUI and
//!   Swift Charts) into a static library with `swiftc` and links it in, as mac-ai does with its
//!   bridge (`crates/mac-ai/build.rs`). Without `swiftc` nothing is compiled, a warning says so,
//!   and the `copycraft_charts` cfg stays off: the table window then has no Chart button. A
//!   `swiftc` that exists but fails is a build error, not a silent fallback.
//! - Adds the Swift runtime to the rpath of copycraft's binaries when the chart or the mac-ai
//!   bridge is built in. Both reference `@rpath/libswift_*.dylib`; the system runtime is
//!   `/usr/lib/swift` (macOS 26, dyld cache). A dependency's link args do not reach this binary,
//!   so mac-ai tells this script through `DEP_MAC_AI_SWIFT_BRIDGE`. Nothing is copied into the
//!   app bundle: the library is linked statically, SwiftUI and Charts are system frameworks.
//!
//! Subprocesses (`swiftc`, `xcrun`) run here at build time only; the app itself starts none.

use std::env;
use std::path::{Path, PathBuf};

/// The deployment target of the chart (the app's minimum), unless the SDK is older.
const MIN_MACOS_MAJOR: u32 = 26;

/// The system Swift runtime (in the dyld cache on macOS 26).
const SWIFT_RUNTIME_DIR: &str = "/usr/lib/swift";

const CHART_SOURCE: &str = "swift/CopyCraftCharts/ChartView.swift";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(copycraft_charts)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={CHART_SOURCE}");
    for var in ["SWIFTC", "SDKROOT", "DEVELOPER_DIR"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let charts = build_charts();
    let bridge = env::var_os("DEP_MAC_AI_SWIFT_BRIDGE").is_some();
    if charts || bridge {
        // Once, for both: a second identical -rpath makes the linker warn.
        let dir = env::var("DEP_MAC_AI_SWIFT_RUNTIME_DIR")
            .unwrap_or_else(|_| SWIFT_RUNTIME_DIR.to_string());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}

/// Compile and link the chart; false when this target or build has none.
// The build script may start swiftc and xcrun (AGENTS.md: no subprocesses in the app itself).
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
fn build_charts() -> bool {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if os != "macos" || arch != "aarch64" {
        return false;
    }
    let Some(swiftc) = find_swiftc() else {
        println!(
            "cargo:warning=copycraft: swiftc not found; the table chart is unavailable in this build"
        );
        return false;
    };
    let sdk = env::var("SDKROOT")
        .ok()
        .filter(|path| !path.is_empty())
        .or_else(|| output("xcrun", &["--sdk", "macosx", "--show-sdk-path"]));
    let major = output("xcrun", &["--sdk", "macosx", "--show-sdk-version"])
        .as_deref()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .map_or(MIN_MACOS_MAJOR, |major| major.min(MIN_MACOS_MAJOR));
    let target = format!("arm64-apple-macos{major}.0");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let library = out_dir.join("libcopycraftcharts.a");
    let mut command = std::process::Command::new(&swiftc);
    command
        .args(["-parse-as-library", "-emit-library", "-static", "-O"])
        .args(["-module-name", "CopyCraftCharts", "-target", &target])
        .args(["-swift-version", "5"]);
    if let Some(sdk) = &sdk {
        command.args(["-sdk", sdk]);
    }
    command.arg(CHART_SOURCE).arg("-o").arg(&library);
    // Flags and paths only, never any text.
    let shown = format!("{command:?}");
    let result = command
        .output()
        .unwrap_or_else(|e| panic!("copycraft: could not run swiftc ({shown}): {e}"));
    if !result.status.success() {
        panic!(
            "copycraft: swiftc failed for the table chart ({})\ncommand: {shown}\nswiftc stderr:\n{}\n\
             hint: the chart needs Xcode with the macOS 26 or newer SDK (SwiftUI, Charts). Check:\n  \
             xcode-select -p\n  xcrun --sdk macosx --show-sdk-version\n  xcrun --find swiftc\n\
             DEVELOPER_DIR / SDKROOT in the environment override these (now: DEVELOPER_DIR={:?}, SDKROOT={:?}).",
            result.status,
            String::from_utf8_lossy(&result.stderr),
            env::var("DEVELOPER_DIR").ok(),
            env::var("SDKROOT").ok(),
        );
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=copycraftcharts");
    // The Swift runtime lives in the OS; the linker needs the SDK's stubs for it and the
    // toolchain's compatibility libraries.
    if let Some(sdk) = &sdk {
        println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    }
    if let Some(dir) = Path::new(&swiftc)
        .parent()
        .and_then(Path::parent)
        .map(|usr| usr.join("lib/swift/macosx"))
        .filter(|dir| dir.is_dir())
    {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    for framework in ["AppKit", "SwiftUI", "Charts"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-cfg=copycraft_charts");
    true
}

/// `$SWIFTC`, else the swiftc of the active Xcode (`xcrun --find swiftc`), else `swiftc` on the PATH.
#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
fn find_swiftc() -> Option<String> {
    if let Ok(path) = env::var("SWIFTC") {
        return Some(path);
    }
    output("xcrun", &["--find", "swiftc"]).or_else(|| {
        std::process::Command::new("swiftc")
            .arg("--version")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|_| "swiftc".to_string())
    })
}

#[allow(clippy::disallowed_methods, clippy::disallowed_types)]
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (out.status.success() && !text.is_empty()).then(|| text.to_string())
}
