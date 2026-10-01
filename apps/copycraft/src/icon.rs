use std::time::{Duration, Instant};

use mac_ui::icon::{Canvas, IconError};
use mac_ui::tray_icon::Icon;

const SIZE: u32 = 32;
const INK: [u8; 4] = [0, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// The menu bar icon: the clipboard glyph as a template image (black and clear only), which
/// macOS draws in the menu bar's own colour, light, dark or tinted. It is the same in every
/// state. The detected kind is in the tooltip and the VoiceOver label instead. Show it with
/// `template: true`.
pub fn menu_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(icon_pixels()?, SIZE, SIZE)?)
}

/// The flash frame of the blink after a copy: the same clipboard filled solid, braces still cut
/// out. Also a template (`template: true`).
pub fn flash_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(flash_pixels()?, SIZE, SIZE)?)
}

/// One phase of the blink: flash, rest, flash, then the normal glyph again (3 phases, 540 ms).
pub const BLINK_PHASE: Duration = Duration::from_millis(180);
const BLINK_PHASES: u32 = 3;

/// Which of the two template glyphs the menu bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Glyph {
    #[default]
    Normal,
    Flash,
}

/// What the event loop does after [`Blink::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlinkStep {
    /// Glyph to put in the menu bar now; `None` when the shown one is still right.
    pub swap: Option<Glyph>,
    /// When the next swap is due; `None` once the blink is over.
    pub wake: Option<Instant>,
}

/// Timing of the short blink on each new copy. Pure state: the event loop feeds it the time,
/// swaps the icon when asked and wakes up when told, so nothing sleeps on the main thread.
#[derive(Debug, Default)]
pub struct Blink {
    started: Option<Instant>,
    shown: Glyph,
}

impl Blink {
    /// A new copy at `now`. A blink that is still running absorbs it, so a burst of copies
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

/// A clipboard with braces on the paper: the board, the clip and the paper are ink; the frame
/// between board and paper and the braces are cut out.
fn icon_pixels() -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, INK);
    canvas.fill_round_rect(5, 8, 22, 20, 4, CLEAR);
    canvas.fill_round_rect(7, 10, 18, 16, 3, INK);
    canvas.fill_round_rect(12, 5, 8, 6, 2, INK);
    draw_brace_left(&mut canvas, CLEAR);
    draw_brace_right(&mut canvas, CLEAR);
    Ok(canvas.into_rgba())
}

/// The board filled solid (the clip sits inside it), braces cut out: same outline as the glyph.
fn flash_pixels() -> Result<Vec<u8>, IconError> {
    let mut canvas = Canvas::new(SIZE, SIZE)?;
    canvas.fill_round_rect(2, 4, 28, 26, 6, INK);
    draw_brace_left(&mut canvas, CLEAR);
    draw_brace_right(&mut canvas, CLEAR);
    Ok(canvas.into_rgba())
}

fn draw_brace_left(canvas: &mut Canvas, c: [u8; 4]) {
    for y in 14..26 {
        canvas.put(11, y, c);
    }
    canvas.put(12, 14, c);
    canvas.put(12, 25, c);
    canvas.put(10, 19, c);
    canvas.put(10, 20, c);
}

fn draw_brace_right(canvas: &mut Canvas, c: [u8; 4]) {
    for y in 14..26 {
        canvas.put(20, y, c);
    }
    canvas.put(19, 14, c);
    canvas.put(19, 25, c);
    canvas.put(21, 19, c);
    canvas.put(21, 20, c);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        BLINK_PHASE, Blink, BlinkStep, Glyph, SIZE, flash_icon, flash_pixels, icon_pixels,
        menu_icon,
    };

    fn mask_only(pixels: &[u8]) -> bool {
        pixels
            .chunks(4)
            .all(|px| px == [0, 0, 0, 255] || px == [0, 0, 0, 0])
    }

    #[test]
    fn builds_the_template_icon() {
        assert!(menu_icon().is_ok());
        assert!(flash_icon().is_ok());
    }

    #[test]
    fn icon_is_black_and_clear_only() {
        let pixels = icon_pixels().unwrap();
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        assert!(mask_only(&pixels));
        let alpha = |x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(alpha(3, 18), 255, "board");
        assert_eq!(alpha(5, 18), 0, "frame between board and paper");
        assert_eq!(alpha(15, 12), 255, "paper");
        assert_eq!(alpha(11, 19), 0, "left brace");
        assert_eq!(alpha(20, 19), 0, "right brace");
        assert_eq!(alpha(16, 6), 255, "clip");
        assert_eq!(alpha(0, 0), 0, "corner");
    }

    #[test]
    fn icon_does_not_change() {
        // One glyph for every clipboard state: nothing to recolour per kind.
        assert_eq!(icon_pixels().unwrap(), icon_pixels().unwrap());
    }

    #[test]
    fn flash_glyph_is_a_filled_template_of_the_same_size() {
        let flash = flash_pixels().unwrap();
        let normal = icon_pixels().unwrap();
        assert_eq!(flash.len(), normal.len());
        assert!(mask_only(&flash));
        assert_ne!(flash, normal);
        let alpha = |x: usize, y: usize| flash[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(alpha(5, 18), 255, "frame is filled");
        assert_eq!(alpha(11, 19), 0, "left brace stays cut out");
        assert_eq!(alpha(20, 19), 0, "right brace stays cut out");
        assert_eq!(alpha(0, 0), 0, "corner");
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
        assert!(BLINK_PHASE * super::BLINK_PHASES <= ms(750));
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
