use std::sync::atomic::AtomicBool;

use super::*;

/// PNG through the `image` crate: what ImageIO does on macOS, for these tests.
struct PngCodec;

impl Codec for PngCodec {
    fn decode(&self, bytes: &[u8]) -> Option<Rgba> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Rgba::new(width, height, image.into_raw())
    }

    fn encode_png(&self, image: &Rgba) -> Option<Vec<u8>> {
        png(image.width, image.height, &image.pixels)
    }
}

fn png(width: usize, height: usize, pixels: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            pixels,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(out)
}

/// `width` × `height`, each pixel (x, y, x + y, 255).
fn gradient(width: usize, height: usize) -> Rgba {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[x as u8, y as u8, (x + y) as u8, 255]);
        }
    }
    Rgba::new(width, height, pixels).unwrap()
}

fn original(width: usize, height: usize) -> SecretBytes {
    let image = gradient(width, height);
    SecretBytes::new(png(width, height, &image.pixels).unwrap()).unwrap()
}

fn run(job: Job) -> Result<JobDone, ImageError> {
    job.run(&AtomicBool::new(false), &PngCodec)
}

/// The size of a step's result.
fn size_of(done: &JobDone) -> (usize, usize) {
    let image = done.picture.with(|bytes| PngCodec.decode(bytes)).unwrap();
    (image.width, image.height)
}

#[test]
fn resize_keeps_the_aspect_ratio_and_never_enlarges() {
    assert_eq!(
        resized_size(4000, 3000, Resize::Percent(50)),
        Some((2000, 1500))
    );
    assert_eq!(
        resized_size(4000, 3000, Resize::Percent(25)),
        Some((1000, 750))
    );
    assert_eq!(
        resized_size(4000, 3000, Resize::Longest(1024)),
        Some((1024, 768))
    );
    assert_eq!(
        resized_size(3000, 4000, Resize::Longest(1024)),
        Some((768, 1024))
    );
    assert_eq!(
        resized_size(4000, 3000, Resize::Width(1200)),
        Some((1200, 900))
    );
    assert_eq!(
        resized_size(4000, 3000, Resize::Height(600)),
        Some((800, 600))
    );
    // Rounded, at least one pixel.
    assert_eq!(
        resized_size(1001, 333, Resize::Percent(50)),
        Some((501, 167))
    );
    assert_eq!(resized_size(5000, 3, Resize::Longest(512)), Some((512, 1)));
    // Not smaller: no step.
    assert_eq!(resized_size(800, 600, Resize::Longest(1024)), None);
    assert_eq!(resized_size(1024, 600, Resize::Longest(1024)), None);
    assert_eq!(resized_size(800, 600, Resize::Width(800)), None);
    assert_eq!(resized_size(800, 600, Resize::Height(900)), None);
    assert_eq!(resized_size(800, 600, Resize::Percent(100)), None);
    assert_eq!(resized_size(800, 600, Resize::Percent(0)), None);
    assert_eq!(resized_size(1, 1, Resize::Percent(50)), None);
    assert_eq!(resized_size(0, 600, Resize::Percent(50)), None);
}

#[test]
fn custom_resize_reads_a_width_or_a_height() {
    assert_eq!(parse_custom_resize("1200"), Some(Resize::Width(1200)));
    assert_eq!(parse_custom_resize(" w 1200 "), Some(Resize::Width(1200)));
    assert_eq!(parse_custom_resize("1200w"), Some(Resize::Width(1200)));
    assert_eq!(
        parse_custom_resize("Width 1200 px"),
        Some(Resize::Width(1200))
    );
    assert_eq!(parse_custom_resize("h 800"), Some(Resize::Height(800)));
    assert_eq!(parse_custom_resize("800H"), Some(Resize::Height(800)));
    assert_eq!(parse_custom_resize("height: 800"), None);
    assert_eq!(parse_custom_resize("800 high"), Some(Resize::Height(800)));
    assert_eq!(parse_custom_resize("0"), None);
    assert_eq!(parse_custom_resize("1200x800"), None);
    assert_eq!(parse_custom_resize("12 00"), None);
    assert_eq!(parse_custom_resize("big"), None);
    assert_eq!(parse_custom_resize(""), None);
}

#[test]
fn steps_have_menu_titles_groups_and_labels() {
    let titles: Vec<String> = ImageOp::MENU.iter().map(ImageOp::title).collect();
    assert_eq!(
        titles,
        [
            "50%",
            "25%",
            "Longest side 2048",
            "Longest side 1024",
            "Longest side 512",
            "Rotate 90° Left",
            "Rotate 90° Right",
            "Flip Horizontal",
            "Flip Vertical",
            "Remove Metadata",
            "Grayscale",
        ]
    );
    let grouped = ImageOp::MENU
        .iter()
        .filter(|op| op.group() == Some("Resize"));
    assert_eq!(grouped.count(), 5);
    assert_eq!(
        ImageOp::Resize(Resize::Longest(1024)).label(1024, 768),
        "Resized to 1024×768"
    );
    assert_eq!(ImageOp::RotateLeft.label(1, 1), "Rotated 90° left");
    assert_eq!(ImageOp::FlipVertical.label(1, 1), "Flipped vertically");
    assert_eq!(ImageOp::RemoveMetadata.label(1, 1), "Metadata removed");
}

#[test]
fn rotating_and_flipping_move_the_pixels() {
    // 3 × 2: rows (0,1,2) and (3,4,5) in the red channel.
    let image = || {
        let pixels = (0..6u8).flat_map(|red| [red, 0, 0, 255]).collect();
        Rgba::new(3, 2, pixels).unwrap()
    };
    let reds = |image: &Rgba| -> Vec<u8> { image.pixels.chunks(4).map(|px| px[0]).collect() };
    let right = ImageOp::RotateRight.apply(image()).ok().unwrap();
    assert_eq!((right.width, right.height), (2, 3));
    assert_eq!(reds(&right), [3, 0, 4, 1, 5, 2]);
    let left = ImageOp::RotateLeft.apply(image()).ok().unwrap();
    assert_eq!((left.width, left.height), (2, 3));
    assert_eq!(reds(&left), [2, 5, 1, 4, 0, 3]);
    let flipped = ImageOp::FlipHorizontal.apply(image()).ok().unwrap();
    assert_eq!(reds(&flipped), [2, 1, 0, 5, 4, 3]);
    let flipped = ImageOp::FlipVertical.apply(image()).ok().unwrap();
    assert_eq!(reds(&flipped), [3, 4, 5, 0, 1, 2]);
    // Four quarter turns, or two flips, are where it started.
    let mut turned = image();
    for _ in 0..4 {
        turned = ImageOp::RotateRight.apply(turned).ok().unwrap();
    }
    assert_eq!(reds(&turned), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn grayscale_keeps_alpha_and_resize_shrinks() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
    ];
    let gray = ImageOp::Grayscale
        .apply(Rgba::new(2, 2, pixels).unwrap())
        .ok()
        .unwrap();
    assert_eq!(
        &gray.pixels[..],
        [
            54, 54, 54, 255, 182, 182, 182, 128, 19, 19, 19, 0, 255, 255, 255, 255
        ]
    );
    let small = ImageOp::Resize(Resize::Percent(50))
        .apply(gradient(40, 20))
        .ok()
        .unwrap();
    assert_eq!(
        (small.width, small.height, small.pixels.len()),
        (20, 10, 800)
    );
    // A larger size gives the picture back as it was.
    let same = ImageOp::Resize(Resize::Longest(100)).apply(gradient(40, 20));
    assert_eq!(same.err().map(|image| image.width), Some(40));
}

#[test]
fn a_step_makes_a_new_picture_and_leaves_the_original() {
    let original = original(40, 20);
    let before = original.with(<[u8]>::to_vec);
    let job = Job::new(ImageOp::RotateRight, &original);
    assert_eq!(job.original().allocation_id(), original.allocation_id());
    let done = run(job).unwrap();
    assert_eq!(done.label, "Rotated 90° right");
    assert_eq!(size_of(&done), (20, 40));
    assert!(!done.picture.same_allocation(&original));
    assert_eq!(
        original.with(<[u8]>::to_vec),
        before,
        "the original is untouched"
    );
    // A step on that result is a step on the new picture.
    let done = run(Job::new(
        ImageOp::Resize(Resize::Percent(50)),
        &done.picture,
    ))
    .unwrap();
    assert_eq!(done.label, "Resized to 10×20");
    assert_eq!(size_of(&done), (10, 20));
}

#[test]
fn a_step_that_changes_nothing_makes_no_copy() {
    let original = original(40, 20);
    let job = Job::new(ImageOp::Resize(Resize::Longest(2048)), &original);
    assert_eq!(
        run(job).err().map(|e| e.to_string()),
        Some("Already that size or smaller".to_string())
    );
}

#[test]
fn every_job_is_newer_and_a_cancelled_one_is_dropped() {
    let original = original(40, 20);
    let old = Job::new(ImageOp::Grayscale, &original);
    let new = Job::new(ImageOp::FlipVertical, &original);
    assert!(new.generation > old.generation);
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        new.run(&cancel, &PngCodec),
        Err(ImageError::Cancelled)
    ));
    let mut done = run(old).unwrap();
    let picture = done.picture.clone();
    done.wipe();
    assert!(picture.with(|bytes| bytes.iter().all(|&b| b == 0)), "wiped");
}

#[test]
fn an_unreadable_picture_fails_the_job() {
    let original = SecretBytes::new(b"not a picture".to_vec()).unwrap();
    let job = Job::new(ImageOp::Grayscale, &original);
    assert!(matches!(run(job), Err(ImageError::Failed(_))));
}

#[test]
fn save_offers_the_source_format_first() {
    let titles =
        |files: &[ImageFile]| -> Vec<String> { files.iter().map(ImageFile::title).collect() };
    // The original: as it is (its bytes), then the re-encodings.
    let (files, picked) = image_files(Some(("JPEG", "jpg")), true);
    assert_eq!(
        titles(&files),
        [
            "JPEG (as it is)",
            "PNG",
            "JPEG — high (0.9)",
            "JPEG — medium (0.7)",
            "HEIC"
        ]
    );
    assert_eq!((picked, files[picked].extension()), (0, "jpg"));
    let (files, picked) = image_files(Some(("GIF", "gif")), true);
    assert_eq!(
        (files[picked].title(), files[picked].extension()),
        ("GIF (as it is)".to_string(), "gif")
    );
    // A step's version: the source's format among PNG, JPEG and HEIC, else PNG.
    let (files, picked) = image_files(Some(("JPEG", "jpg")), false);
    assert_eq!(
        titles(&files),
        ["PNG", "JPEG — high (0.9)", "JPEG — medium (0.7)", "HEIC"]
    );
    assert_eq!(files[picked], JPEG_HIGH);
    let (files, picked) = image_files(Some(("HEIC", "heic")), false);
    assert_eq!(
        (files[picked], files[picked].extension()),
        (ImageFile::Heic, "heic")
    );
    let (files, picked) = image_files(Some(("GIF", "gif")), false);
    assert_eq!(files[picked], ImageFile::Png);
    let (files, picked) = image_files(None, true);
    assert_eq!((files.len(), files[picked]), (4, ImageFile::Png));
    assert_eq!(JPEG_MEDIUM.extension(), "jpg");
}

#[test]
fn a_quarter_turn_of_a_square_picture_changes_its_pixels_both_ways() {
    // Square: the size stays, so only the pixels can show the turn (Marcel saw no change).
    let n = 4;
    let source = original(n, n);
    let pixel = |done: &JobDone, x: usize, y: usize| -> Vec<u8> {
        let image = done.picture.with(|bytes| PngCodec.decode(bytes)).unwrap();
        image.pixels[(y * image.width + x) * 4..][..4].to_vec()
    };
    // gradient: (x, y, x + y, 255).
    let at = |x: usize, y: usize| vec![x as u8, y as u8, (x + y) as u8, 255];
    let right = run(Job::new(ImageOp::RotateRight, &source)).unwrap();
    let left = run(Job::new(ImageOp::RotateLeft, &source)).unwrap();
    assert_ne!(
        right.picture.with(<[u8]>::to_vec),
        source.with(<[u8]>::to_vec)
    );
    assert_ne!(
        left.picture.with(<[u8]>::to_vec),
        right.picture.with(<[u8]>::to_vec)
    );
    for y in 0..n {
        for x in 0..n {
            // Clockwise: the new (x, y) is the old (y, n-1-x).
            assert_eq!(pixel(&right, x, y), at(y, n - 1 - x), "right at {x},{y}");
            // Counter-clockwise: the new (x, y) is the old (n-1-y, x).
            assert_eq!(pixel(&left, x, y), at(n - 1 - y, x), "left at {x},{y}");
        }
    }
    // Right then left is the original again.
    let back = run(Job::new(ImageOp::RotateLeft, &right.picture)).unwrap();
    for y in 0..n {
        for x in 0..n {
            assert_eq!(pixel(&back, x, y), at(x, y));
        }
    }
}

#[test]
fn rotate_left_after_other_steps_rotates_and_is_labelled_so() {
    // Marcel's case: steps taken, then Rotate left: exactly that step on the copy shown.
    let mut picture = original(40, 20);
    for op in [ImageOp::Grayscale, ImageOp::FlipHorizontal] {
        picture = run(Job::new(op, &picture)).unwrap().picture;
    }
    let done = run(Job::new(ImageOp::RotateLeft, &picture)).unwrap();
    assert_eq!(done.label, "Rotated 90° left");
    // Turned a quarter, not mirrored: width and height swap.
    assert_eq!(size_of(&done), (20, 40));
}

#[test]
fn feather_radial_keeps_the_middle_and_clears_corners_and_edges() {
    let image = Rgba {
        width: 100,
        height: 100,
        pixels: zeroize::Zeroizing::new(vec![200; 100 * 100 * 4]),
    };
    let out = crate::image_edit::feather_radial(image, 0.25);
    let alpha = |x: usize, y: usize| out.pixels[(y * 100 + x) * 4 + 3];
    assert_eq!(alpha(50, 50), 200, "middle untouched");
    assert_eq!(alpha(50, 20), 200, "inside 75% of the radius");
    assert_eq!(alpha(0, 0), 0, "corner clear");
    assert!(alpha(50, 0) <= 2, "edge midpoint nearly clear");
    assert!(
        alpha(50, 8) > 0 && alpha(50, 8) < 200,
        "a soft fade between"
    );
    // Colour stays; only alpha fades.
    assert_eq!(out.pixels[0], 200);
}
