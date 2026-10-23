use mac_ui::tray_icon::Icon;

/// 18 pt at @2x: tray-icon draws the status item image 18 pt high, so one image pixel is half a
/// point on a Retina screen (and macOS averages two by two on an @1x one).
const SIZE: u32 = 36;
/// Samples per pixel side for the anti-aliased edges (8 × 8 = 64 per pixel).
const SAMPLES: u32 = 8;
/// Board outline, outer edge: 12 × 14 pt with 2.5 pt corners. The stroke is 1.5 pt (3 px), so
/// the inner edge has 1 pt corners. Edges sit on whole pixels, so the straight parts stay sharp.
const BOARD: RoundRect = RoundRect::new(6.0, 6.0, 30.0, 34.0, 5.0);
const BOARD_STROKE: f32 = 3.0;
/// The clip, filled, over the top edge of the board.
const CLIP: RoundRect = RoundRect::new(13.0, 3.0, 23.0, 10.0, 2.0);
/// Clear space around the clip, so it stays a separate shape and does not melt into the top edge.
const CLIP_GAP: f32 = 1.5;
/// The left brace `{` as a stroked line, top to bottom, with its point in the middle (y 22).
/// The right brace is its mirror image.
const BRACE: [(f32, f32); 7] = [
    (15.5, 15.0),
    (13.5, 16.5),
    (13.5, 20.5),
    (11.0, 22.0),
    (13.5, 23.5),
    (13.5, 27.5),
    (15.5, 29.0),
];
/// 2.5 px, round caps and joins.
const BRACE_STROKE: f32 = 2.5;

/// The menu bar icon: the clipboard outline with braces, as a template image (black with an
/// anti-aliased alpha only), which macOS draws in the menu bar's own colour, light, dark or
/// tinted. It is the same in every state. The detected kind is in the tooltip and the VoiceOver
/// label instead. Show it with `template: true`.
pub fn menu_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(icon_pixels(), SIZE, SIZE)?)
}

/// The flash frame of the blink after a copy: the same clipboard with the board filled solid
/// and the braces knocked out of it. Also a template (`template: true`).
pub fn flash_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    Ok(Icon::from_rgba(flash_pixels(), SIZE, SIZE)?)
}

fn icon_pixels() -> Vec<u8> {
    rasterize(|x, y| ink(x, y, false))
}

fn flash_pixels() -> Vec<u8> {
    rasterize(|x, y| ink(x, y, true))
}

/// Whether the point (in pixels, y down) is ink. The outline (or, for the flash, the filled
/// board) loses a ring around the clip; the clip is ink; the braces are ink on the outline and
/// cut out of the filled board.
fn ink(x: f32, y: f32, flash: bool) -> bool {
    let outer = BOARD.distance(x, y);
    let board = outer <= 0.0 && (flash || outer >= -BOARD_STROKE);
    let clip = CLIP.distance(x, y);
    let shape = (board && clip > CLIP_GAP) || clip <= 0.0;
    let brace = brace_distance(x, y).min(brace_distance(SIZE as f32 - x, y)) <= BRACE_STROKE / 2.0;
    if flash {
        shape && !brace
    } else {
        shape || brace
    }
}

/// Black RGBA with the alpha the share of covered samples: an anti-aliased template mask.
fn rasterize(ink: impl Fn(f32, f32) -> bool) -> Vec<u8> {
    let step = 1.0 / SAMPLES as f32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let mut hits = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f32 + (sx as f32 + 0.5) * step;
                    let y = py as f32 + (sy as f32 + 0.5) * step;
                    hits += u32::from(ink(x, y));
                }
            }
            let alpha = (hits * 255 + SAMPLES * SAMPLES / 2) / (SAMPLES * SAMPLES);
            rgba.extend_from_slice(&[0, 0, 0, u8::try_from(alpha).unwrap_or(u8::MAX)]);
        }
    }
    rgba
}

/// Distance from the point to the left brace's centre line.
fn brace_distance(x: f32, y: f32) -> f32 {
    BRACE
        .windows(2)
        .map(|pair| segment_distance(x, y, pair[0], pair[1]))
        .fold(f32::INFINITY, f32::min)
}

fn segment_distance(x: f32, y: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - ax - t * dx).hypot(y - ay - t * dy)
}

/// A rounded rectangle from (x0, y0) to (x1, y1) with corner radius `r`.
#[derive(Clone, Copy)]
struct RoundRect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: f32,
}

impl RoundRect {
    const fn new(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Self {
        Self { x0, y0, x1, y1, r }
    }

    /// Signed distance: negative inside, positive outside.
    fn distance(self, x: f32, y: f32) -> f32 {
        let qx = (x - (self.x0 + self.x1) / 2.0).abs() - ((self.x1 - self.x0) / 2.0 - self.r);
        let qy = (y - (self.y0 + self.y1) / 2.0).abs() - ((self.y1 - self.y0) / 2.0 - self.r);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - self.r
    }
}

#[cfg(test)]
mod tests {
    use super::{SIZE, flash_icon, flash_pixels, icon_pixels, menu_icon};

    const S: usize = SIZE as usize;

    fn alpha(pixels: &[u8], x: usize, y: usize) -> u8 {
        pixels[(y * S + x) * 4 + 3]
    }

    /// A template: black, with only the alpha drawing the glyph.
    fn black_only(pixels: &[u8]) -> bool {
        pixels.chunks(4).all(|px| px[..3] == [0, 0, 0])
    }

    /// The glyph at @1x: macOS averages two by two pixels.
    fn at_1x(pixels: &[u8], x: usize, y: usize) -> u32 {
        let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
            .iter()
            .map(|(dx, dy)| u32::from(alpha(pixels, x * 2 + dx, y * 2 + dy)))
            .sum();
        sum / 4
    }

    #[test]
    fn builds_the_template_icon() {
        assert!(menu_icon().is_ok());
        assert!(flash_icon().is_ok());
    }

    #[test]
    fn icon_is_an_anti_aliased_black_template() {
        let pixels = icon_pixels();
        assert_eq!(S, 36, "18 pt at @2x");
        assert_eq!(pixels.len(), S * S * 4);
        assert!(black_only(&pixels));
        assert!(
            pixels.chunks(4).any(|px| px[3] > 0 && px[3] < 255),
            "edges are anti-aliased"
        );
        let a = |x, y| alpha(&pixels, x, y);
        assert_eq!(
            (a(6, 20), a(7, 20), a(8, 20)),
            (255, 255, 255),
            "3 px outline"
        );
        assert_eq!(a(9, 20), 0, "inside the outline is clear");
        assert_eq!(a(10, 12), 0, "paper is clear");
        assert_eq!(a(18, 6), 255, "clip");
        assert_eq!(a(12, 7), 0, "gap between the clip and the top edge");
        assert_eq!(a(8, 8), 255, "top corner of the board");
        assert_eq!(a(13, 18), 255, "left brace");
        assert!(
            a(11, 22) > 0 && a(12, 22) == 255,
            "left brace's middle point"
        );
        assert_eq!(a(18, 22), 0, "between the braces");
        assert_eq!(a(0, 0), 0, "corner");
        for y in 0..S {
            for x in 0..S {
                assert_eq!(a(x, y), a(S - 1 - x, y), "mirror image at {x},{y}");
            }
        }
    }

    #[test]
    fn icon_reads_at_1x() {
        let pixels = icon_pixels();
        assert!(at_1x(&pixels, 3, 10) >= 192, "outline");
        assert_eq!(at_1x(&pixels, 5, 6), 0, "paper");
        assert_eq!(at_1x(&pixels, 9, 3), 255, "clip");
        assert!(at_1x(&pixels, 6, 9) >= 128, "left brace");
        assert!(at_1x(&pixels, 11, 9) >= 128, "right brace");
        assert_eq!(
            (at_1x(&pixels, 8, 11), at_1x(&pixels, 9, 11)),
            (0, 0),
            "space between"
        );
    }

    #[test]
    fn icon_does_not_change() {
        // One glyph for every clipboard state: nothing to recolour per kind.
        assert_eq!(icon_pixels(), icon_pixels());
    }

    #[test]
    fn flash_glyph_is_a_filled_template_of_the_same_size() {
        let flash = flash_pixels();
        let normal = icon_pixels();
        assert_eq!(flash.len(), normal.len());
        assert!(black_only(&flash));
        assert_ne!(flash, normal);
        let a = |x, y| alpha(&flash, x, y);
        assert_eq!(a(10, 12), 255, "board is filled");
        assert_eq!(a(18, 6), 255, "clip");
        assert_eq!(a(12, 7), 0, "gap around the clip");
        assert_eq!(a(13, 18), 0, "left brace is knocked out");
        assert_eq!(a(22, 18), 0, "right brace is knocked out");
        assert_eq!(a(0, 0), 0, "corner");
        // The same silhouette: outside the paper, where the braces are, the flash frame
        // covers everything the outline does.
        for (index, px) in normal.chunks(4).enumerate() {
            let (x, y) = (index % S, index / S);
            if (9..27).contains(&x) && (10..31).contains(&y) {
                continue;
            }
            assert!(flash[index * 4 + 3] >= px[3], "{x},{y}");
        }
    }

    /// `assets/icon.svg` is the flat copy of the Icon Composer document: its background is the
    /// `icon.json` fill and its shapes are the three layers, verbatim. The layers stay square
    /// and unmasked, with no baked-in corner radius or shadow (Icon Composer adds those).
    #[test]
    fn app_icon_layers_and_flat_svg_stay_in_sync() {
        const FLAT: &str = include_str!("../assets/icon.svg");
        const JSON: &str = include_str!("../assets/AppIcon.icon/icon.json");
        let layers = [
            (
                "board",
                include_str!("../assets/AppIcon.icon/Assets/board.svg"),
            ),
            (
                "clip",
                include_str!("../assets/AppIcon.icon/Assets/clip.svg"),
            ),
            (
                "braces",
                include_str!("../assets/AppIcon.icon/Assets/braces.svg"),
            ),
        ];
        let mut last = 0;
        for (name, layer) in layers {
            assert!(layer.contains(r#"width="1024" height="1024" viewBox="0 0 1024 1024""#));
            assert!(
                !layer.contains("filter") && !layer.contains("rx=\"224\""),
                "{name}"
            );
            let body = layer.split_once('\n').map_or("", |(_, rest)| rest);
            let body = &body[..body.rfind("</svg>").unwrap_or(0)];
            assert!(
                !body.is_empty() && FLAT.contains(body),
                "{name} is in icon.svg"
            );
            // Bottom of the stack first, in icon.json as in icon.svg.
            let at = JSON
                .find(&format!(r#""image-name" : "{name}.svg""#))
                .expect("layer in icon.json");
            assert!(at > last, "{name} is stacked in order");
            last = at;
        }
        assert!(JSON.contains(r#""solid" : "extended-srgb:0.02745,0.06667,0.17647,1.00000""#));
        assert!(FLAT.contains(r##"<rect width="1024" height="1024" fill="#07112D"/>"##));
        assert!(!FLAT.contains("filter"));
    }
}
