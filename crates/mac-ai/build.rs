//! Compiles `swift/bridge.swift` (Foundation Models) into a static library with `swiftc` and links
//! it in, on Apple silicon macOS only. On any other target, or when `swiftc` is missing, nothing is
//! compiled and the crate reports the model as unavailable (the `mac_ai_swift` cfg stays off).
//! The bridge is linked statically into the binary: nothing extra goes into the app bundle.
//!
//! A `swiftc` that exists but fails (for example an SDK older than macOS 26, with no
//! FoundationModels) is a build error, not a silent fallback: a release must not ship without it.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Foundation Models needs macOS 26: the deployment target of the bridge, unless the installed
/// SDK is older (then the SDK's own version, and swiftc says what is missing).
const MIN_MACOS_MAJOR: u32 = 26;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(mac_ai_swift)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=swift/bridge.swift");
    for var in ["SWIFTC", "SDKROOT", "DEVELOPER_DIR"] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if os != "macos" || arch != "aarch64" {
        return;
    }
    let Some(swiftc) = find_swiftc() else {
        println!(
            "cargo:warning=mac-ai: swiftc not found; Apple Intelligence actions are unavailable in this build"
        );
        return;
    };

    // The SDK of the active Xcode (xcrun honours DEVELOPER_DIR), or $SDKROOT when set.
    let sdk = env::var("SDKROOT")
        .ok()
        .filter(|path| !path.is_empty())
        .or_else(|| output("xcrun", &["--sdk", "macosx", "--show-sdk-path"]));
    let sdk_version = output("xcrun", &["--sdk", "macosx", "--show-sdk-version"]);
    let major = sdk_version
        .as_deref()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .map_or(MIN_MACOS_MAJOR, |major| major.min(MIN_MACOS_MAJOR));
    let target = format!("arm64-apple-macos{major}.0");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let library = out_dir.join("libmacaibridge.a");
    let mut command = Command::new(&swiftc);
    command
        .args(["-emit-library", "-static", "-parse-as-library", "-O"])
        .args(["-module-name", "MacAiBridge", "-target", &target])
        .args(["-swift-version", "5"]);
    if let Some(sdk) = &sdk {
        command.args(["-sdk", sdk]);
    }
    command.arg("swift/bridge.swift").arg("-o").arg(&library);
    // Flags and paths only, never any text.
    let shown = format!("{command:?}");
    let result = command
        .output()
        .unwrap_or_else(|e| panic!("mac-ai: could not run swiftc ({shown}): {e}"));
    if !result.status.success() {
        panic!(
            "mac-ai: swiftc failed ({})\ncommand: {shown}\nswiftc stderr:\n{}\n\
             hint: mac-ai needs Xcode with the macOS 26 or newer SDK. Check:\n  \
             xcode-select -p                      (the active developer directory; Xcode, not the Command Line Tools)\n  \
             xcrun --sdk macosx --show-sdk-version (26 or higher)\n  \
             xcrun --find swiftc                   (inside the active Xcode)\n\
             DEVELOPER_DIR / SDKROOT in the environment override these (now: DEVELOPER_DIR={:?}, SDKROOT={:?}).",
            result.status,
            String::from_utf8_lossy(&result.stderr),
            env::var("DEVELOPER_DIR").ok(),
            env::var("SDKROOT").ok(),
        );
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=macaibridge");
    // The Swift runtime lives in the OS (/usr/lib/swift); the linker needs the SDK's stubs for it,
    // and the toolchain's back-deployment libraries.
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
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=FoundationModels");
    println!("cargo:rustc-cfg=mac_ai_swift");
}

/// `$SWIFTC`, else the swiftc of the active Xcode (`xcrun --find swiftc`), else `swiftc` on the PATH.
fn find_swiftc() -> Option<String> {
    if let Ok(path) = env::var("SWIFTC") {
        return Some(path);
    }
    output("xcrun", &["--find", "swiftc"]).or_else(|| {
        Command::new("swiftc")
            .arg("--version")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|_| "swiftc".to_string())
    })
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (out.status.success() && !text.is_empty()).then(|| text.to_string())
}
