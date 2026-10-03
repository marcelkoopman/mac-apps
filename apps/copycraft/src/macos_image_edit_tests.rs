use std::sync::atomic::AtomicBool;

use image::ImageEncoder;
use objc2_core_foundation::{CFData, CFDictionary, CFString, CFType};
use objc2_image_io::CGImageSource;

use super::*;
use crate::clipboard::SecretBytes;
use crate::image_edit::{ImageOp, ImageVersions, Resize};

/// ImageIO without the scan (text recognition is not what these tests are about).
struct NoScan;

impl Codec for NoScan {
    fn decode(&self, bytes: &[u8]) -> Option<Rgba> {
        decode(bytes)
    }

    fn encode_png(&self, image: &Rgba) -> Option<Vec<u8>> {
        encode(image, Encoding::Png)
    }
}

/// `width` × `height`: the left half red, the right half blue, opaque.
fn halves(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            let color = if x < width / 2 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            };
            pixels.extend_from_slice(&color);
            pixels.push(255);
        }
    }
    pixels
}

fn rgb(pixels: &[u8]) -> Vec<u8> {
    pixels
        .chunks(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

fn jpeg(width: usize, height: usize, pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
        .write_image(
            &rgb(pixels),
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

/// `jpeg` with an EXIF block giving the camera make ("Cam") and `orientation` (6: turn a
/// quarter clockwise to view).
fn with_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = b"MM\0\x2a\0\0\0\x08".to_vec();
    tiff.extend_from_slice(&2u16.to_be_bytes()); // two entries
    tiff.extend_from_slice(&0x010Fu16.to_be_bytes()); // Make
    tiff.extend_from_slice(&2u16.to_be_bytes()); // ASCII
    tiff.extend_from_slice(&4u32.to_be_bytes());
    tiff.extend_from_slice(b"Cam\0");
    tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // padding, no next IFD
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let mut out = jpeg[..2].to_vec(); // SOI
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn pixel(image: &Rgba, x: usize, y: usize) -> [u8; 4] {
    let at = (y * image.width + x) * 4;
    image.pixels[at..at + 4].try_into().unwrap()
}

fn near(actual: [u8; 4], expected: [u8; 4]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 24)
}

/// The keys of the encoded picture's properties, those of a nested dictionary after its name
/// ("Orientation", "{GPS}", "{TIFF}.Make"…).
fn property_keys(bytes: &[u8]) -> Vec<String> {
    let data = CFData::from_bytes(bytes);
    let source = unsafe { CGImageSource::with_data(&data, None) }.unwrap();
    let properties = unsafe { source.properties_at_index(0, None) }.unwrap();
    let mut keys = Vec::new();
    flatten("", &properties, &mut keys);
    keys
}

fn flatten(prefix: &str, dict: &CFDictionary, keys: &mut Vec<String>) {
    // SAFETY: image properties have CFString keys.
    let dict: &CFDictionary<CFString, CFType> = unsafe { dict.cast_unchecked() };
    let (names, values) = dict.to_vecs();
    for (name, value) in names.iter().zip(values.iter()) {
        let key = format!("{prefix}{name}");
        if let Some(nested) = value.downcast_ref::<CFDictionary>() {
            flatten(&format!("{key}."), nested, keys);
        }
        keys.push(key);
    }
}

/// The EXIF orientation of encoded picture bytes; 1 (upright) when there is none.
fn orientation(bytes: &[u8]) -> i64 {
    let data = CFData::from_bytes(bytes);
    let source = unsafe { CGImageSource::with_data(&data, None) }.unwrap();
    let properties = unsafe { source.properties_at_index(0, None) }.unwrap();
    let properties: &CFDictionary<CFString, CFType> = unsafe { properties.cast_unchecked() };
    let key = unsafe { objc2_image_io::kCGImagePropertyOrientation };
    properties
        .get(key)
        .and_then(|value| {
            value
                .downcast_ref::<objc2_core_foundation::CFNumber>()?
                .as_i64()
        })
        .unwrap_or(1)
}

fn uti_of(bytes: &[u8]) -> String {
    let data = CFData::from_bytes(bytes);
    let source = unsafe { CGImageSource::with_data(&data, None) }.unwrap();
    unsafe { source.r#type() }
        .map(|uti| uti.to_string())
        .unwrap_or_default()
}

#[test]
fn exif_orientation_is_applied_when_decoding() {
    // Stored 16 × 8, red left, blue right; orientation 6 shows it 8 × 16, red on top.
    let stored = jpeg(16, 8, &halves(16, 8));
    let rotated = with_orientation(&stored, 6);
    let keys = property_keys(&rotated);
    assert!(keys.contains(&"Orientation".to_string()), "{keys:?}");
    assert!(keys.contains(&"{TIFF}.Make".to_string()), "{keys:?}");
    let image = decode(&rotated).unwrap();
    assert_eq!((image.width, image.height), (8, 16));
    assert!(
        near(pixel(&image, 4, 2), [255, 0, 0, 255]),
        "{:?}",
        pixel(&image, 4, 2)
    );
    assert!(
        near(pixel(&image, 4, 13), [0, 0, 255, 255]),
        "{:?}",
        pixel(&image, 4, 13)
    );
    // Without the orientation, it is as stored.
    let plain = decode(&stored).unwrap();
    assert_eq!((plain.width, plain.height), (16, 8));
}

#[test]
fn png_round_trips_exactly_with_transparency() {
    let mut pixels = halves(6, 4);
    pixels[3] = 0; // one transparent pixel
    let image = Rgba::new(6, 4, pixels.clone()).unwrap();
    let png = encode(&image, Encoding::Png).unwrap();
    assert_eq!(uti_of(&png), "public.png");
    let back = decode(&png).unwrap();
    assert_eq!((back.width, back.height), (6, 4));
    // sRGB in, sRGB out: the same pixels, give or take rounding.
    for (got, sent) in back.pixels[4..].iter().zip(&pixels[4..]) {
        assert!(got.abs_diff(*sent) <= 2, "{got} vs {sent}");
    }
    assert_eq!(back.pixels[3], 0);
}

#[test]
fn jpeg_encodes_with_quality_and_without_transparency() {
    let mut pixels = halves(32, 16);
    pixels[3] = 0;
    let image = Rgba::new(32, 16, pixels).unwrap();
    let high = encode(&image, Encoding::Jpeg(0.9)).unwrap();
    let low = encode(&image, Encoding::Jpeg(0.2)).unwrap();
    assert_eq!(uti_of(&high), "public.jpeg");
    assert!(low.len() < high.len(), "{} < {}", low.len(), high.len());
    let back = decode(&high).unwrap();
    assert_eq!((back.width, back.height), (32, 16));
    assert!(
        near(pixel(&back, 0, 0), [255, 255, 255, 255]),
        "white, not black"
    );
    assert!(near(pixel(&back, 8, 8), [255, 0, 0, 255]));
    assert!(near(pixel(&back, 24, 8), [0, 0, 255, 255]));
}

#[test]
fn heic_encodes_and_decodes() {
    let image = Rgba::new(64, 32, halves(64, 32)).unwrap();
    let heic = encode(&image, Encoding::Heic(0.8)).expect("ImageIO writes HEIC");
    assert_eq!(uti_of(&heic), "public.heic");
    let back = decode(&heic).unwrap();
    assert_eq!((back.width, back.height), (64, 32));
    assert!(
        near(pixel(&back, 10, 16), [255, 0, 0, 255]),
        "{:?}",
        pixel(&back, 10, 16)
    );
    assert!(
        near(pixel(&back, 54, 16), [0, 0, 255, 255]),
        "{:?}",
        pixel(&back, 54, 16)
    );
}

#[test]
fn versions_carry_no_metadata_and_stay_upright() {
    let rotated = with_orientation(&jpeg(16, 8, &halves(16, 8)), 6);
    for encoding in [Encoding::Png, Encoding::Jpeg(0.9), Encoding::Heic(0.8)] {
        let bytes = encode(&decode(&rotated).unwrap(), encoding).unwrap();
        let keys = property_keys(&bytes);
        // No orientation, camera or place: ImageIO may still note the color space and size.
        assert_eq!(orientation(&bytes), 1, "{encoding:?}: upright as written");
        for gone in ["{TIFF}.Make", "{GPS}"] {
            assert!(
                !keys.contains(&gone.to_string()),
                "{encoding:?}: {gone} in {keys:?}"
            );
        }
        let back = decode(&bytes).unwrap();
        assert_eq!((back.width, back.height), (8, 16), "{encoding:?}");
    }
}

#[test]
fn steps_replay_through_imageio() {
    let original = SecretBytes::new(with_orientation(&jpeg(40, 20, &halves(40, 20)), 6)).unwrap();
    let mut versions = ImageVersions::default();
    let run = |job: crate::image_edit::Job| job.run(&AtomicBool::new(false), &NoScan).unwrap();
    // Upright it is 20 × 40, red on top.
    for op in [
        ImageOp::RotateRight,
        ImageOp::Resize(Resize::Percent(50)),
        ImageOp::FlipHorizontal,
        ImageOp::Grayscale,
    ] {
        let job = versions.push(op, &original).unwrap();
        assert!(versions.finish(run(job)));
    }
    assert_eq!(
        versions.labels(),
        [
            "Original",
            "Rotated 90° right",
            "Resized to 20×10",
            "Flipped horizontally",
            "Grayscale"
        ]
    );
    let shown = versions.shown().unwrap();
    assert_eq!((shown.facts.width, shown.facts.height), (20, 10));
    let image = shown.picture.with(decode).unwrap();
    // Red went right with the turn and left again with the flip; gray now.
    let left = pixel(&image, 2, 5);
    let right = pixel(&image, 17, 5);
    assert_eq!((left[0], left[1]), (left[1], left[2]), "gray: {left:?}");
    assert!(
        left[0] > right[0] + 20,
        "red is lighter than blue: {left:?} {right:?}"
    );
}

#[test]
fn unreadable_and_empty_bytes_do_not_decode() {
    assert!(decode(b"").is_none());
    assert!(decode(b"not a picture").is_none());
}

#[test]
fn save_writes_the_format_picked() {
    use crate::image_edit::{ImageFile, JPEG_MEDIUM};
    use crate::macos_save::SaveJob;
    let original = with_orientation(&jpeg(16, 8, &halves(16, 8)), 6);
    let dir = std::env::temp_dir().join(format!("copycraft-image-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let as_is = ImageFile::AsIs {
        label: "JPEG",
        extension: "jpg",
    };
    for (file, name, uti) in [
        (as_is, "photo.jpg", "public.jpeg"),
        (ImageFile::Png, "photo.png", "public.png"),
        (JPEG_MEDIUM, "photo.jpg", "public.jpeg"),
        (ImageFile::Heic, "photo.heic", "public.heic"),
    ] {
        let bytes = zeroize::Zeroizing::new(original.clone());
        let job = SaveJob::image("photo.jpeg", bytes, false, (vec![file], 0));
        assert_eq!(job.filename, name);
        let path = dir.join(&job.filename);
        job.content.write_to(&path).unwrap();
        let written = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(uti_of(&written), uti, "{file:?}");
        if file == as_is {
            assert_eq!(written, original, "as it is: byte for byte");
            continue;
        }
        let keys = property_keys(&written);
        assert_eq!(orientation(&written), 1, "{file:?}: {keys:?}");
        assert!(
            !keys.contains(&"{TIFF}.Make".to_string()),
            "{file:?}: {keys:?}"
        );
        let back = decode(&written).unwrap();
        assert_eq!((back.width, back.height), (8, 16), "{file:?} upright");
    }
    // A version's PNG is written as it is for PNG.
    let png = encode(&decode(&original).unwrap(), Encoding::Png).unwrap();
    let job = SaveJob::image(
        "clipboard",
        zeroize::Zeroizing::new(png.clone()),
        true,
        crate::image_edit::image_files(Some(("JPEG", "jpg")), false),
    );
    assert_eq!(job.filename, "clipboard.jpg", "the source's format first");
    let job = SaveJob::image(
        "clipboard",
        zeroize::Zeroizing::new(png.clone()),
        true,
        (vec![ImageFile::Png], 0),
    );
    let path = dir.join(&job.filename);
    job.content.write_to(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), png);
    std::fs::remove_dir_all(&dir).unwrap();
}
