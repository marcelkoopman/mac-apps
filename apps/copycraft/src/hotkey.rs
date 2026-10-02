use global_hotkey::hotkey::{Code, HotKey, Modifiers};

/// Menu label for the chord registered in [`open`].
pub const LABEL: &str = "⌃⌥⌘C";

pub fn open() -> HotKey {
    HotKey::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER),
        Code::KeyC,
    )
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    use super::{LABEL, open};

    #[test]
    fn label_names_the_registered_chord() {
        assert_eq!(LABEL, "⌃⌥⌘C");
        let chord = open();
        assert_eq!(chord.key, Code::KeyC);
        assert_eq!(
            chord.mods,
            Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER
        );
    }

    #[test]
    fn hotkey_leaves_plain_copy_and_find_to_the_card() {
        // Global hotkeys match their exact modifiers, so ⌘C (copy) and ⌘F (find in item) still
        // reach the card.
        let chord = open();
        assert_ne!(chord, HotKey::new(Some(Modifiers::SUPER), Code::KeyC));
        assert_ne!(chord, HotKey::new(Some(Modifiers::SUPER), Code::KeyF));
        assert_ne!(
            chord.id(),
            HotKey::new(Some(Modifiers::SUPER), Code::KeyC).id()
        );
    }
}
