//! The System Settings panes CopyCraft itself opens from its own buttons (never from a copied
//! link). Fixed internal URLs, kept apart from [`crate::url_policy`]: a link the user clicks
//! or visits still may not open `x-apple.systempreferences:`.

/// A pane in System Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// Notifications (CopyCraft is in its list once it asked).
    Notifications,
}

impl Pane {
    #[cfg(test)]
    pub const ALL: [Self; 1] = [Self::Notifications];

    /// The pane's fixed URL.
    pub fn url(self) -> &'static str {
        match self {
            Self::Notifications => {
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
            }
        }
    }
}

/// Open `pane` in System Settings via `NSWorkspace`. False when the system could not open it.
#[cfg(target_os = "macos")]
pub fn open(pane: Pane) -> bool {
    use mac_ui::objc2_app_kit::NSWorkspace;
    use mac_ui::objc2_foundation::{NSString, NSURL};

    let Some(url) = NSURL::URLWithString(&NSString::from_str(pane.url())) else {
        return false;
    };
    NSWorkspace::sharedWorkspace().openURL(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_open_system_settings_and_links_still_may_not() {
        for pane in Pane::ALL {
            assert!(
                pane.url()
                    .starts_with("x-apple.systempreferences:com.apple."),
                "{pane:?}"
            );
            // Only these buttons open it: a visited or clicked link is still refused.
            assert!(!crate::url_policy::may_visit(pane.url()), "{pane:?}");
        }
    }
}
