//! The Colour card: a copy that is exactly one CSS colour (`#RGB`, `#RGBA`, `#RRGGBB`,
//! `#RRGGBBAA`, `rgb()`/`rgba()`, `hsl()`/`hsla()`, `oklch()`) shows a swatch and the same
//! colour as HEX, RGB, HSL, OKLCH, SwiftUI `Color` and `NSColor`, with its WCAG contrast
//! against white and black. String work and arithmetic only, no crate. A colour is never
//! logged.

/// A colour in sRGB, each channel and the alpha 0…1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swatch {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

/// A parsed colour and whether it had to be clipped into sRGB (an `oklch()` outside it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parsed {
    pub swatch: Swatch,
    pub clipped: bool,
}

/// The colour `text` is, or `None`. One value, optionally quoted or ending in `;`.
pub fn parse(text: &str) -> Option<Parsed> {
    let mut value = text.trim();
    if value.len() > 80 {
        return None;
    }
    value = value.strip_suffix(';').unwrap_or(value).trim_end();
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            value = inner.trim();
        }
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex).map(|swatch| Parsed {
            swatch,
            clipped: false,
        });
    }
    let open = value.find('(')?;
    let inner = value[open + 1..].strip_suffix(')')?;
    let name = value[..open].trim().to_ascii_lowercase();
    let (parts, alpha) = arguments(inner)?;
    let alpha = match alpha {
        Some(alpha) => alpha_value(alpha)?,
        None => 1.0,
    };
    let [a, b, c] = parts;
    match name.as_str() {
        "rgb" | "rgba" => {
            let channel = |part: &str| match part.strip_suffix('%') {
                Some(percent) => number(percent)
                    .filter(|p| (0.0..=100.0).contains(p))
                    .map(|p| p / 100.0),
                None => number(part)
                    .filter(|v| (0.0..=255.0).contains(v))
                    .map(|v| v / 255.0),
            };
            Some(Parsed {
                swatch: Swatch {
                    red: channel(a)?,
                    green: channel(b)?,
                    blue: channel(c)?,
                    alpha,
                },
                clipped: false,
            })
        }
        "hsl" | "hsla" => {
            let hue = hue(a)?;
            let fraction = |part: &str| {
                number(part.strip_suffix('%').unwrap_or(part))
                    .filter(|p| (0.0..=100.0).contains(p))
                    .map(|p| p / 100.0)
            };
            let (red, green, blue) = hsl_to_rgb(hue, fraction(b)?, fraction(c)?);
            Some(Parsed {
                swatch: Swatch {
                    red,
                    green,
                    blue,
                    alpha,
                },
                clipped: false,
            })
        }
        "oklch" => {
            let lightness = match a.strip_suffix('%') {
                Some(percent) => number(percent)? / 100.0,
                None => number(a)?,
            };
            let chroma = match b.strip_suffix('%') {
                Some(percent) => number(percent)? / 100.0 * 0.4,
                None => number(b)?,
            };
            if !(0.0..=1.0).contains(&lightness) || !(0.0..=0.5).contains(&chroma) {
                return None;
            }
            let (red, green, blue) = oklch_to_srgb(lightness, chroma, hue(c)?);
            let inside = |v: f64| (-0.0005..=1.0005).contains(&v);
            let clipped = !(inside(red) && inside(green) && inside(blue));
            Some(Parsed {
                swatch: Swatch {
                    red: red.clamp(0.0, 1.0),
                    green: green.clamp(0.0, 1.0),
                    blue: blue.clamp(0.0, 1.0),
                    alpha,
                },
                clipped,
            })
        }
        _ => None,
    }
}

/// `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`. A short form of decimal digits only that are not
/// all the same (`#123`, `#2024`) is an issue or a number, not a colour; `#000` and `#999` are.
fn parse_hex(hex: &str) -> Option<Swatch> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digits: Vec<u8> = hex
        .bytes()
        .map(|b| (b as char).to_digit(16).unwrap_or(0) as u8)
        .collect();
    let channels: Vec<f64> = match digits.len() {
        3 | 4 => {
            let decimal = hex.bytes().all(|b| b.is_ascii_digit());
            if decimal && digits.iter().any(|d| *d != digits[0]) {
                return None;
            }
            digits.iter().map(|d| f64::from(d * 17) / 255.0).collect()
        }
        6 | 8 => digits
            .chunks(2)
            .map(|pair| f64::from(pair[0] * 16 + pair[1]) / 255.0)
            .collect(),
        _ => return None,
    };
    Some(Swatch {
        red: channels[0],
        green: channels[1],
        blue: channels[2],
        alpha: channels.get(3).copied().unwrap_or(1.0),
    })
}

/// Three components and an optional alpha: `a, b, c[, d]` or `a b c[ / d]`.
fn arguments(inner: &str) -> Option<([&str; 3], Option<&str>)> {
    let (main, slash) = match inner.split_once('/') {
        Some((main, alpha)) => (main, Some(alpha.trim())),
        None => (inner, None),
    };
    let parts: Vec<&str> = if main.contains(',') {
        main.split(',').map(str::trim).collect()
    } else {
        main.split_whitespace().collect()
    };
    match (parts.as_slice(), slash) {
        (&[a, b, c], alpha) => Some(([a, b, c], alpha)),
        (&[a, b, c, d], None) if main.contains(',') => Some(([a, b, c], Some(d))),
        _ => None,
    }
    .filter(|(parts, _)| parts.iter().all(|part| !part.is_empty()))
}

fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty()
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+')
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn alpha_value(text: &str) -> Option<f64> {
    match text.strip_suffix('%') {
        Some(percent) => number(percent).map(|p| p / 100.0),
        None => number(text),
    }
    .filter(|a| (0.0..=1.0).contains(a))
}

/// A hue in degrees, 0…360: a number, `deg`, `turn` or `rad`.
fn hue(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let degrees = if let Some(v) = lower.strip_suffix("deg") {
        number(v)?
    } else if let Some(v) = lower.strip_suffix("turn") {
        number(v)? * 360.0
    } else if let Some(v) = lower.strip_suffix("rad") {
        number(v)?.to_degrees()
    } else {
        number(&lower)?
    };
    Some(degrees.rem_euclid(360.0))
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> (f64, f64, f64) {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let section = hue / 60.0;
    let x = chroma * (1.0 - (section.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match section as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    (r + m, g + m, b + m)
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    if delta < 1e-9 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue, saturation, lightness)
}

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// OKLCH (Björn Ottosson's OKLab, polar) to sRGB, not clipped.
fn oklch_to_srgb(lightness: f64, chroma: f64, hue: f64) -> (f64, f64, f64) {
    let (a, b) = (
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
    );
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    let r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    let g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    let bl = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s;
    (from_linear(r), from_linear(g), from_linear(bl))
}

/// sRGB to OKLCH: lightness 0…1, chroma, hue in degrees.
fn srgb_to_oklch(swatch: &Swatch) -> (f64, f64, f64) {
    let (r, g, b) = (
        to_linear(swatch.red),
        to_linear(swatch.green),
        to_linear(swatch.blue),
    );
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    let chroma = (a * a + bb * bb).sqrt();
    let hue = if chroma < 2e-4 {
        0.0
    } else {
        bb.atan2(a).to_degrees().rem_euclid(360.0)
    };
    (lightness, chroma, hue)
}

/// A number with at most `places` decimals, without trailing zeros: `0.2`, `1`, `0.153`.
fn short(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if text == "-0" { "0".to_string() } else { text }
}

fn byte(channel: f64) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Swatch {
    pub fn opaque(&self) -> bool {
        self.alpha >= 0.9995
    }

    pub fn hex(&self) -> String {
        let mut hex = format!(
            "#{:02X}{:02X}{:02X}",
            byte(self.red),
            byte(self.green),
            byte(self.blue)
        );
        if !self.opaque() {
            hex.push_str(&format!("{:02X}", byte(self.alpha)));
        }
        hex
    }

    fn alpha_suffix(&self) -> String {
        if self.opaque() {
            String::new()
        } else {
            format!(" / {}", short(self.alpha, 3))
        }
    }

    pub fn rgb(&self) -> String {
        format!(
            "rgb({} {} {}{})",
            byte(self.red),
            byte(self.green),
            byte(self.blue),
            self.alpha_suffix()
        )
    }

    pub fn hsl(&self) -> String {
        let (hue, saturation, lightness) = rgb_to_hsl(self.red, self.green, self.blue);
        format!(
            "hsl({} {}% {}%{})",
            short(hue, 1),
            short(saturation * 100.0, 1),
            short(lightness * 100.0, 1),
            self.alpha_suffix()
        )
    }

    pub fn oklch(&self) -> String {
        let (lightness, chroma, hue) = srgb_to_oklch(self);
        format!(
            "oklch({}% {} {}{})",
            short(lightness * 100.0, 1),
            short(chroma, 3),
            short(hue, 1),
            self.alpha_suffix()
        )
    }

    pub fn swiftui(&self) -> String {
        let mut text = format!(
            "Color(red: {}, green: {}, blue: {}",
            short(self.red, 3),
            short(self.green, 3),
            short(self.blue, 3)
        );
        if !self.opaque() {
            text.push_str(&format!(", opacity: {}", short(self.alpha, 3)));
        }
        text.push(')');
        text
    }

    pub fn nscolor(&self) -> String {
        format!(
            "NSColor(srgbRed: {}, green: {}, blue: {}, alpha: {})",
            short(self.red, 3),
            short(self.green, 3),
            short(self.blue, 3),
            short(self.alpha, 3)
        )
    }

    /// The WCAG 2 contrast ratio (1…21) of this colour, laid over `background`, with it.
    pub fn contrast(&self, background: [f64; 3]) -> f64 {
        let over = |channel: f64, under: f64| channel * self.alpha + under * (1.0 - self.alpha);
        let front = luminance(
            over(self.red, background[0]),
            over(self.green, background[1]),
            over(self.blue, background[2]),
        );
        let back = luminance(background[0], background[1], background[2]);
        let (light, dark) = if front > back {
            (front, back)
        } else {
            (back, front)
        };
        (light + 0.05) / (dark + 0.05)
    }
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// One line of the card: a label and a value (the value can be copied).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
}

/// `4.5:1 · AA ✓ · AAA ✗` (AA: 4.5, AAA: 7, for normal text).
pub fn contrast_value(ratio: f64) -> String {
    let mark = |pass: bool| if pass { "✓" } else { "✗" };
    format!(
        "{}:1  ·  AA {}  ·  AAA {}",
        short((ratio * 100.0).floor() / 100.0, 2),
        mark(ratio >= 4.5),
        mark(ratio >= 7.0)
    )
}

impl Parsed {
    /// The card's rows, in order.
    pub fn rows(&self) -> Vec<Row> {
        let swatch = &self.swatch;
        let row = |label: &'static str, value: String| Row { label, value };
        let t = crate::locale::t;
        vec![
            row("HEX", swatch.hex()),
            row("RGB", swatch.rgb()),
            row("HSL", swatch.hsl()),
            row("OKLCH", swatch.oklch()),
            row("SwiftUI", swatch.swiftui()),
            row("NSColor", swatch.nscolor()),
            row(
                t("color_on_white"),
                contrast_value(swatch.contrast([1.0; 3])),
            ),
            row(
                t("color_on_black"),
                contrast_value(swatch.contrast([0.0; 3])),
            ),
        ]
    }

    /// The rows as plain text, one `label  value` per line (the card's excerpt and copy).
    pub fn text(&self) -> String {
        let rows = self.rows();
        let width = rows
            .iter()
            .map(|row| row.label.chars().count())
            .max()
            .unwrap_or(0);
        let mut lines: Vec<String> = rows
            .iter()
            .map(|row| {
                let pad = width - row.label.chars().count();
                format!("{}{}  {}", row.label, " ".repeat(pad), row.value)
            })
            .collect();
        if self.clipped {
            lines.push(crate::locale::t("color_clipped").to_string());
        }
        lines.join("\n")
    }
}

/// A picked colour (`NSColorSampler`, converted to sRGB) as the text that goes on the
/// clipboard: `#RRGGBB`, or `#RRGGBBAA` when it is not opaque.
pub fn picked_text(red: f64, green: f64, blue: f64, alpha: f64) -> String {
    Swatch {
        red,
        green,
        blue,
        alpha,
    }
    .hex()
}

#[cfg(test)]
#[path = "color_tests.rs"]
mod tests;
