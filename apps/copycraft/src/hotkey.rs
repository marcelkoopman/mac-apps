//! Global hotkeys: the one that summons the card (default ⌃⌥Space) and the one that starts a
//! screenshot (default ⌃⌥4), written in the Mac order ⌃⌥⇧⌘. Both are stored in the user
//! defaults; a default is never stored, so a stored chord is one the user recorded in Settings.
//!
//! The defaults are the same on every keyboard (ISO, ANSI, JIS): Space (`kVK_Space`) and 4
//! (`kVK_ANSI_4`) sit in the same place on all of them, and with Control and Option together
//! they type nothing. With more than one input source macOS may use ⌃⌥Space itself (*Select
//! next source in Input menu*); a chord that cannot be registered is marked taken ([`set_taken`])
//! and Settings says so.

use std::sync::atomic::{AtomicBool, Ordering};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// Storage form of the card's default ([`open`]); never stored by CopyCraft itself.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_STORAGE: &str = "control+alt+Space";

/// The default before ⌃⌘C. A stored copy of it follows [`open`] so the chord moves with the
/// app. Every later default (⌃⌘C, ⌥§, ⌃⌥`; for screenshots ⌃⇧4, ⌥⇧§, ⌃⌥⇧`) stays when stored:
/// CopyCraft never stored a default, so a stored one was recorded by the user.
const PREVIOUS_DEFAULT_STORAGE: &str = "control+alt+super+KeyC";

/// The chord registered when none is stored: ⌃⌥Space.
pub fn open() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
}

/// The chord that starts a screenshot when none is stored: ⌃⌥4.
pub fn screenshot() -> HotKey {
    HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit4)
}

/// The card's hotkey could not be registered (another app or macOS holds it).
static CARD_TAKEN: AtomicBool = AtomicBool::new(false);
/// The screenshot hotkey could not be registered.
static SHOT_TAKEN: AtomicBool = AtomicBool::new(false);

/// Note whether the card's chord (`screenshot` false) or the screenshot chord could not be
/// registered, for Settings ([`is_taken`]).
pub fn set_taken(screenshot: bool, taken: bool) {
    let slot = if screenshot { &SHOT_TAKEN } else { &CARD_TAKEN };
    slot.store(taken, Ordering::Relaxed);
}

/// Whether the card's chord (`screenshot` false) or the screenshot chord is not registered
/// because it is taken.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
pub fn is_taken(screenshot: bool) -> bool {
    let slot = if screenshot { &SHOT_TAKEN } else { &CARD_TAKEN };
    slot.load(Ordering::Relaxed)
}

/// The screenshot chord from `storage`, or [`screenshot`] when missing or unsafe. A stored
/// earlier default (⌃⇧4, ⌥⇧§) stays: it was recorded by the user.
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

/// Whether `chord` is strong enough: not a bare key, not Shift alone, and Option or
/// Option-Shift alone only on § (ISO, top left). Those type characters (⌥C is ç, ⌥⇧O is Ø,
/// ⌥` a dead key on ANSI), so on any other key a hotkey would take them away. With Control or
/// Command too nothing is typed: the defaults ⌃⌥Space and ⌃⌥4, ⌃⌥C and ⌃⌘C are fine.
pub fn is_safe(chord: &HotKey) -> bool {
    let mods =
        chord.mods & (Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT);
    if mods.is_empty() || mods == Modifiers::SHIFT {
        return false;
    }
    let typing = mods == Modifiers::ALT || mods == Modifiers::ALT | Modifiers::SHIFT;
    !typing || chord.key == Code::IntlBackslash
}

/// Compact Mac label (`⌃⌥Space`, `⌃⌘C`; Space is *Spatie* in Dutch) for menus and Settings.
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
        Code::Space => crate::locale::t("key_space").into(),
        Code::IntlBackslash => "§".into(),
        Code::Backquote => "`".into(),
        other => format!("{other:?}").replace("Key", ""),
    }
}

/// The Carbon / `NSEvent.keyCode` for letters, digits, Space and the key top left (§ or `), for
/// recording a chord in Settings.
pub fn code_from_key_code(key_code: u16) -> Option<Code> {
    // HIToolbox kVK_ANSI_* values, and kVK_ISO_Section.
    Some(match key_code {
        mac_ui::keys::code::ISO_SECTION => Code::IntlBackslash,
        mac_ui::keys::code::ANSI_GRAVE => Code::Backquote,
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
        DEFAULT_STORAGE, code_from_key_code, from_storage, is_safe, is_taken, label, may_store,
        open, screenshot, screenshot_from_storage, set_taken, to_storage,
    };

    #[test]
    fn the_defaults_are_control_option_space_and_four_on_every_keyboard() {
        let (card, shot) = (open(), screenshot());
        assert_eq!(card.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(card.key, Code::Space);
        assert_eq!(shot.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(shot.key, Code::Digit4);
        assert_eq!(label(&card), "⌃⌥Space");
        assert_eq!(label(&shot), "⌃⌥4");
        assert_eq!(to_storage(&card), DEFAULT_STORAGE);
        assert!(is_safe(&card) && is_safe(&shot));
        // kVK_Space and kVK_ANSI_4: the same keys on ISO, ANSI and JIS.
        assert_eq!(code_from_key_code(0x31), Some(Code::Space));
        assert_eq!(code_from_key_code(0x15), Some(Code::Digit4));
        // The other top left keys can still be recorded.
        assert_eq!(code_from_key_code(0x0A), Some(Code::IntlBackslash));
        assert_eq!(code_from_key_code(0x32), Some(Code::Backquote));
    }

    #[test]
    fn space_is_named_in_the_language_of_the_app() {
        crate::locale::set_test_language(crate::settings::Language::Nl);
        assert_eq!(label(&open()), "⌃⌥Spatie");
        crate::locale::set_test_language(crate::settings::Language::En);
        assert_eq!(label(&open()), "⌃⌥Space");
    }

    #[test]
    fn a_taken_chord_is_remembered_per_hotkey() {
        set_taken(true, true);
        assert!(is_taken(true));
        set_taken(true, false);
        assert!(!is_taken(true));
        // The card's flag is only touched by the card (and by no other test).
        assert!(!is_taken(false));
        set_taken(false, true);
        assert!(is_taken(false));
        set_taken(false, false);
        assert!(!is_taken(false));
    }

    #[test]
    fn the_screenshot_chord_round_trips() {
        let chord = screenshot();
        assert_ne!(chord.id(), open().id());
        let stored = to_storage(&chord);
        assert_eq!(screenshot_from_storage(Some(&stored)), chord);
        // Another safe chord is kept; nothing, rubbish or an unsafe chord gives the default.
        let other = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit5);
        assert_eq!(screenshot_from_storage(Some(&to_storage(&other))), other);
        assert_eq!(screenshot_from_storage(None), chord);
        assert_eq!(screenshot_from_storage(Some("nonsense")), chord);
        assert_eq!(screenshot_from_storage(Some("shift+Digit4")), chord);
        // The previous default of the card's chord is a valid screenshot chord here.
        let previous = screenshot_from_storage(Some("control+alt+super+KeyC"));
        assert_eq!(previous.key, Code::KeyC);
    }

    #[test]
    fn recorded_chords_keep_their_value_when_the_defaults_move() {
        // The old defaults were never stored by CopyCraft, so a stored one was recorded.
        let card = from_storage(Some("control+meta+KeyC"));
        assert_eq!(card.mods, Modifiers::CONTROL | Modifiers::META);
        assert_eq!(card.key, Code::KeyC);
        assert_eq!(from_storage(Some("control+super+KeyC")), card);
        let shot = screenshot_from_storage(Some("shift+control+Digit4"));
        assert_eq!(shot.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(shot.key, Code::Digit4);
        // ⌥§ / ⌥⇧§ (ISO) and ⌃⌥` / ⌃⌥⇧` (ANSI), the defaults before ⌃⌥Space, stay as recorded.
        for (stored, key) in [
            ("alt+IntlBackslash", Code::IntlBackslash),
            ("control+alt+Backquote", Code::Backquote),
        ] {
            assert_eq!(from_storage(Some(stored)).key, key, "{stored}");
        }
        for (stored, key) in [
            ("shift+alt+IntlBackslash", Code::IntlBackslash),
            ("shift+control+alt+Backquote", Code::Backquote),
        ] {
            assert_eq!(screenshot_from_storage(Some(stored)).key, key, "{stored}");
        }
        // Nothing stored: the new defaults.
        assert_eq!(from_storage(None), open());
        assert_eq!(screenshot_from_storage(None), screenshot());
        // The default before ⌃⌘C still follows the default, as it did.
        assert_eq!(from_storage(Some("control+alt+super+KeyC")), open());
    }

    #[test]
    fn the_two_hotkeys_may_not_be_the_same_chord() {
        let (card, shot) = (open(), screenshot());
        assert_ne!(card.id(), shot.id());
        assert!(may_store(&shot, &card));
        assert!(may_store(&card, &shot));
        assert!(!may_store(&card, &card));
        assert!(!may_store(&shot, &shot));
        assert!(!may_store(
            &HotKey::new(Some(Modifiers::SHIFT), Code::Digit4),
            &card
        ));
    }

    #[test]
    fn option_alone_only_on_the_key_top_left() {
        assert!(!is_safe(&HotKey::new(Some(Modifiers::ALT), Code::KeyC)));
        assert!(!is_safe(&HotKey::new(Some(Modifiers::ALT), Code::Digit1)));
        assert!(!is_safe(&HotKey::new(Some(Modifiers::SHIFT), Code::KeyC)));
        assert!(!is_safe(&HotKey::new(None, Code::KeyC)));
        assert!(!is_safe(&HotKey::new(None, Code::IntlBackslash)));
        assert!(!is_safe(&HotKey::new(
            Some(Modifiers::SHIFT),
            Code::IntlBackslash
        )));
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::ALT),
            Code::IntlBackslash
        )));
        // ⌥` is a dead key on ANSI (à), ⌥< on ISO types ≤: not alone.
        assert!(!is_safe(&HotKey::new(
            Some(Modifiers::ALT),
            Code::Backquote
        )));
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::ALT),
            Code::Backquote
        )));
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::ALT),
            Code::KeyC
        )));
        assert!(is_safe(&HotKey::new(Some(Modifiers::META), Code::KeyC)));
        assert!(is_safe(&open()));
        assert!(is_safe(&screenshot()));
    }

    #[test]
    fn option_shift_alone_only_on_the_key_top_left() {
        let option_shift = Some(Modifiers::ALT | Modifiers::SHIFT);
        // ⌥⇧O types Ø, ⌥⇧2 €: a hotkey there would take the character away.
        for key in [
            Code::KeyO,
            Code::KeyC,
            Code::Digit2,
            Code::Space,
            Code::Backquote,
        ] {
            assert!(!is_safe(&HotKey::new(option_shift, key)), "{key:?}");
        }
        assert!(is_safe(&HotKey::new(option_shift, Code::IntlBackslash)));
        // With Control or Command too it types nothing: fine on any key.
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
            Code::KeyO
        )));
        assert!(is_safe(&HotKey::new(
            Some(Modifiers::META | Modifiers::ALT | Modifiers::SHIFT),
            Code::KeyO
        )));
        // A stored ⌥⇧O falls back to the default.
        assert_eq!(from_storage(Some("shift+alt+KeyO")), open());
        assert_eq!(
            screenshot_from_storage(Some("shift+alt+KeyO")),
            screenshot()
        );
    }

    #[test]
    fn storage_roundtrips_and_falls_back_when_unsafe() {
        let chord = from_storage(Some("control+alt+KeyC"));
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(from_storage(Some("alt+KeyC")), open());
        assert_eq!(from_storage(None), open());
        assert_eq!(from_storage(Some(DEFAULT_STORAGE)), open());
    }

    #[test]
    fn hotkey_leaves_plain_copy_and_find_to_the_card() {
        for chord in [open(), screenshot()] {
            assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyC));
            assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyF));
        }
    }
}
