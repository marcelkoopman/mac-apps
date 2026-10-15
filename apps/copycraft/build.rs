//! Adds the Swift runtime to the rpath of copycraft's binaries when the mac-ai bridge is built in.
//! The bridge references `@rpath/libswift_*.dylib`; the system runtime is `/usr/lib/swift`
//! (macOS 26, dyld cache). A dependency's link args do not reach this binary, so mac-ai tells
//! this script through `DEP_MAC_AI_SWIFT_BRIDGE` (see `crates/mac-ai/build.rs`). Nothing is
//! copied into the app bundle.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("DEP_MAC_AI_SWIFT_BRIDGE").is_some() {
        let dir = std::env::var("DEP_MAC_AI_SWIFT_RUNTIME_DIR")
            .unwrap_or_else(|_| "/usr/lib/swift".to_string());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
