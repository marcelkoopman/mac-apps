//! Image ▾: steps on the picture on the card. A step's result is a new copy, like a Text ▾
//! step's: it goes on the clipboard and into history (`App::image_stepped`); the picture it was
//! taken on stays as it was. Decoding (with the EXIF orientation applied) and encoding are the
//! platform's ([`Codec`]); the pixel steps are here, on straight RGBA.
//!
//! Every picture made here is zeroized: the pixel buffers when a step is done with them, a
//! result that is not copied when it is dropped, a copied one with its history entry.

use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;

/// How Resize sizes a picture. It never makes one larger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resize {
    /// This percentage of each side.
    Percent(u32),
    /// The longest side this many pixels.
    Longest(u32),
    /// This many pixels wide.
    Width(u32),
    /// This many pixels high.
    Height(u32),
}

/// One step from a version to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageOp {
    Resize(Resize),
    RotateLeft,
    RotateRight,
    FlipHorizontal,
    FlipVertical,
    /// The pixels as they are, without EXIF, GPS or other metadata (as every version is).
    RemoveMetadata,
    Grayscale,
}

impl ImageOp {
    /// The steps of the "Image ▾" menu, in its order (Resize › Custom… comes after the sizes).
    pub const MENU: [ImageOp; 11] = [
        ImageOp::Resize(Resize::Percent(50)),
        ImageOp::Resize(Resize::Percent(25)),
        ImageOp::Resize(Resize::Longest(2048)),
        ImageOp::Resize(Resize::Longest(1024)),
        ImageOp::Resize(Resize::Longest(512)),
        ImageOp::RotateLeft,
        ImageOp::RotateRight,
        ImageOp::FlipHorizontal,
        ImageOp::FlipVertical,
        ImageOp::RemoveMetadata,
        ImageOp::Grayscale,
    ];

    /// The menu item.
    pub fn title(&self) -> String {
        match self {
            Self::Resize(Resize::Percent(percent)) => format!("{percent}%"),
            Self::Resize(Resize::Longest(side)) => crate::locale::tf("image_longest_side", &[side]),
            Self::Resize(Resize::Width(width)) => crate::locale::tf("image_width", &[width]),
            Self::Resize(Resize::Height(height)) => crate::locale::tf("image_height", &[height]),
            Self::RotateLeft => crate::locale::t("image_rotate_left").to_string(),
            Self::RotateRight => crate::locale::t("image_rotate_right").to_string(),
            Self::FlipHorizontal => crate::locale::t("image_flip_horizontal").to_string(),
            Self::FlipVertical => crate::locale::t("image_flip_vertical").to_string(),
            Self::RemoveMetadata => crate::locale::t("image_remove_metadata").to_string(),
            Self::Grayscale => crate::locale::t("image_grayscale").to_string(),
        }
    }

    /// The submenu the step is in, if any.
    pub fn group(&self) -> Option<&'static str> {
        matches!(self, Self::Resize(_)).then_some(resize_group())
    }

    /// The version's label, once its size (`width` × `height`) is known.
    pub fn label(&self, width: usize, height: usize) -> String {
        match self {
            Self::Resize(_) => crate::locale::tf("image_resized_to", &[&width, &height]),
            Self::RotateLeft => crate::locale::t("image_rotated_left").to_string(),
            Self::RotateRight => crate::locale::t("image_rotated_right").to_string(),
            Self::FlipHorizontal => crate::locale::t("image_flipped_horizontally").to_string(),
            Self::FlipVertical => crate::locale::t("image_flipped_vertically").to_string(),
            Self::RemoveMetadata => crate::locale::t("image_metadata_removed").to_string(),
            Self::Grayscale => crate::locale::t("image_grayscale").to_string(),
        }
    }

    /// The meta-line note when the step would change nothing (a Resize that would enlarge).
    pub fn unchanged_note(&self) -> &'static str {
        match self {
            Self::Resize(_) => crate::locale::t("image_already_that_size"),
            _ => crate::locale::t("nothing_to_change"),
        }
    }

    /// `image` after this step, or `Err` with `image` as it was when the step would change
    /// nothing (Resize never enlarges).
    pub fn apply(&self, image: Rgba) -> Result<Rgba, Rgba> {
        match self {
            Self::Resize(to) => match resized_size(image.width, image.height, *to) {
                Some((width, height)) => Ok(resize(image, width, height)),
                None => Err(image),
            },
            Self::RotateLeft => Ok(rotate(image, false)),
            Self::RotateRight => Ok(rotate(image, true)),
            Self::FlipHorizontal => Ok(flip(image, true)),
            Self::FlipVertical => Ok(flip(image, false)),
            Self::RemoveMetadata => Ok(image),
            Self::Grayscale => Ok(grayscale(image)),
        }
    }
}

/// The Resize submenu.
pub fn resize_group() -> &'static str {
    crate::locale::t("image_resize")
}

/// `Custom…` in the Resize submenu: "1200" or "w 1200" is a width, "h 800" a height (also
/// "1200w", "width 1200", "800 px high"…). `None` for anything else, or 0.
pub fn parse_custom_resize(text: &str) -> Option<Resize> {
    let lower = text.trim().to_lowercase();
    let mut numbers = lower
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty());
    let value: u32 = numbers.next()?.parse().ok().filter(|value| *value > 0)?;
    if numbers.next().is_some() {
        return None;
    }
    let words: String = lower
        .chars()
        .filter(|ch| !ch.is_ascii_digit() && !ch.is_whitespace())
        .collect();
    let words = words.replace("px", "");
    match words.as_str() {
        "" | "w" | "width" | "wide" => Some(Resize::Width(value)),
        "h" | "height" | "high" => Some(Resize::Height(value)),
        _ => None,
    }
}

/// The size `width` × `height` becomes, keeping its aspect ratio; `None` when that is not
/// smaller (Resize never enlarges) or the picture is empty. A side is at least 1 pixel.
pub fn resized_size(width: usize, height: usize, to: Resize) -> Option<(usize, usize)> {
    if width == 0 || height == 0 {
        return None;
    }
    // `side * target / of`, rounded, at least 1.
    let scaled = |side: usize, target: usize, of: usize| ((side * target + of / 2) / of).max(1);
    let size = match to {
        Resize::Percent(percent) => {
            let percent = percent as usize;
            if percent == 0 || percent >= 100 {
                return None;
            }
            (scaled(width, percent, 100), scaled(height, percent, 100))
        }
        Resize::Longest(side) => {
            let side = side as usize;
            let longest = width.max(height);
            if side == 0 || side >= longest {
                return None;
            }
            (scaled(width, side, longest), scaled(height, side, longest))
        }
        Resize::Width(target) => {
            let target = target as usize;
            if target == 0 || target >= width {
                return None;
            }
            (target, scaled(height, target, width))
        }
        Resize::Height(target) => {
            let target = target as usize;
            if target == 0 || target >= height {
                return None;
            }
            (scaled(width, target, height), target)
        }
    };
    (size != (width, height)).then_some(size)
}

/// Straight (not premultiplied) 8-bit RGBA, rows top to bottom. Zeroized when dropped.
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub pixels: Zeroizing<Vec<u8>>,
}

impl Rgba {
    /// `None` unless `pixels` holds exactly `width` × `height` pixels.
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Option<Self> {
        let pixels = Zeroizing::new(pixels);
        (width > 0 && height > 0 && pixels.len() == width * height * 4).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    fn blank(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: Zeroizing::new(vec![0; width * height * 4]),
        }
    }

    fn pixel(&self, x: usize, y: usize) -> &[u8] {
        let at = (y * self.width + x) * 4;
        &self.pixels[at..at + 4]
    }
}

/// A quarter turn, clockwise (`clockwise`) or counterclockwise.
fn rotate(image: Rgba, clockwise: bool) -> Rgba {
    let (width, height) = (image.height, image.width);
    let mut out = Rgba::blank(width, height);
    for y in 0..height {
        for x in 0..width {
            // The source pixel that lands on (x, y).
            let (sx, sy) = if clockwise {
                (y, image.height - 1 - x)
            } else {
                (image.width - 1 - y, x)
            };
            let at = (y * width + x) * 4;
            out.pixels[at..at + 4].copy_from_slice(image.pixel(sx, sy));
        }
    }
    out
}

/// Feather the picture to transparent towards its edge: alpha stays as it is inside a circle
/// of radius `1 - fade` (half the shorter side is 1) and eases (smoothstep) to 0 at radius 1,
/// so the corners are fully clear. For a tile shown on a background of about its own colour.
pub fn feather_radial(mut image: Rgba, fade: f64) -> Rgba {
    let (width, height) = (image.width, image.height);
    let (cx, cy) = (width as f64 / 2.0, height as f64 / 2.0);
    let radius = cx.min(cy).max(1.0);
    let inner = (1.0 - fade.clamp(0.01, 1.0)).max(0.0);
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f64 + 0.5 - cx) / radius;
            let dy = (y as f64 + 0.5 - cy) / radius;
            let d = (dx * dx + dy * dy).sqrt();
            let t = ((1.0 - d) / (1.0 - inner)).clamp(0.0, 1.0);
            let keep = t * t * (3.0 - 2.0 * t);
            let at = (y * width + x) * 4 + 3;
            image.pixels[at] = (f64::from(image.pixels[at]) * keep).round() as u8;
        }
    }
    image
}

/// A mirror image: left to right (`horizontal`) or top to bottom.
fn flip(mut image: Rgba, horizontal: bool) -> Rgba {
    let (width, height) = (image.width, image.height);
    let row = width * 4;
    if horizontal {
        for line in image.pixels.chunks_exact_mut(row) {
            for x in 0..width / 2 {
                let (left, right) = (x * 4, (width - 1 - x) * 4);
                for channel in 0..4 {
                    line.swap(left + channel, right + channel);
                }
            }
        }
    } else {
        for y in 0..height / 2 {
            let (top, bottom) = (y * row, (height - 1 - y) * row);
            for offset in 0..row {
                image.pixels.swap(top + offset, bottom + offset);
            }
        }
    }
    image
}

/// Luma (Rec. 709 weights, 54 + 183 + 19 = 256) in every color channel; alpha as it was.
fn grayscale(mut image: Rgba) -> Rgba {
    for pixel in image.pixels.as_chunks_mut::<4>().0 {
        let luma =
            (54 * u32::from(pixel[0]) + 183 * u32::from(pixel[1]) + 19 * u32::from(pixel[2]) + 128)
                >> 8;
        let luma = luma.min(255) as u8;
        pixel[0] = luma;
        pixel[1] = luma;
        pixel[2] = luma;
    }
    image
}

/// Lanczos3 to `width` × `height`, on premultiplied pixels so transparent ones lend no color.
fn resize(mut image: Rgba, width: usize, height: usize) -> Rgba {
    premultiply(&mut image.pixels);
    let source = std::mem::take(&mut *image.pixels);
    let Some(buffer) = image::RgbaImage::from_raw(image.width as u32, image.height as u32, source)
    else {
        return Rgba::blank(width, height);
    };
    let resized = image::imageops::resize(
        &buffer,
        width as u32,
        height as u32,
        image::imageops::FilterType::Lanczos3,
    );
    // The source buffer goes back to be zeroized with `image`.
    *image.pixels = buffer.into_raw();
    let mut pixels = Zeroizing::new(resized.into_raw());
    unpremultiply(&mut pixels);
    Rgba {
        width,
        height,
        pixels,
    }
}

fn premultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel[..3].fill(0);
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

/// What Save writes a picture as: the Format popup under the save panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFile {
    /// The original as it came, byte for byte (metadata too): only for the original version.
    AsIs {
        label: &'static str,
        extension: &'static str,
    },
    Png,
    /// JPEG at this quality, in percent.
    Jpeg(u8),
    Heic,
}

/// The JPEG choices: quality 0.9 and 0.7.
pub const JPEG_HIGH: ImageFile = ImageFile::Jpeg(90);
pub const JPEG_MEDIUM: ImageFile = ImageFile::Jpeg(70);
/// HEIC quality.
pub const HEIC_QUALITY: f64 = 0.8;

impl ImageFile {
    /// The popup item.
    pub fn title(&self) -> String {
        match self {
            Self::AsIs { label, .. } => crate::locale::tf("image_file_as_is", &[label]),
            Self::Png => "PNG".to_string(),
            Self::Jpeg(90) => crate::locale::t("image_jpeg_high").to_string(),
            Self::Jpeg(70) => crate::locale::t("image_jpeg_medium").to_string(),
            Self::Jpeg(quality) => format!("JPEG ({:.2})", f64::from(*quality) / 100.0),
            Self::Heic => "HEIC".to_string(),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::AsIs { extension, .. } => extension,
            Self::Png => "png",
            Self::Jpeg(_) => "jpg",
            Self::Heic => "heic",
        }
    }
}

/// The Format popup for a picture whose original is `source` (its label and extension, "JPEG"
/// and "jpg"), and the item picked first: the source's format. For the original version
/// (`original`), that is the original as it is; for a step's version, PNG, JPEG (high) or HEIC
/// as the source was, else PNG.
pub fn image_files(
    source: Option<(&'static str, &'static str)>,
    original: bool,
) -> (Vec<ImageFile>, usize) {
    let mut files = vec![ImageFile::Png, JPEG_HIGH, JPEG_MEDIUM, ImageFile::Heic];
    match source {
        Some((label, extension)) if original => {
            files.insert(0, ImageFile::AsIs { label, extension });
            (files, 0)
        }
        Some(("JPEG", _)) => (files, 1),
        Some(("HEIC", _)) => (files, 3),
        _ => (files, 0),
    }
}

/// Decoding and encoding for versions: ImageIO on macOS ([`crate::macos_image_edit`]).
pub trait Codec {
    /// The picture upright (its EXIF orientation applied), at full size.
    fn decode(&self, bytes: &[u8]) -> Option<Rgba>;
    /// `image` as PNG, without metadata.
    fn encode_png(&self, image: &Rgba) -> Option<Vec<u8>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// A newer job took over.
    Cancelled,
    /// The picture could not be read or written (message for the card).
    Failed(String),
    /// The step would change nothing, so it makes no copy (a note, not an error).
    Unchanged(&'static str),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "{}", crate::locale::t("cancelled")),
            Self::Failed(message) => write!(f, "{message}"),
            Self::Unchanged(note) => write!(f, "{note}"),
        }
    }
}

/// One Image ▾ step on the picture on the card. Send it to a thread and [`run`](Self::run) it.
pub struct Job {
    pub generation: u64,
    /// The picture the step is taken on (shares its entry's bytes: zeroized with it).
    original: SecretBytes,
    op: ImageOp,
}

/// A finished [`Job`]: the new picture as PNG and what the step did. It is copied at once; its
/// scan (info, text, barcodes) runs afterwards, off the main thread. A copy shares the
/// picture's bytes.
#[derive(Clone)]
pub struct JobDone {
    /// "Converted to grayscale", for the card of the copy it makes.
    pub label: String,
    pub picture: SecretBytes,
}

impl JobDone {
    /// Overwrite the picture: a result that is not copied after all.
    pub fn wipe(&mut self) {
        self.picture.zeroize();
    }
}

impl Job {
    pub fn new(op: ImageOp, original: &SecretBytes) -> Self {
        Self {
            generation: next_generation(),
            original: original.clone(),
            op,
        }
    }

    /// The allocation of the picture the step is taken on ([`SecretBytes::allocation_id`]).
    pub fn source(&self) -> usize {
        self.original.allocation_id()
    }

    /// Decode the picture, take the step and encode the result as PNG. `cancel` is checked
    /// before and after the step.
    pub fn run(self, cancel: &AtomicBool, codec: &dyn Codec) -> Result<JobDone, ImageError> {
        let unreadable = || ImageError::Failed(crate::locale::t("image_cannot_read").to_string());
        let bytes = Zeroizing::new(self.original.with(<[u8]>::to_vec));
        let image = codec.decode(&bytes).ok_or_else(unreadable)?;
        drop(bytes);
        if cancel.load(Ordering::Relaxed) {
            return Err(ImageError::Cancelled);
        }
        // A step that changes nothing makes no copy.
        let image = self
            .op
            .apply(image)
            .map_err(|_| ImageError::Unchanged(self.op.unchanged_note()))?;
        if cancel.load(Ordering::Relaxed) {
            return Err(ImageError::Cancelled);
        }
        let png = codec.encode_png(&image).ok_or_else(|| {
            ImageError::Failed(crate::locale::t("image_cannot_write").to_string())
        })?;
        let picture = SecretBytes::new(png).ok_or_else(|| {
            ImageError::Failed(crate::locale::t("image_cannot_write").to_string())
        })?;
        Ok(JobDone {
            label: self.op.label(image.width, image.height),
            picture,
        })
    }
}

fn next_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
#[path = "image_edit_tests.rs"]
mod tests;
