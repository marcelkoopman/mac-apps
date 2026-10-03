//! Image ▾: steps on a picture entry, kept as versions like a table's ([`crate::table`]).
//!
//! A version is the original picture with the steps up to it replayed: the steps stay with the
//! history entry, the pictures they make are derived data. The version shown, and a few shown
//! before it, are kept as PNG ([`ImageVersions`]); the others are worked out again from the
//! original when needed. Decoding (with the EXIF orientation applied) and encoding are the
//! platform's ([`Codec`]); the pixel steps are here, on straight RGBA.
//!
//! Every picture made here is zeroized: the pixel buffers when a step is done with them, the
//! versions with their entry (Wipe, Clear history, lock, retention) or when they are forgotten.

use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::Zeroizing;

use crate::clipboard::SecretBytes;
use crate::commands::{ImageFacts, ImageScan};

/// The original and at most this many versions in all, as for a table.
pub const MAX_VERSIONS: usize = 20;
/// Versions kept as pictures: the one shown and the ones shown just before it.
const KEPT_PICTURES: usize = 3;

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
            Self::Resize(Resize::Longest(side)) => format!("Longest side {side}"),
            Self::Resize(Resize::Width(width)) => format!("Width {width}"),
            Self::Resize(Resize::Height(height)) => format!("Height {height}"),
            Self::RotateLeft => "Rotate 90° left".to_string(),
            Self::RotateRight => "Rotate 90° right".to_string(),
            Self::FlipHorizontal => "Flip horizontal".to_string(),
            Self::FlipVertical => "Flip vertical".to_string(),
            Self::RemoveMetadata => "Remove metadata".to_string(),
            Self::Grayscale => "Grayscale".to_string(),
        }
    }

    /// The submenu the step is in, if any.
    pub fn group(&self) -> Option<&'static str> {
        matches!(self, Self::Resize(_)).then_some(RESIZE_GROUP)
    }

    /// The version's label, once its size (`width` × `height`) is known.
    pub fn label(&self, width: usize, height: usize) -> String {
        match self {
            Self::Resize(_) => format!("Resized to {width}×{height}"),
            Self::RotateLeft => "Rotated 90° left".to_string(),
            Self::RotateRight => "Rotated 90° right".to_string(),
            Self::FlipHorizontal => "Flipped horizontally".to_string(),
            Self::FlipVertical => "Flipped vertically".to_string(),
            Self::RemoveMetadata => "Metadata removed".to_string(),
            Self::Grayscale => "Grayscale".to_string(),
        }
    }

    /// The meta-line note when the step would change nothing (a Resize that would enlarge).
    pub fn unchanged_note(&self) -> &'static str {
        match self {
            Self::Resize(_) => "Already that size or smaller",
            _ => "Nothing to change",
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
pub const RESIZE_GROUP: &str = "Resize";

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

/// Decoding and encoding for versions: ImageIO on macOS ([`crate::macos_image_edit`]).
pub trait Codec {
    /// The picture upright (its EXIF orientation applied), at full size.
    fn decode(&self, bytes: &[u8]) -> Option<Rgba>;
    /// `image` as PNG, without metadata.
    fn encode_png(&self, image: &Rgba) -> Option<Vec<u8>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageError {
    /// [`MAX_VERSIONS`] already.
    Full,
    /// A newer job took over.
    Cancelled,
    /// The picture could not be read or written (message for the card).
    Failed(String),
    /// The step would change nothing, so it makes no version (a note, not an error).
    Unchanged(&'static str),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "{MAX_VERSIONS} versions at most"),
            Self::Cancelled => write!(f, "Cancelled"),
            Self::Failed(message) => write!(f, "{message}"),
            Self::Unchanged(note) => write!(f, "{note}"),
        }
    }
}

/// A version worked out: its picture as PNG, its size, and what a scan found in it (info,
/// text, barcodes: worked out after the picture, [`ImageVersions::remember_scan`]).
#[derive(Clone)]
pub struct VersionPicture {
    pub picture: SecretBytes,
    pub facts: ImageFacts,
    pub scan: Option<ImageScan>,
}

impl VersionPicture {
    fn wipe(&mut self) {
        self.picture.zeroize();
        if let Some(scan) = self.scan.as_mut() {
            scan.wipe();
        }
    }
}

/// Work for one version: replay `ops` on the original. Send it to a thread and
/// [`run`](Self::run) it.
pub struct Job {
    pub generation: u64,
    /// The original picture (shares the entry's bytes: zeroized with it).
    original: SecretBytes,
    ops: Vec<ImageOp>,
    /// The version shown when done.
    target: usize,
    /// A new step (`target` is then the version it makes).
    new_step: Option<ImageOp>,
}

/// A finished [`Job`], for [`ImageVersions::finish`]. A copy shares the picture's bytes.
#[derive(Clone)]
pub struct JobDone {
    generation: u64,
    target: usize,
    new_step: Option<(ImageOp, String)>,
    version: VersionPicture,
}

impl JobDone {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The version's picture (PNG), to scan.
    pub fn picture(&self) -> SecretBytes {
        self.version.picture.clone()
    }
}

impl Job {
    /// Decode the original, replay the steps (`cancel` is checked before each) and encode the
    /// version as PNG.
    pub fn run(self, cancel: &AtomicBool, codec: &dyn Codec) -> Result<JobDone, ImageError> {
        let unreadable = || ImageError::Failed("Can't read this picture".to_string());
        let bytes = Zeroizing::new(self.original.with(<[u8]>::to_vec));
        let mut image = codec.decode(&bytes).ok_or_else(unreadable)?;
        drop(bytes);
        let steps = self.ops.iter().map(|op| (op, false));
        for (op, new) in steps.chain(self.new_step.iter().map(|op| (op, true))) {
            if cancel.load(Ordering::Relaxed) {
                return Err(ImageError::Cancelled);
            }
            image = match op.apply(image) {
                Ok(next) => next,
                // A new step that changes nothing makes no version.
                Err(_) if new => return Err(ImageError::Unchanged(op.unchanged_note())),
                // Replayed, a step does what it did when it was taken.
                Err(same) => same,
            };
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(ImageError::Cancelled);
        }
        let png = codec
            .encode_png(&image)
            .ok_or_else(|| ImageError::Failed("Can't write this picture".to_string()))?;
        let facts = ImageFacts {
            format: "PNG".to_string(),
            width: image.width,
            height: image.height,
            byte_len: png.len(),
        };
        let picture = SecretBytes::new(png)
            .ok_or_else(|| ImageError::Failed("Can't write this picture".to_string()))?;
        Ok(JobDone {
            generation: self.generation,
            target: self.target,
            new_step: self
                .new_step
                .map(|op| (op, op.label(image.width, image.height))),
            version: VersionPicture {
                picture,
                facts,
                scan: None,
            },
        })
    }
}

/// The versions of one picture entry (see the module docs).
#[derive(Default)]
pub struct ImageVersions {
    /// Each step and its version's label ("Resized to 1024×768").
    steps: Vec<(ImageOp, String)>,
    /// The version shown: 0 is the original, `n` the one after `steps[n - 1]`.
    cursor: usize,
    generation: u64,
    /// Versions worked out, by index, the one shown last: at most [`KEPT_PICTURES`].
    kept: Vec<(usize, VersionPicture)>,
}

impl Clone for ImageVersions {
    /// A copy keeps the steps; the pictures are worked out again when needed.
    fn clone(&self) -> Self {
        Self {
            steps: self.steps.clone(),
            cursor: self.cursor,
            generation: self.generation,
            kept: Vec::new(),
        }
    }
}

impl Drop for ImageVersions {
    fn drop(&mut self) {
        self.forget_pictures();
    }
}

impl ImageVersions {
    /// Versions there are: the original and one per step.
    pub fn len(&self) -> usize {
        self.steps.len() + 1
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// "Original", then each step's label.
    pub fn labels(&self) -> Vec<String> {
        std::iter::once("Original".to_string())
            .chain(self.steps.iter().map(|(_, label)| label.clone()))
            .collect()
    }

    /// The version shown, when it is a step's and worked out. `None` for the original.
    pub fn shown(&self) -> Option<&VersionPicture> {
        self.kept
            .iter()
            .find(|(index, _)| *index == self.cursor && self.cursor > 0)
            .map(|(_, version)| version)
    }

    /// The job that works out the version shown, when it is a step's and not there.
    pub fn load(&mut self, original: &SecretBytes) -> Option<Job> {
        if self.cursor == 0 || self.shown().is_some() {
            return None;
        }
        let ops = self.ops_to(self.cursor);
        Some(self.job(original, ops, self.cursor, None))
    }

    /// The job for `op` on the version shown. Versions after it are dropped when it is done.
    pub fn push(&mut self, op: ImageOp, original: &SecretBytes) -> Result<Job, ImageError> {
        if self.cursor + 2 > MAX_VERSIONS {
            return Err(ImageError::Full);
        }
        let ops = self.ops_to(self.cursor);
        Ok(self.job(original, ops, self.cursor + 1, Some(op)))
    }

    /// Show version `index` (undo, redo, a pick in the version menu): at once when it is the
    /// original or kept (`None`), else the job that works it out. `None` too when there is no
    /// such version.
    pub fn goto(&mut self, index: usize, original: &SecretBytes) -> Option<Job> {
        if index >= self.len() || index == self.cursor {
            return None;
        }
        if index == 0 || self.kept.iter().any(|(kept, _)| *kept == index) {
            self.cursor = index;
            // A job still running for another version is stale now.
            self.generation = next_generation();
            self.touch(index);
            return None;
        }
        let ops = self.ops_to(index);
        Some(self.job(original, ops, index, None))
    }

    /// Take a finished job: the version it made is shown. `false` (and nothing changes) when a
    /// newer job was made since.
    pub fn finish(&mut self, done: JobDone) -> bool {
        if done.generation != self.generation {
            let mut version = done.version;
            version.wipe();
            return false;
        }
        if let Some(step) = done.new_step {
            self.steps.truncate(self.cursor);
            self.steps.push(step);
            // The versions after the one the step was taken from are gone.
            let last = self.steps.len();
            self.drop_kept(|index| index >= last);
        }
        self.cursor = done.target.min(self.steps.len());
        self.drop_kept(|index| index == done.target);
        self.kept.push((done.target, done.version));
        while self.kept.len() > KEPT_PICTURES {
            let (_, mut version) = self.kept.remove(0);
            version.wipe();
        }
        true
    }

    /// Keep `scan` with the kept version whose picture is `picture`; `false` when there is none
    /// (it was dropped meanwhile).
    pub fn remember_scan(&mut self, picture: &SecretBytes, scan: &ImageScan) -> bool {
        let Some((_, version)) = self
            .kept
            .iter_mut()
            .find(|(_, version)| version.picture.same_allocation(picture))
        else {
            return false;
        };
        if let Some(old) = version.scan.as_mut() {
            old.wipe();
        }
        version.scan = Some(scan.clone());
        true
    }

    /// Drop the pictures (the steps stay). A pending job is stale from now on.
    pub fn forget_pictures(&mut self) {
        for (_, version) in &mut self.kept {
            version.wipe();
        }
        self.kept.clear();
        self.generation = next_generation();
    }

    /// `generation` is this picture's newest job.
    pub fn awaits(&self, generation: u64) -> bool {
        self.generation == generation
    }

    fn ops_to(&self, index: usize) -> Vec<ImageOp> {
        self.steps[..index.min(self.steps.len())]
            .iter()
            .map(|(op, _)| *op)
            .collect()
    }

    fn job(
        &mut self,
        original: &SecretBytes,
        ops: Vec<ImageOp>,
        target: usize,
        new_step: Option<ImageOp>,
    ) -> Job {
        self.generation = next_generation();
        Job {
            generation: self.generation,
            original: original.clone(),
            ops,
            target,
            new_step,
        }
    }

    /// The kept version `index` becomes the one shown last.
    fn touch(&mut self, index: usize) {
        if let Some(at) = self.kept.iter().position(|(kept, _)| *kept == index) {
            let version = self.kept.remove(at);
            self.kept.push(version);
        }
    }

    fn drop_kept(&mut self, gone: impl Fn(usize) -> bool) {
        self.kept.retain_mut(|(index, version)| {
            if gone(*index) {
                version.wipe();
                false
            } else {
                true
            }
        });
    }
}

fn next_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
#[path = "image_edit_tests.rs"]
mod tests;
