//! Global hotkeys: the one that summons the card (default ⌥§) and the one that starts a
//! screenshot (default ⌥⇧§, Option-Shift-§, written in the Mac order ⌃⌥⇧⌘). Both are stored
//! in the user defaults; a default is never stored, so a stored chord is one the user
//! recorded in Settings.
//!
//! Both defaults sit on the key top left, next to 1, by its position, not its character: § on
//! ISO keyboards (`kVK_ISO_Section`, [`Code::IntlBackslash`]), ` on ANSI keyboards
//! (`kVK_ANSI_Grave`, [`Code::Backquote`]). [`board`] asks macOS which keyboard it is.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use mac_ui::keys::Board;

/// Menu / Settings label for the default chord on an ISO keyboard.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_LABEL: &str = "⌥§";

/// Storage form of the default ([`open`]) on an ISO keyboard.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_STORAGE: &str = "alt+IntlBackslash";

/// The default before ⌃⌘C. A stored copy of it follows [`open`] so the chord moves with the
/// app. ⌃⌘C itself (`control+meta+KeyC`, the default until ⌥§) stays: CopyCraft never stored
/// a default, so a stored ⌃⌘C was recorded by the user.
const PREVIOUS_DEFAULT_STORAGE: &str = "control+alt+super+KeyC";

/// The keyboard's layout, for the default chords; ISO when macOS does not know it (and off
/// macOS). A JIS keyboard has no key there, so it gets the ISO code too.
pub fn board() -> Board {
    #[cfg(target_os = "macos")]
    {
        mac_ui::keys::board().unwrap_or(Board::Iso)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Board::Iso
    }
}

/// The key top left, next to 1, on `board`: § on ISO (and JIS), ` on ANSI.
pub fn top_left_key(board: Board) -> Code {
    match board {
        Board::Ansi => Code::Backquote,
        Board::Iso | Board::Jis => Code::IntlBackslash,
    }
}

/// The card's default chord on `board`: Option and the key top left.
pub fn open_on(board: Board) -> HotKey {
    HotKey::new(Some(Modifiers::ALT), top_left_key(board))
}

/// The chord registered at launch when none is stored: ⌥§ (⌥` on ANSI).
pub fn open() -> HotKey {
    open_on(board())
}

/// Menu / Settings label for the default screenshot chord ([`screenshot`]) on ISO.
#[cfg_attr(not(test), allow(dead_code))]
pub const SCREENSHOT_LABEL: &str = "⌥⇧§";

/// The screenshot default on `board`: the card's chord with Shift.
pub fn screenshot_on(board: Board) -> HotKey {
    HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), top_left_key(board))
}

/// The chord that starts a screenshot when none is stored: ⌥⇧§ (⌥⇧` on ANSI).
pub fn screenshot() -> HotKey {
    screenshot_on(board())
}

/// The screenshot chord from `storage`, or [`screenshot`] when missing or unsafe. A stored
/// ⌃⇧4 (the default until ⌥⇧§) stays: it was recorded by the user.
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

/// Whether `key` is the key top left on either keyboard (§ or `), which nobody types while
/// writing text, so Option alone is enough there.
fn is_top_left(key: Code) -> bool {
    matches!(key, Code::IntlBackslash | Code::Backquote)
}

/// Whether `chord` is strong enough: not a bare key, not Shift alone, and Option or
/// Option-Shift alone only on the key top left. Those type characters (⌥C is ç, ⌥⇧O is Ø), so
/// on any other key a hotkey would take them away; ⌥§ and ⌥⇧§ are the defaults.
/// Control-Option-C and Control-Command-C are fine.
pub fn is_safe(chord: &HotKey) -> bool {
    let mods =
        chord.mods & (Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT);
    if mods.is_empty() || mods == Modifiers::SHIFT {
        return false;
    }
    let typing = mods == Modifiers::ALT || mods == Modifiers::ALT | Modifiers::SHIFT;
    !typing || is_top_left(chord.key)
}

/// Compact Mac label (`⌥§`, `⌃⌘C`) for menus and Settings.
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
    use mac_ui::keys::Board;

    use super::{
        DEFAULT_LABEL, DEFAULT_STORAGE, SCREENSHOT_LABEL, code_from_key_code, from_storage,
        is_safe, label, may_store, open, open_on, screenshot, screenshot_from_storage,
        screenshot_on, to_storage,
    };

    #[test]
    fn the_defaults_sit_on_the_key_top_left_by_position() {
        // ISO: § (kVK_ISO_Section); JIS has no key there and gets the same code.
        for board in [Board::Iso, Board::Jis] {
            assert_eq!(open_on(board).key, Code::IntlBackslash);
            assert_eq!(screenshot_on(board).key, Code::IntlBackslash);
        }
        assert_eq!(label(&open_on(Board::Iso)), DEFAULT_LABEL);
        assert_eq!(label(&screenshot_on(Board::Iso)), SCREENSHOT_LABEL);
        assert_eq!(to_storage(&open_on(Board::Iso)), DEFAULT_STORAGE);
        // ANSI: ` (kVK_ANSI_Grave), shown as itself.
        assert_eq!(open_on(Board::Ansi).key, Code::Backquote);
        assert_eq!(label(&open_on(Board::Ansi)), "⌥`");
        assert_eq!(label(&screenshot_on(Board::Ansi)), "⌥⇧`");
        for board in [Board::Iso, Board::Ansi, Board::Jis] {
            assert_eq!(open_on(board).mods, Modifiers::ALT);
            assert_eq!(screenshot_on(board).mods, Modifiers::ALT | Modifiers::SHIFT);
        }
        // Recording in Settings reads the same keys back.
        assert_eq!(code_from_key_code(0x0A), Some(Code::IntlBackslash));
        assert_eq!(code_from_key_code(0x32), Some(Code::Backquote));
    }

    #[test]
    fn the_screenshot_chord_round_trips() {
        let chord = screenshot();
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
    fn recorded_chords_keep_their_value_when_the_defaults_move() {
        // The old defaults were never stored by CopyCraft, so a stored one was recorded.
        let card = from_storage(Some("control+meta+KeyC"));
        assert_eq!(card.mods, Modifiers::CONTROL | Modifiers::META);
        assert_eq!(card.key, Code::KeyC);
        assert_eq!(from_storage(Some("control+super+KeyC")).id(), card.id());
        let shot = screenshot_from_storage(Some("shift+control+Digit4"));
        assert_eq!(shot.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(shot.key, Code::Digit4);
        // Nothing stored: the new defaults.
        assert_eq!(from_storage(None).id(), open().id());
        assert_eq!(screenshot_from_storage(None).id(), screenshot().id());
        // The default before ⌃⌘C still follows the default, as it did.
        assert_eq!(
            from_storage(Some("control+alt+super+KeyC")).id(),
            open().id()
        );
        // A recorded ⌥§ / ⌥⇧§ round-trips.
        for chord in [open_on(Board::Iso), screenshot_on(Board::Ansi)] {
            assert_eq!(from_storage(Some(&to_storage(&chord))).id(), chord.id());
        }
    }

    #[test]
    fn the_two_hotkeys_may_not_be_the_same_chord() {
        for board in [Board::Iso, Board::Ansi, Board::Jis] {
            assert_ne!(open_on(board).id(), screenshot_on(board).id());
            assert!(may_store(&screenshot_on(board), &open_on(board)));
            assert!(may_store(&open_on(board), &screenshot_on(board)));
            assert!(!may_store(&open_on(board), &open_on(board)));
            assert!(!may_store(&screenshot_on(board), &screenshot_on(board)));
        }
        assert!(may_store(&screenshot(), &open()));
        assert!(!may_store(
            &HotKey::new(Some(Modifiers::SHIFT), Code::Digit4),
            &open()
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
        assert!(is_safe(&HotKey::new(Some(Modifiers::ALT), Code::Backquote)));
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
        for key in [Code::KeyO, Code::KeyC, Code::Digit2, Code::Space] {
            assert!(!is_safe(&HotKey::new(option_shift, key)), "{key:?}");
        }
        assert!(is_safe(&HotKey::new(option_shift, Code::IntlBackslash)));
        assert!(is_safe(&HotKey::new(option_shift, Code::Backquote)));
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
        assert_eq!(from_storage(Some("shift+alt+KeyO")).id(), open().id());
        assert_eq!(
            screenshot_from_storage(Some("shift+alt+KeyO")).id(),
            screenshot().id()
        );
    }

    #[test]
    fn storage_roundtrips_and_falls_back_when_unsafe() {
        let chord = from_storage(Some("control+alt+KeyC"));
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(from_storage(Some("alt+KeyC")).id(), open().id());
        assert_eq!(from_storage(None).id(), open().id());
        assert_eq!(
            from_storage(Some(DEFAULT_STORAGE)).id(),
            open_on(Board::Iso).id()
        );
    }

    #[test]
    fn hotkey_leaves_plain_copy_and_find_to_the_card() {
        let chord = open();
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyC));
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyF));
    }
}
