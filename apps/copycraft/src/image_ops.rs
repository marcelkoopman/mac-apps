use image::ExtendedColorType;
use image::ImageEncoder;
use image::codecs::jpeg::JpegEncoder;

use crate::clipboard::ClipboardImage;

pub const JPEG_QUALITY: u8 = 80;

/// Longest edge of the on-screen picture. The preview is only an indication
/// of the clipboard image, so it stays small enough to decode and draw quickly.
pub const PREVIEW_MAX_EDGE: usize = 640;

pub fn info_with_sizes(
    image: &ClipboardImage,
    png_len: Option<usize>,
    jpeg_len: Option<usize>,
) -> String {
    let (width, height) = display_size(image);
    let mut lines = vec![
        crate::locale::t("title_image").to_string(),
        crate::locale::tf("image_size_line", &[&width, &height]),
        crate::locale::tf("image_aspect_line", &[&aspect_ratio(width, height)]),
        crate::locale::tf("image_pixels_line", &[&width.saturating_mul(height)]),
    ];
    if let Some(len) = png_len {
        lines.push(format!("PNG: {}", format_bytes(len)));
    }
    if let Some(len) = jpeg_len {
        lines.push(format!("JPEG: {}", format_bytes(len)));
    }
    lines.join("\n")
}

pub fn jpeg_bytes(image: &ClipboardImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = rgba_to_rgb(&image.rgba);
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .write_image(
            &rgb,
            image.width as u32,
            image.height as u32,
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

pub fn image_from_encoded(bytes: &[u8]) -> Option<ClipboardImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    ClipboardImage::new(img.width() as usize, img.height() as usize, img.into_raw())
}

pub fn preview_from_encoded(bytes: &[u8]) -> Option<ClipboardImage> {
    let image = image_from_encoded(bytes)?;
    Some(downscale_for_preview(image, PREVIEW_MAX_EDGE))
}

pub fn downscale_for_preview(image: ClipboardImage, max_edge: usize) -> ClipboardImage {
    let max_edge = max_edge.max(1);
    let (tw, th) = fitted_size(image.width, image.height, max_edge);
    if tw == 0 || th == 0 || (tw == image.width && th == image.height) {
        return image;
    }
    let mut out = vec![0u8; tw * th * 4];
    for y in 0..th {
        let y0 = y * image.height / th;
        let y1 = ((y + 1) * image.height / th).max(y0 + 1).min(image.height);
        for x in 0..tw {
            let x0 = x * image.width / tw;
            let x1 = ((x + 1) * image.width / tw).max(x0 + 1).min(image.width);
            let mut acc = [0u32; 4];
            let mut count = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * image.width + sx) * 4;
                    acc[0] += u32::from(image.rgba[i]);
                    acc[1] += u32::from(image.rgba[i + 1]);
                    acc[2] += u32::from(image.rgba[i + 2]);
                    acc[3] += u32::from(image.rgba[i + 3]);
                    count += 1;
                }
            }
            let d = (y * tw + x) * 4;
            let count = count.max(1);
            out[d] = (acc[0] / count) as u8;
            out[d + 1] = (acc[1] / count) as u8;
            out[d + 2] = (acc[2] / count) as u8;
            out[d + 3] = (acc[3] / count) as u8;
        }
    }
    let full_width = image.full_width;
    let full_height = image.full_height;
    match ClipboardImage::new(tw, th, out) {
        Some(mut preview) => {
            preview.full_width = full_width;
            preview.full_height = full_height;
            preview
        }
        None => image,
    }
}

fn display_size(image: &ClipboardImage) -> (usize, usize) {
    if image.full_width == 0 || image.full_height == 0 {
        (image.width, image.height)
    } else {
        (image.full_width, image.full_height)
    }
}

fn fitted_size(width: usize, height: usize, max_edge: usize) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    let long = width.max(height);
    if long <= max_edge {
        return (width, height);
    }
    if width >= height {
        let tw = max_edge;
        let th = (height * max_edge / width).max(1);
        (tw, th)
    } else {
        let th = max_edge;
        let tw = (width * max_edge / height).max(1);
        (tw, th)
    }
}

fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for px in rgba.as_chunks::<4>().0 {
        let a = u16::from(px[3]);
        let blend = |c: u8| ((u16::from(c) * a + 255 * (255 - a)) / 255) as u8;
        rgb.push(blend(px[0]));
        rgb.push(blend(px[1]));
        rgb.push(blend(px[2]));
    }
    rgb
}

fn aspect_ratio(width: usize, height: usize) -> String {
    let g = gcd(width, height);
    format!("{}:{}", width / g, height / g)
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a.max(1)
}

fn format_bytes(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1} MB", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1} KB", n as f64 / 1_000.0)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        aspect_ratio, downscale_for_preview, format_bytes, image_from_encoded, info_with_sizes,
        jpeg_bytes,
    };
    use crate::clipboard::ClipboardImage;

    fn sample() -> ClipboardImage {
        let mut rgba = Vec::with_capacity(64 * 64 * 4);
        for y in 0u8..64 {
            for x in 0u8..64 {
                let n = x.wrapping_mul(37) ^ y.wrapping_mul(91);
                rgba.extend_from_slice(&[n, n.wrapping_add(x), n.wrapping_add(y), 255]);
            }
        }
        ClipboardImage::new(64, 64, rgba).expect("rgba")
    }

    #[test]
    fn info_includes_dimensions_and_sizes() {
        let image = sample();
        let png_len = image.png_bytes().ok().map(|bytes| bytes.len());
        let jpeg_len = jpeg_bytes(&image, 80).ok().map(|bytes| bytes.len());
        let text = info_with_sizes(&image, png_len, jpeg_len);
        assert!(text.contains("64×64"));
        assert!(text.contains("1:1"));
        assert!(text.contains("PNG:"));
        assert!(text.contains("JPEG:"));
        assert!(text.contains("Pixels: 4096"));
    }

    #[test]
    fn jpeg_roundtrips() {
        let image = sample();
        let jpeg = jpeg_bytes(&image, 80).expect("jpeg");
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
        let decoded = image_from_encoded(&jpeg).expect("decode");
        assert_eq!(decoded.width, 64);
        assert_eq!(decoded.height, 64);
    }

    #[test]
    fn noisy_image_jpeg_is_smaller_than_png() {
        let image = sample();
        let png = image.png_bytes().expect("png");
        let jpeg = jpeg_bytes(&image, 80).expect("jpeg");
        assert!(jpeg.len() < png.len());
    }

    #[test]
    fn downscale_keeps_full_size_and_shrinks_pixels() {
        let rgba = [10, 20, 30, 255].repeat(8);
        let image = ClipboardImage::new(4, 2, rgba).expect("rgba");
        let preview = downscale_for_preview(image, 2);
        assert_eq!((preview.width, preview.height), (2, 1));
        assert_eq!((preview.full_width, preview.full_height), (4, 2));
        assert!(preview.rgba.chunks(4).all(|px| px == [10, 20, 30, 255]));
        let info = info_with_sizes(&preview, None, None);
        assert!(info.contains("4×2"));
        assert!(info.contains("Pixels: 8"));
        assert!(!info.contains("2×1"));
    }

    #[test]
    fn downscale_leaves_small_images_alone() {
        let image = ClipboardImage::new(2, 2, vec![7; 16]).expect("rgba");
        let preview = downscale_for_preview(image.clone(), 640);
        assert_eq!(preview, image);
    }

    #[test]
    fn format_bytes_uses_units() {
        assert_eq!(format_bytes(12), "12 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
        assert_eq!(format_bytes(2_000_000), "2.0 MB");
        assert_eq!(aspect_ratio(1920, 1080), "16:9");
    }
}
