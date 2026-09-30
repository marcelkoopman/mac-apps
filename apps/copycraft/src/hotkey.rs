use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// Menu label for the chord registered in [`open`].
pub const LABEL: &str = "⌃⌥⌘F";

pub fn open() -> HotKey {
    HotKey::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER),
        Code::KeyF,
    )
}
