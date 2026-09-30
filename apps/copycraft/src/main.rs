mod appearance;
mod avro_schema;
mod clipboard;
mod commands;
mod convert;
mod dataframe;
mod decode;
mod format;
mod highlight;
mod hotkey;
mod icon;
mod image_ops;
mod launcher;
mod menubar;
mod open_file;
mod page_preview;
mod redact;
mod sensitivity;
mod toolbar_visibility;
mod transform;
mod validate;
mod xsd_schema;
mod youtube;

#[cfg(target_os = "macos")]
mod macos_card_text;
#[cfg(target_os = "macos")]
mod macos_fetch;
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
mod macos_vision;

fn main() {
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
}
