//! Generic rows for a menu bar (tray) menu, template icons, the icon set ([`Glyphs`]) and the
//! timing of a short icon blink ([`Blink`]).

use std::time::{Duration, Instant};

use tray_icon::menu::{IconMenuItem, MenuItem, NativeIcon};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Menu id of [`quit_item`]. Match it in the app's `MenuEvent` loop.
pub const QUIT_ID: &str = "quit";

/// Disabled, informational row such as a hotkey hint or the version.
///
/// On macOS the title is drawn in `secondaryLabelColor`.
pub fn info_item(label: &str) -> MenuItem {
    let item = MenuItem::new(label, false, None);
    #[cfg(target_os = "macos")]
    {
        use objc2::AnyThread;
        use objc2_app_kit::{NSColor, NSForegroundColorAttributeName};
        use objc2_foundation::{NSAttributedString, NSMutableAttributedString, NSRange, NSString};

        let ns = NSString::from_str(label);
        let attr =
            NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns);
        let all = NSRange::new(0, ns.length());
        // SAFETY: NSForegroundColorAttributeName takes an NSColor value.
        unsafe {
            attr.addAttribute_value_range(
                NSForegroundColorAttributeName,
                &NSColor::secondaryLabelColor(),
                all,
            );
        }
        let title: &NSAttributedString = &attr;
        item.set_attributed_title(Some(title));
    }
    item
}

/// Version row text: `"{app} {version}"`.
///
/// Pass the app's own version, e.g. `env!("CARGO_PKG_VERSION")` expanded in the app crate.
pub fn version_label(app: &str, version: &str) -> String {
    format!("{app} {version}")
}

/// Version row: an [`info_item`] showing [`version_label`].
pub fn version_item(app: &str, version: &str) -> MenuItem {
    info_item(&version_label(app, version))
}

/// Builder with `icon`, drawn as a template image on macOS when `template` is set (see
/// [`set_icon`]). Elsewhere the icon is used as-is.
pub fn with_icon(builder: TrayIconBuilder, icon: Icon, template: bool) -> TrayIconBuilder {
    #[cfg(target_os = "macos")]
    if template {
        return builder.with_icon_templated(icon);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    builder.with_icon(icon)
}

/// Show `icon` in the menu bar. With `template` set, macOS draws it from its alpha channel only
/// (`NSImage.isTemplate`) in the menu bar's colour, so it follows light, dark and tinted menu
/// bars. Without it the colours are kept, for a state that must stand out (an alert). Elsewhere
/// the icon is used as-is.
///
/// # Errors
///
/// When `tray_icon` cannot set the icon.
pub fn set_icon(tray: &TrayIcon, icon: Icon, template: bool) -> tray_icon::Result<()> {
    #[cfg(target_os = "macos")]
    if template {
        return tray.set_icon_templated(Some(icon));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = template;
    tray.set_icon(Some(icon))
}

/// VoiceOver label of the menu bar button of `tray`, for an icon without a title or a title
/// that does not say what the app is. The tooltip stays the help text.
///
/// macOS only, on the main thread; elsewhere it does nothing.
pub fn set_accessibility_label(tray: &TrayIcon, label: &str) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSAccessibility;
        use objc2_foundation::NSString;

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(button) = tray.ns_status_item().and_then(|item| item.button(mtm)) else {
            return;
        };
        button.setAccessibilityLabel(Some(&NSString::from_str(label)));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (tray, label);
}

/// Enabled quit row with id [`QUIT_ID`] and the native stop icon.
pub fn quit_item(label: &str) -> IconMenuItem {
    IconMenuItem::with_id_and_native_icon(
        QUIT_ID,
        label,
        true,
        Some(NativeIcon::StopProgress),
        None,
    )
}

/// The icons a menu bar item switches between: `normal` and `flash` are templates (drawn in the
/// menu bar's colour), `alert` keeps its colours. A missing `flash` or `alert` shows `normal`.
#[derive(Clone)]
pub struct Glyphs {
    pub normal: Icon,
    pub flash: Option<Icon>,
    pub alert: Option<Icon>,
}

impl Glyphs {
    /// The icon for `glyph` and whether it is a template.
    pub fn icon(&self, glyph: Glyph) -> (Icon, bool) {
        match glyph {
            Glyph::Flash => match &self.flash {
                Some(icon) => (icon.clone(), true),
                None => (self.normal.clone(), true),
            },
            Glyph::Alert => match &self.alert {
                Some(icon) => (icon.clone(), false),
                None => (self.normal.clone(), true),
            },
            Glyph::Normal => (self.normal.clone(), true),
        }
    }

    /// Put `glyph` in the menu bar (see [`set_icon`]).
    pub fn show(&self, tray: &TrayIcon, glyph: Glyph) -> tray_icon::Result<()> {
        let (icon, template) = self.icon(glyph);
        set_icon(tray, icon, template)
    }
}

/// One phase of the blink: flash, rest, flash, then the normal glyph again (3 phases, 540 ms).
pub const BLINK_PHASE: Duration = Duration::from_millis(180);
const BLINK_PHASES: u32 = 3;

/// Which of the [`Glyphs`] the menu bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Glyph {
    /// The usual icon, a template.
    #[default]
    Normal,
    /// The [`Blink`] frame, a template.
    Flash,
    /// A state that must stand out, in its own colours.
    Alert,
}

/// What the event loop does after [`Blink::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlinkStep {
    /// Glyph to put in the menu bar now; `None` when the shown one is still right.
    pub swap: Option<Glyph>,
    /// When the next swap is due; `None` once the blink is over.
    pub wake: Option<Instant>,
}

/// Timing of a short blink of the menu bar icon after an event (copycraft: each new copy). Pure
/// state: the event loop feeds it the time, swaps the icon when asked and wakes up when told, so
/// nothing sleeps on the main thread.
#[derive(Debug, Default)]
pub struct Blink {
    started: Option<Instant>,
    shown: Glyph,
}

impl Blink {
    /// Start a blink at `now`. A blink that is still running absorbs it, so a burst of events
    /// blinks once.
    pub fn start(&mut self, now: Instant) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    /// Advance to `now`. A late wake-up skips the phases it missed, so a stalled loop never
    /// leaves the flash glyph standing.
    pub fn tick(&mut self, now: Instant) -> BlinkStep {
        let (glyph, wake) = self.due(now);
        if wake.is_none() {
            self.started = None;
        }
        let swap = (glyph != self.shown).then_some(glyph);
        self.shown = glyph;
        BlinkStep { swap, wake }
    }

    fn due(&self, now: Instant) -> (Glyph, Option<Instant>) {
        let Some(started) = self.started else {
            return (Glyph::Normal, None);
        };
        let elapsed = now.saturating_duration_since(started).as_millis();
        let phase = elapsed / BLINK_PHASE.as_millis();
        if phase >= u128::from(BLINK_PHASES) {
            return (Glyph::Normal, None);
        }
        let glyph = if phase.is_multiple_of(2) {
            Glyph::Flash
        } else {
            Glyph::Normal
        };
        // phase < BLINK_PHASES, so it fits in a u32.
        let next = BLINK_PHASE * (phase as u32 + 1);
        (glyph, Some(started + next))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        BLINK_PHASE, BLINK_PHASES, Blink, BlinkStep, Glyph, Glyphs, QUIT_ID, version_label,
    };
    use crate::icon::Canvas;

    fn square(rgba: [u8; 4]) -> tray_icon::Icon {
        Canvas::filled(4, 4, rgba).unwrap().into_icon().unwrap()
    }

    #[test]
    fn glyphs_fall_back_to_the_normal_template() {
        let only_normal = Glyphs {
            normal: square([0, 0, 0, 255]),
            flash: None,
            alert: None,
        };
        assert!(only_normal.icon(Glyph::Normal).1);
        assert!(only_normal.icon(Glyph::Flash).1);
        assert!(
            only_normal.icon(Glyph::Alert).1,
            "no alert icon: the normal template"
        );
        let full = Glyphs {
            flash: Some(square([0, 0, 0, 255])),
            alert: Some(square([255, 80, 80, 255])),
            ..only_normal
        };
        assert!(full.icon(Glyph::Flash).1, "flash is a template");
        assert!(!full.icon(Glyph::Alert).1, "alert keeps its colours");
    }

    #[test]
    fn version_label_joins_app_and_version() {
        assert_eq!(version_label("Copycraft", "1.2.3"), "Copycraft 1.2.3");
    }

    #[test]
    fn quit_id_is_stable() {
        assert_eq!(QUIT_ID, "quit");
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn idle_blink_does_nothing() {
        let mut blink = Blink::default();
        let step = blink.tick(Instant::now());
        assert_eq!(
            step,
            BlinkStep {
                swap: None,
                wake: None
            }
        );
    }

    #[test]
    fn copy_blinks_twice_then_rests() {
        let t0 = Instant::now();
        let mut blink = Blink::default();
        blink.start(t0);
        let p = BLINK_PHASE;
        assert_eq!(
            blink.tick(t0),
            BlinkStep {
                swap: Some(Glyph::Flash),
                wake: Some(t0 + p)
            }
        );
        // A wake-up before the phase ends changes nothing.
        assert_eq!(
            blink.tick(t0 + ms(50)),
            BlinkStep {
                swap: None,
                wake: Some(t0 + p)
            }
        );
        assert_eq!(
            blink.tick(t0 + p),
            BlinkStep {
                swap: Some(Glyph::Normal),
                wake: Some(t0 + p * 2)
            }
        );
        assert_eq!(
            blink.tick(t0 + p * 2),
            BlinkStep {
                swap: Some(Glyph::Flash),
                wake: Some(t0 + p * 3)
            }
        );
        assert_eq!(
            blink.tick(t0 + p * 3),
            BlinkStep {
                swap: Some(Glyph::Normal),
                wake: None
            }
        );
        assert_eq!(
            blink.tick(t0 + p * 4),
            BlinkStep {
                swap: None,
                wake: None
            }
        );
    }

    #[test]
    fn blink_phases_last_150_to_250_ms_and_the_whole_blink_stays_short() {
        assert!(BLINK_PHASE >= ms(150) && BLINK_PHASE <= ms(250));
        assert!(BLINK_PHASE * BLINK_PHASES <= ms(750));
    }

    #[test]
    fn rapid_copies_coalesce_into_one_blink() {
        let t0 = Instant::now();
        let mut blink = Blink::default();
        blink.start(t0);
        assert_eq!(blink.tick(t0).swap, Some(Glyph::Flash));
        // More copies during the blink neither restart nor stretch it.
        blink.start(t0 + ms(100));
        blink.start(t0 + BLINK_PHASE * 2 + ms(10));
        assert_eq!(blink.tick(t0 + BLINK_PHASE * 3).wake, None);
        // The next copy after it ends blinks again.
        let t1 = t0 + BLINK_PHASE * 3 + ms(20);
        blink.start(t1);
        assert_eq!(
            blink.tick(t1),
            BlinkStep {
                swap: Some(Glyph::Flash),
                wake: Some(t1 + BLINK_PHASE)
            }
        );
    }

    #[test]
    fn late_wake_up_never_leaves_the_flash_standing() {
        let t0 = Instant::now();
        let mut blink = Blink::default();
        blink.start(t0);
        assert_eq!(blink.tick(t0).swap, Some(Glyph::Flash));
        assert_eq!(
            blink.tick(t0 + ms(2_000)),
            BlinkStep {
                swap: Some(Glyph::Normal),
                wake: None
            }
        );
    }
}
