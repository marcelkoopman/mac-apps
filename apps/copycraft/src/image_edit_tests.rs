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

fn shown_size(versions: &ImageVersions) -> (usize, usize) {
    let facts = &versions.shown().unwrap().facts;
    (facts.width, facts.height)
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
            "Rotate 90° left",
            "Rotate 90° right",
            "Flip horizontal",
            "Flip vertical",
            "Remove metadata",
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
fn versions_replay_steps_on_the_original() {
    let original = original(40, 20);
    let mut versions = ImageVersions::default();
    assert_eq!(versions.labels(), ["Original"]);
    let job = versions.push(ImageOp::RotateRight, &original).unwrap();
    assert!(versions.finish(run(job).unwrap()));
    let job = versions
        .push(ImageOp::Resize(Resize::Percent(50)), &original)
        .unwrap();
    assert!(versions.finish(run(job).unwrap()));
    assert_eq!(
        versions.labels(),
        ["Original", "Rotated 90° right", "Resized to 10×20"]
    );
    assert_eq!(versions.cursor(), 2);
    assert_eq!(shown_size(&versions), (10, 20));
    assert_eq!(versions.shown().unwrap().facts.format, "PNG");

    // Undo to a kept version and to the original is at once.
    assert!(versions.goto(1, &original).is_none());
    assert_eq!(shown_size(&versions), (20, 40));
    assert!(versions.goto(0, &original).is_none());
    assert!(versions.shown().is_none());

    // Without the pictures, a version is worked out again.
    versions.forget_pictures();
    let job = versions.goto(2, &original).expect("replayed");
    assert_eq!(versions.cursor(), 0, "shown when done");
    assert!(versions.finish(run(job).unwrap()));
    assert_eq!(shown_size(&versions), (10, 20));
    assert_eq!(versions.labels().len(), 3);
}

#[test]
fn a_step_after_undo_drops_the_later_versions() {
    let original = original(40, 20);
    let mut versions = ImageVersions::default();
    for op in [ImageOp::FlipHorizontal, ImageOp::Grayscale] {
        let job = versions.push(op, &original).unwrap();
        assert!(versions.finish(run(job).unwrap()));
    }
    assert!(versions.goto(1, &original).is_none());
    let job = versions.push(ImageOp::RotateLeft, &original).unwrap();
    assert!(versions.finish(run(job).unwrap()));
    assert_eq!(
        versions.labels(),
        ["Original", "Flipped horizontally", "Rotated 90° left"]
    );
    assert_eq!(versions.cursor(), 2);
    assert_eq!(shown_size(&versions), (20, 40));
}

#[test]
fn a_step_that_changes_nothing_makes_no_version() {
    let original = original(40, 20);
    let mut versions = ImageVersions::default();
    let job = versions
        .push(ImageOp::Resize(Resize::Longest(2048)), &original)
        .unwrap();
    assert_eq!(
        run(job).err(),
        Some(ImageError::Unchanged("Already that size or smaller"))
    );
    assert_eq!(versions.labels(), ["Original"]);
}

#[test]
fn stale_and_cancelled_jobs_are_dropped() {
    let original = original(40, 20);
    let mut versions = ImageVersions::default();
    let old = versions.push(ImageOp::Grayscale, &original).unwrap();
    let new = versions.push(ImageOp::FlipVertical, &original).unwrap();
    assert!(!versions.awaits(old.generation));
    assert!(versions.awaits(new.generation));
    assert!(!versions.finish(run(old).unwrap()));
    assert_eq!(versions.labels(), ["Original"]);
    assert!(versions.finish(run(new).unwrap()));
    assert_eq!(versions.labels(), ["Original", "Flipped vertically"]);

    let job = versions.push(ImageOp::Grayscale, &original).unwrap();
    let cancel = AtomicBool::new(true);
    assert_eq!(
        job.run(&cancel, &PngCodec).err(),
        Some(ImageError::Cancelled)
    );

    // Forgetting the pictures makes a pending job stale.
    let job = versions.push(ImageOp::Grayscale, &original).unwrap();
    versions.forget_pictures();
    assert!(!versions.finish(run(job).unwrap()));
}

#[test]
fn versions_are_capped_and_few_pictures_are_kept() {
    let original = original(8, 4);
    let mut versions = ImageVersions::default();
    for _ in 1..MAX_VERSIONS {
        let job = versions.push(ImageOp::FlipVertical, &original).unwrap();
        assert!(versions.finish(run(job).unwrap()));
    }
    assert_eq!(versions.len(), MAX_VERSIONS);
    assert!(matches!(
        versions.push(ImageOp::Grayscale, &original),
        Err(ImageError::Full)
    ));
    assert_eq!(versions.kept.len(), 3);
    // An old version is worked out again; a copy keeps only the steps.
    assert!(versions.goto(2, &original).is_some());
    let copy = versions.clone();
    assert_eq!(copy.labels(), versions.labels());
    assert!(copy.kept.is_empty());
}

#[test]
fn an_unreadable_picture_fails_the_job() {
    let original = SecretBytes::new(b"not a picture".to_vec()).unwrap();
    let mut versions = ImageVersions::default();
    let job = versions.push(ImageOp::Grayscale, &original).unwrap();
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
fn a_scan_is_kept_with_its_version() {
    let original = original(40, 20);
    let mut versions = ImageVersions::default();
    let job = versions.push(ImageOp::Grayscale, &original).unwrap();
    let done = run(job).unwrap();
    let picture = done.picture();
    assert!(versions.finish(done));
    assert!(versions.shown().unwrap().scan.is_none());
    let scan = ImageScan {
        info: "Image".to_string(),
        data_url: None,
        ocr: Some("text".to_string()),
        qr: None,
    };
    assert!(versions.remember_scan(&picture, &scan));
    assert_eq!(versions.shown().unwrap().scan.as_ref(), Some(&scan));
    assert!(!versions.remember_scan(&original, &scan));
    versions.forget_pictures();
    assert!(!versions.remember_scan(&picture, &scan));
}
