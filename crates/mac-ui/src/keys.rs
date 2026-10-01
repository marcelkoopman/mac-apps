//! Named hardware key codes (`NSEvent.keyCode`) for the keys apps handle themselves.
//!
//! The values are the virtual key codes from Carbon's `HIToolbox/Events.h` (`kVK_*`), which
//! objc2-app-kit does not bind. A key code names a physical key, the same on every keyboard
//! layout and whatever the modifiers, so matching on it behaves exactly like matching the raw
//! numbers. Pure data, available on every platform.

/// Virtual key codes, as returned by `NSEvent::keyCode`.
pub mod code {
    /// `kVK_Return`, the main Return key.
    pub const RETURN: u16 = 0x24;
    /// `kVK_Tab`.
    pub const TAB: u16 = 0x30;
    /// `kVK_Delete`, the Backspace key labelled "delete".
    pub const DELETE: u16 = 0x33;
    /// `kVK_Escape`.
    pub const ESCAPE: u16 = 0x35;
    /// `kVK_ANSI_KeypadEnter`, Enter on the numeric keypad (and fn-Return on laptops).
    pub const KEYPAD_ENTER: u16 = 0x4C;
    /// `kVK_LeftArrow`.
    pub const LEFT_ARROW: u16 = 0x7B;
    /// `kVK_RightArrow`.
    pub const RIGHT_ARROW: u16 = 0x7C;
    /// `kVK_DownArrow`.
    pub const DOWN_ARROW: u16 = 0x7D;
    /// `kVK_UpArrow`.
    pub const UP_ARROW: u16 = 0x7E;
}

/// A key handled by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Up,
    Down,
    /// Return or keypad Enter.
    Return,
    Escape,
    Tab,
    /// Backspace ("delete").
    Delete,
}

impl Key {
    /// The named key for hardware key code `key_code`, `None` for any other key.
    pub const fn from_key_code(key_code: u16) -> Option<Self> {
        match key_code {
            code::LEFT_ARROW => Some(Self::Left),
            code::RIGHT_ARROW => Some(Self::Right),
            code::UP_ARROW => Some(Self::Up),
            code::DOWN_ARROW => Some(Self::Down),
            code::RETURN | code::KEYPAD_ENTER => Some(Self::Return),
            code::ESCAPE => Some(Self::Escape),
            code::TAB => Some(Self::Tab),
            code::DELETE => Some(Self::Delete),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_carbon_values() {
        assert_eq!(code::RETURN, 36);
        assert_eq!(code::TAB, 48);
        assert_eq!(code::DELETE, 51);
        assert_eq!(code::ESCAPE, 53);
        assert_eq!(code::KEYPAD_ENTER, 76);
        assert_eq!(code::LEFT_ARROW, 123);
        assert_eq!(code::RIGHT_ARROW, 124);
        assert_eq!(code::DOWN_ARROW, 125);
        assert_eq!(code::UP_ARROW, 126);
    }

    #[test]
    fn named_keys() {
        assert_eq!(Key::from_key_code(123), Some(Key::Left));
        assert_eq!(Key::from_key_code(124), Some(Key::Right));
        assert_eq!(Key::from_key_code(126), Some(Key::Up));
        assert_eq!(Key::from_key_code(125), Some(Key::Down));
        assert_eq!(Key::from_key_code(36), Some(Key::Return));
        assert_eq!(Key::from_key_code(76), Some(Key::Return));
        assert_eq!(Key::from_key_code(53), Some(Key::Escape));
        assert_eq!(Key::from_key_code(48), Some(Key::Tab));
        assert_eq!(Key::from_key_code(51), Some(Key::Delete));
    }

    #[test]
    fn other_keys_are_not_named() {
        // kVK_ANSI_A, kVK_ANSI_Slash, kVK_Space, kVK_ForwardDelete.
        for key_code in [0x00, 0x2C, 0x31, 0x75, u16::MAX] {
            assert_eq!(Key::from_key_code(key_code), None);
        }
    }
}
