//! Global hotkeys: the one that summons the card (default ⌃⌘C) and the one that starts a
//! screenshot (default ⌃⇧4). Both are stored in the user defaults.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// Menu / Settings label for the default chord.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_LABEL: &str = "⌃⌘C";

/// Storage form of the default ([`open`]). An older stored `super` still parses as Command.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_STORAGE: &str = "control+meta+KeyC";

/// The previous default. A stored copy of it follows [`open`] so the chord moves with the app.
const PREVIOUS_DEFAULT_STORAGE: &str = "control+alt+super+KeyC";

/// The chord registered at launch when none is stored.
pub fn open() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::META), Code::KeyC)
}

/// Menu / Settings label for the default screenshot chord ([`screenshot`]).
#[cfg_attr(not(test), allow(dead_code))]
pub const SCREENSHOT_LABEL: &str = "⌃⇧4";

/// The chord that starts a screenshot when none is stored: ⇧⌘4 with Control instead of
/// Command, so it sits next to the system's own chords without taking one of them.
pub fn screenshot() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Digit4)
}

/// The screenshot chord from `storage`, or [`screenshot`] when missing or unsafe.
pub fn screenshot_from_storage(storage: Option<&str>) -> HotKey {
    storage
        .and_then(|text| text.parse::<HotKey>().ok())
        .filter(is_safe)
        .unwrap_or_else(screenshot)
}

/// Whether `chord` may be stored next to `other` (the other global hotkey): safe, and not the
/// same chord, which would register only once.
pub fn may_store(chord: &HotKey, other: &HotKey) -> bool {
    is_safe(chord) && chord.id() != other.id()
}

/// The chord from `storage` (a [`HotKey`] string), or the default when missing, unsafe, or still
/// the previous default.
pub fn from_storage(storage: Option<&str>) -> HotKey {
    storage
        .filter(|text| *text != PREVIOUS_DEFAULT_STORAGE)
        .and_then(|text| text.parse::<HotKey>().ok())
        .filter(is_safe)
        .unwrap_or_else(open)
}

/// Whether `chord` is strong enough: not a bare key, not Option alone, not Shift alone
/// (Option-C while typing is too easy to hit). Control-Option-C and Control-Command-C are fine.
pub fn is_safe(chord: &HotKey) -> bool {
    let mods =
        chord.mods & (Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT);
    if mods.is_empty() || mods == Modifiers::ALT || mods == Modifiers::SHIFT {
        return false;
    }
    true
}

/// Compact Mac label (`⌃⌘C`) for menus and Settings.
pub fn label(chord: &HotKey) -> String {
    let mut out = String::new();
    if chord.mods.contains(Modifiers::CONTROL) {
        out.push('⌃');
    }
    if chord.mods.contains(Modifiers::ALT) {
        out.push('⌥');
    }
    if chord.mods.contains(Modifiers::SHIFT) {
        out.push('⇧');
    }
    if chord.mods.contains(Modifiers::META) {
        out.push('⌘');
    }
    out.push_str(&key_glyph(chord.key));
    out
}

/// Storage string for the user defaults.
pub fn to_storage(chord: &HotKey) -> String {
    chord.into_string()
}

fn key_glyph(code: Code) -> String {
    match code {
        Code::KeyA => "A".into(),
        Code::KeyB => "B".into(),
        Code::KeyC => "C".into(),
        Code::KeyD => "D".into(),
        Code::KeyE => "E".into(),
        Code::KeyF => "F".into(),
        Code::KeyG => "G".into(),
        Code::KeyH => "H".into(),
        Code::KeyI => "I".into(),
        Code::KeyJ => "J".into(),
        Code::KeyK => "K".into(),
        Code::KeyL => "L".into(),
        Code::KeyM => "M".into(),
        Code::KeyN => "N".into(),
        Code::KeyO => "O".into(),
        Code::KeyP => "P".into(),
        Code::KeyQ => "Q".into(),
        Code::KeyR => "R".into(),
        Code::KeyS => "S".into(),
        Code::KeyT => "T".into(),
        Code::KeyU => "U".into(),
        Code::KeyV => "V".into(),
        Code::KeyW => "W".into(),
        Code::KeyX => "X".into(),
        Code::KeyY => "Y".into(),
        Code::KeyZ => "Z".into(),
        Code::Digit0 => "0".into(),
        Code::Digit1 => "1".into(),
        Code::Digit2 => "2".into(),
        Code::Digit3 => "3".into(),
        Code::Digit4 => "4".into(),
        Code::Digit5 => "5".into(),
        Code::Digit6 => "6".into(),
        Code::Digit7 => "7".into(),
        Code::Digit8 => "8".into(),
        Code::Digit9 => "9".into(),
        Code::Space => "Space".into(),
        other => format!("{other:?}").replace("Key", ""),
    }
}

/// The Carbon / `NSEvent.keyCode` for letters and digits, for recording a chord in Settings.
pub fn code_from_key_code(key_code: u16) -> Option<Code> {
    // HIToolbox kVK_ANSI_* values.
    Some(match key_code {
        0x00 => Code::KeyA,
        0x0B => Code::KeyB,
        0x08 => Code::KeyC,
        0x02 => Code::KeyD,
        0x0E => Code::KeyE,
        0x03 => Code::KeyF,
        0x05 => Code::KeyG,
        0x04 => Code::KeyH,
        0x22 => Code::KeyI,
        0x26 => Code::KeyJ,
        0x28 => Code::KeyK,
        0x25 => Code::KeyL,
        0x2E => Code::KeyM,
        0x2D => Code::KeyN,
        0x1F => Code::KeyO,
        0x23 => Code::KeyP,
        0x0C => Code::KeyQ,
        0x0F => Code::KeyR,
        0x01 => Code::KeyS,
        0x11 => Code::KeyT,
        0x20 => Code::KeyU,
        0x09 => Code::KeyV,
        0x0D => Code::KeyW,
        0x07 => Code::KeyX,
        0x10 => Code::KeyY,
        0x06 => Code::KeyZ,
        0x1D => Code::Digit0,
        0x12 => Code::Digit1,
        0x13 => Code::Digit2,
        0x14 => Code::Digit3,
        0x15 => Code::Digit4,
        0x17 => Code::Digit5,
        0x16 => Code::Digit6,
        0x1A => Code::Digit7,
        0x1C => Code::Digit8,
        0x19 => Code::Digit9,
        0x31 => Code::Space,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::{
        DEFAULT_LABEL, DEFAULT_STORAGE, SCREENSHOT_LABEL, from_storage, is_safe, label, may_store,
        open, screenshot, screenshot_from_storage, to_storage,
    };

    #[test]
    fn the_screenshot_chord_is_control_shift_4_and_round_trips() {
        let chord = screenshot();
        assert_eq!(label(&chord), SCREENSHOT_LABEL);
        assert_eq!(chord.key, Code::Digit4);
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert!(is_safe(&chord));
        assert_ne!(chord.id(), open().id());
        let stored = to_storage(&chord);
        assert_eq!(screenshot_from_storage(Some(&stored)).id(), chord.id());
        // Another safe chord is kept; nothing, rubbish or an unsafe chord gives the default.
        let other = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit5);
        assert_eq!(
            screenshot_from_storage(Some(&to_storage(&other))).id(),
            other.id()
        );
        assert_eq!(screenshot_from_storage(None).id(), chord.id());
        assert_eq!(screenshot_from_storage(Some("nonsense")).id(), chord.id());
        assert_eq!(
            screenshot_from_storage(Some("shift+Digit4")).id(),
            chord.id()
        );
        // The previous default of the card's chord is a valid screenshot chord here.
        let previous = screenshot_from_storage(Some("control+alt+super+KeyC"));
        assert_eq!(previous.key, Code::KeyC);
    }

    #[test]
    fn the_two_hotkeys_may_not_be_the_same_chord() {
        assert!(may_store(&screenshot(), &open()));
        assert!(may_store(&open(), &screenshot()));
        assert!(!may_store(&open(), &open()));
        assert!(!may_store(&screenshot(), &screenshot()));
        assert!(!may_store(
            &HotKey::new(Some(Modifiers::SHIFT), Code::Digit4),
            &open()
        ));
    }

    #[test]
    fn default_chord_is_control_command_c() {
        assert_eq!(label(&open()), DEFAULT_LABEL);
        assert_eq!(to_storage(&open()), DEFAULT_STORAGE);
        let chord = open();
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::META);
        assert_eq!(
            from_storage(Some("control+alt+super+KeyC")).id(),
            open().id()
        );
        assert_eq!(from_storage(Some("control+super+KeyC")).id(), open().id());
    }

    #[test]
    fn option_alone_and_bare_keys_are_rejected() {
        assert!(!is_safe(&HotKey::new(Some(Modifiers::ALT), Code::KeyC)));
        assert!(!is_safe(&HotKey::new(Some(Modifiers::SHIFT), Code::KeyC)));
        assert!(!is_safe(&HotKey::new(None, Code::KeyC)));
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::ALT),
            Code::KeyC
        )));
        assert!(is_safe(&HotKey::new(Some(Modifiers::META), Code::KeyC)));
        assert!(is_safe(&open()));
    }

    #[test]
    fn storage_roundtrips_and_falls_back_when_unsafe() {
        let chord = from_storage(Some("control+alt+KeyC"));
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(from_storage(Some("alt+KeyC")).id(), open().id());
        assert_eq!(from_storage(None).id(), open().id());
    }

    #[test]
    fn hotkey_leaves_plain_copy_and_find_to_the_card() {
        let chord = open();
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyC));
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyF));
    }
}
