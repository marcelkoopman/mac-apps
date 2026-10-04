mod appearance;
mod avro_schema;
mod brackets;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod card_layout;
mod clipboard;
mod column_picker;
mod commands;
mod convert;
mod dataframe;
mod decode;
mod format;
mod highlight;
mod hotkey;
mod icon;
mod image_edit;
mod image_ops;
mod item_find;
mod launcher;
mod memo;
mod menubar;
mod open_file;
mod page_preview;
mod python;
mod recognize;
mod sensitivity;
mod settings;
mod table;
mod table_combine;
mod table_filter;
mod table_ops;
#[cfg(test)]
mod testdata_manifest;
mod toolbar_visibility;
mod transform;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod url_policy;
mod validate;
mod wipe_alloc;
mod xsd_schema;
mod youtube;

/// Freed heap memory is zeroed first ([`wipe_alloc`]), so copied text left in buffers of
/// Polars, AppKit or any other library does not stay readable after use.
#[global_allocator]
static ALLOC: wipe_alloc::WipeOnFree = wipe_alloc::WipeOnFree;

#[cfg(target_os = "macos")]
mod macos_card_text;
#[cfg(target_os = "macos")]
mod macos_image_edit;
#[cfg(target_os = "macos")]
mod macos_image_io;
#[cfg(target_os = "macos")]
mod macos_launcher;
#[cfg(target_os = "macos")]
mod macos_open;
#[cfg(target_os = "macos")]
mod macos_pasteboard;
#[cfg(target_os = "macos")]
mod macos_preview_image;
#[cfg(target_os = "macos")]
mod macos_save;
#[cfg(target_os = "macos")]
mod macos_session;
#[cfg(target_os = "macos")]
mod macos_table_window;
#[cfg(target_os = "macos")]
mod macos_vision;

fn main() {
    // Polars prints 10 rows and 8 columns of a table, in at most 100 characters, by default.
    // The card limits rows itself (`commands::PREVIEW_ROWS`) and wants every column.
    for (name, value) in dataframe::DISPLAY_ENV {
        if std::env::var_os(name).is_none() {
            // SAFETY: first thing in `main`, before any other thread exists.
            unsafe { std::env::set_var(name, value) };
        }
    }
    if let Err(e) = menubar::run() {
        eprintln!("copycraft failed: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name() {
        assert_eq!(env!("CARGO_PKG_NAME"), "copycraft");
    }

    /// The signed app gets exactly these rights: the sandbox and files the user picks. No
    /// network (copycraft works offline), no hardened-runtime exceptions
    /// (`com.apple.security.cs.*`), no Apple Events. scripts/sign_app.sh and the release
    /// workflow check the signed app the same way.
    #[test]
    fn entitlements_are_the_sandbox_and_user_selected_files_only() {
        let plist = include_str!("../assets/entitlements.plist");
        let body = &plist[plist.find("<plist").expect("plist element")..];
        let keys: Vec<&str> = body
            .split("<key>")
            .skip(1)
            .filter_map(|rest| rest.split_once("</key>").map(|(key, _)| key.trim()))
            .collect();
        assert_eq!(
            keys,
            [
                "com.apple.security.app-sandbox",
                "com.apple.security.files.user-selected.read-write",
            ]
        );
        assert_eq!(body.matches("<true/>").count(), 2);
        assert!(!body.contains("<false/>") && !body.contains("<array>"));
    }
}
