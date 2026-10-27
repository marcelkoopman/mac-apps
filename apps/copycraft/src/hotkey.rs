//! Global hotkeys: the one that summons the card (default ⌥§) and the one that starts a
//! screenshot (default ⌥⇧§, Option-Shift-§, written in the Mac order ⌃⌥⇧⌘). Both are stored
//! in the user defaults; a default is never stored, so a stored chord is one the user
//! recorded in Settings.
//!
//! Both defaults sit on the key top left, next to 1, by its position, not its character: § on
//! ISO keyboards (`kVK_ISO_Section`, [`Code::IntlBackslash`]), ` on ANSI keyboards
//! (`kVK_ANSI_Grave`, [`Code::Backquote`]). On ANSI ⌥` is a dead key (à, è), so the default
//! there has Control too: ⌃⌥` and ⌃⌥⇧`. A JIS keyboard has no key there: no default, the
//! user records one. [`board`] asks macOS which keyboard it is, again on wake, when Settings
//! opens and when the input source changes ([`detect_board`]).
//!
//! Some ISO layouts type a character with ⌥ on that key too (German: ⌥^ is „); the README
//! and Settings say so, and another chord can be recorded.

use std::sync::atomic::{AtomicU8, Ordering};

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

/// The keyboard last detected ([`board_code`]); 0 before the first look.
static BOARD: AtomicU8 = AtomicU8::new(0);

const fn board_code(board: Board) -> u8 {
    match board {
        Board::Iso => 1,
        Board::Ansi => 2,
        Board::Jis => 3,
    }
}

const fn board_of(code: u8) -> Option<Board> {
    match code {
        1 => Some(Board::Iso),
        2 => Some(Board::Ansi),
        3 => Some(Board::Jis),
        _ => None,
    }
}

/// The keyboard's layout as macOS reports it now; ISO when it does not know (and off macOS).
fn detected() -> Board {
    #[cfg(target_os = "macos")]
    {
        mac_ui::keys::board().unwrap_or(Board::Iso)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Board::Iso
    }
}

/// The keyboard's layout, for the default chords: the one last detected (the first call
/// looks).
pub fn board() -> Board {
    board_of(BOARD.load(Ordering::Relaxed)).unwrap_or_else(|| {
        let board = detected();
        BOARD.store(board_code(board), Ordering::Relaxed);
        board
    })
}

/// Ask macOS again which keyboard it is (wake, Settings opening, another input source).
/// Whether it changed, so the default chords need registering again.
pub fn detect_board() -> bool {
    let next = board_code(detected());
    BOARD.swap(next, Ordering::Relaxed) != next
}

/// The key top left, next to 1, on `board`: § on ISO, ` on ANSI, none on JIS.
pub fn top_left_key(board: Board) -> Option<Code> {
    match board {
        Board::Ansi => Some(Code::Backquote),
        Board::Iso => Some(Code::IntlBackslash),
        Board::Jis => None,
    }
}

/// The card's default chord on `board`: Option and the key top left (⌥§), with Control on
/// ANSI (⌃⌥`, as ⌥` is a dead key there); none on JIS.
pub fn open_on(board: Board) -> Option<HotKey> {
    let key = top_left_key(board)?;
    let mods = match board {
        Board::Ansi => Modifiers::CONTROL | Modifiers::ALT,
        Board::Iso | Board::Jis => Modifiers::ALT,
    };
    Some(HotKey::new(Some(mods), key))
}

/// The chord registered when none is stored: ⌥§ (⌃⌥` on ANSI, none on JIS).
pub fn open() -> Option<HotKey> {
    open_on(board())
}

/// Menu / Settings label for the default screenshot chord ([`screenshot`]) on ISO.
#[cfg_attr(not(test), allow(dead_code))]
pub const SCREENSHOT_LABEL: &str = "⌥⇧§";

/// The screenshot default on `board`: the card's chord with Shift.
pub fn screenshot_on(board: Board) -> Option<HotKey> {
    open_on(board).map(|chord| HotKey::new(Some(chord.mods | Modifiers::SHIFT), chord.key))
}

/// The chord that starts a screenshot when none is stored: ⌥⇧§ (⌃⌥⇧` on ANSI, none on JIS).
pub fn screenshot() -> Option<HotKey> {
    screenshot_on(board())
}

/// The screenshot chord from `storage`, or [`screenshot`] when missing or unsafe. A stored
/// ⌃⇧4 (the default until ⌥⇧§) stays: it was recorded by the user.
pub fn screenshot_from_storage(storage: Option<&str>) -> Option<HotKey> {
    storage
        .and_then(|text| text.parse::<HotKey>().ok())
        .filter(is_safe)
        .or_else(screenshot)
}

/// Whether `chord` may be stored next to `other` (the other global hotkey, if any): safe, and
/// not the same chord, which would register only once.
pub fn may_store(chord: &HotKey, other: Option<&HotKey>) -> bool {
    is_safe(chord) && other.is_none_or(|other| chord.id() != other.id())
}

/// The chord from `storage` (a [`HotKey`] string), or the default when missing, unsafe, or still
/// the previous default.
pub fn from_storage(storage: Option<&str>) -> Option<HotKey> {
    storage
        .filter(|text| *text != PREVIOUS_DEFAULT_STORAGE)
        .and_then(|text| text.parse::<HotKey>().ok())
        .filter(is_safe)
        .or_else(open)
}

/// Whether `chord` is strong enough: not a bare key, not Shift alone, and Option or
/// Option-Shift alone only on § (ISO, top left). Those type characters (⌥C is ç, ⌥⇧O is Ø,
/// ⌥` a dead key on ANSI), so on any other key a hotkey would take them away; ⌥§ and ⌥⇧§ are
/// the ISO defaults. Control-Option-C and Control-Command-C are fine.
pub fn is_safe(chord: &HotKey) -> bool {
    let mods =
        chord.mods & (Modifiers::CONTROL | Modifiers::ALT | Modifiers::META | Modifiers::SHIFT);
    if mods.is_empty() || mods == Modifiers::SHIFT {
        return false;
    }
    let typing = mods == Modifiers::ALT || mods == Modifiers::ALT | Modifiers::SHIFT;
    !typing || chord.key == Code::IntlBackslash
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
        DEFAULT_LABEL, DEFAULT_STORAGE, SCREENSHOT_LABEL, board, board_code, board_of,
        code_from_key_code, from_storage, is_safe, label, may_store, open, open_on, screenshot,
        screenshot_from_storage, screenshot_on, to_storage,
    };

    fn iso() -> HotKey {
        open_on(Board::Iso).expect("ISO default")
    }

    fn iso_shot() -> HotKey {
        screenshot_on(Board::Iso).expect("ISO screenshot default")
    }

    #[test]
    fn the_defaults_sit_on_the_key_top_left_by_position() {
        // ISO: § (kVK_ISO_Section), Option alone.
        assert_eq!(iso().key, Code::IntlBackslash);
        assert_eq!(iso_shot().key, Code::IntlBackslash);
        assert_eq!(iso().mods, Modifiers::ALT);
        assert_eq!(iso_shot().mods, Modifiers::ALT | Modifiers::SHIFT);
        assert_eq!(label(&iso()), DEFAULT_LABEL);
        assert_eq!(label(&iso_shot()), SCREENSHOT_LABEL);
        assert_eq!(to_storage(&iso()), DEFAULT_STORAGE);
        // ANSI: ` (kVK_ANSI_Grave) with Control too, as ⌥` is a dead key (à, è).
        let ansi = open_on(Board::Ansi).expect("ANSI default");
        let ansi_shot = screenshot_on(Board::Ansi).expect("ANSI screenshot default");
        assert_eq!(ansi.key, Code::Backquote);
        assert_eq!(label(&ansi), "⌃⌥`");
        assert_eq!(label(&ansi_shot), "⌃⌥⇧`");
        assert!(is_safe(&ansi) && is_safe(&ansi_shot));
        // JIS has no key there: no default at all.
        assert_eq!(open_on(Board::Jis), None);
        assert_eq!(screenshot_on(Board::Jis), None);
        // Recording in Settings reads the same keys back.
        assert_eq!(code_from_key_code(0x0A), Some(Code::IntlBackslash));
        assert_eq!(code_from_key_code(0x32), Some(Code::Backquote));
    }

    #[test]
    fn the_board_is_kept_until_looked_at_again() {
        for b in [Board::Iso, Board::Ansi, Board::Jis] {
            assert_eq!(board_of(board_code(b)), Some(b));
        }
        assert_eq!(board_of(0), None);
        // Off macOS the look always says ISO.
        if cfg!(not(target_os = "macos")) {
            assert_eq!(board(), Board::Iso);
            assert!(!super::detect_board());
            assert_eq!(open(), Some(iso()));
        }
    }

    #[test]
    fn the_screenshot_chord_round_trips() {
        let chord = screenshot().expect("default");
        assert!(is_safe(&chord));
        assert_ne!(Some(chord.id()), open().map(|c| c.id()));
        let stored = to_storage(&chord);
        assert_eq!(screenshot_from_storage(Some(&stored)), Some(chord));
        // Another safe chord is kept; nothing, rubbish or an unsafe chord gives the default.
        let other = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Digit5);
        assert_eq!(
            screenshot_from_storage(Some(&to_storage(&other))),
            Some(other)
        );
        assert_eq!(screenshot_from_storage(None), Some(chord));
        assert_eq!(screenshot_from_storage(Some("nonsense")), Some(chord));
        assert_eq!(screenshot_from_storage(Some("shift+Digit4")), Some(chord));
        // The previous default of the card's chord is a valid screenshot chord here.
        let previous = screenshot_from_storage(Some("control+alt+super+KeyC"));
        assert_eq!(previous.map(|c| c.key), Some(Code::KeyC));
    }

    #[test]
    fn recorded_chords_keep_their_value_when_the_defaults_move() {
        // The old defaults were never stored by CopyCraft, so a stored one was recorded.
        let card = from_storage(Some("control+meta+KeyC")).expect("stored");
        assert_eq!(card.mods, Modifiers::CONTROL | Modifiers::META);
        assert_eq!(card.key, Code::KeyC);
        assert_eq!(from_storage(Some("control+super+KeyC")), Some(card));
        let shot = screenshot_from_storage(Some("shift+control+Digit4")).expect("stored");
        assert_eq!(shot.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(shot.key, Code::Digit4);
        // Nothing stored: the new defaults.
        assert_eq!(from_storage(None), open());
        assert_eq!(screenshot_from_storage(None), screenshot());
        // The default before ⌃⌘C still follows the default, as it did.
        assert_eq!(from_storage(Some("control+alt+super+KeyC")), open());
        // Recorded defaults of either board round-trip.
        for chord in [iso(), screenshot_on(Board::Ansi).expect("ANSI")] {
            assert_eq!(from_storage(Some(&to_storage(&chord))), Some(chord));
        }
    }

    #[test]
    fn the_two_hotkeys_may_not_be_the_same_chord() {
        for board in [Board::Iso, Board::Ansi] {
            let (card, shot) = (
                open_on(board).expect("card"),
                screenshot_on(board).expect("shot"),
            );
            assert_ne!(card.id(), shot.id());
            assert!(may_store(&shot, Some(&card)));
            assert!(may_store(&card, Some(&shot)));
            assert!(!may_store(&card, Some(&card)));
            assert!(!may_store(&shot, Some(&shot)));
        }
        // JIS: the other has none, any safe chord goes.
        assert!(may_store(&iso(), None));
        assert!(!may_store(
            &HotKey::new(Some(Modifiers::SHIFT), Code::Digit4),
            None
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
        assert!(is_safe(&iso()));
        assert!(is_safe(&iso_shot()));
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
        let chord = from_storage(Some("control+alt+KeyC")).expect("stored");
        assert_eq!(chord.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(from_storage(Some("alt+KeyC")), open());
        assert_eq!(from_storage(None), open());
        assert_eq!(from_storage(Some(DEFAULT_STORAGE)), Some(iso()));
    }

    #[test]
    fn hotkey_leaves_plain_copy_and_find_to_the_card() {
        let chord = iso();
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyC));
        assert_ne!(chord, HotKey::new(Some(Modifiers::META), Code::KeyF));
    }
}
