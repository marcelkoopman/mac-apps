use super::*;

#[test]
fn plain_text_and_links_may_become_a_code() {
    assert!(offered("https://example.com/menu"));
    assert!(offered("Tafel 12"));
    assert!(!offered("   "));
    assert!(!offered(&"a".repeat(MAX_BYTES + 1)));
    assert!(offered(&"a".repeat(MAX_BYTES)));
}

#[test]
fn a_secret_never_becomes_a_code() {
    // Split, so secret scanners skip the test.
    let token = format!("ghp_{}", "1234567890abcdefghijklmnopqrstuvwxyzAB");
    assert!(!offered(&token));
    assert!(!offered(&format!("see {token} here")));
}

#[test]
fn the_modules_are_the_box_around_the_dark_pixels() {
    // A 2 × 2 code in a 5 × 4 picture with a white margin.
    let dark = [
        false, false, false, false, false, //
        false, true, false, false, false, //
        false, true, true, false, false, //
        false, false, false, false, false,
    ];
    let (square, side) = modules(&dark, 5, 4).unwrap();
    assert_eq!(side, 2);
    assert_eq!(square, vec![true, false, true, true]);
    assert_eq!(modules(&[false; 4], 2, 2), None);
    // Not square: 3 wide, 1 high.
    assert_eq!(modules(&[true, true, true], 3, 1), None);
    assert_eq!(modules(&[true], 2, 2), None);
}

#[test]
fn the_picture_is_large_black_on_white_with_a_quiet_zone() {
    // Version 1 is 21 modules: 1024 / 29 = 35 pixels each.
    let side = 21;
    let mut grid = vec![false; side * side];
    grid[0] = true;
    let image = picture(&grid, side).unwrap();
    let scale = module_pixels(side);
    assert_eq!(scale, 35);
    assert_eq!(image.width, (side + 2 * QUIET_ZONE) * scale);
    assert_eq!(image.height, image.width);
    let at = |x: usize, y: usize| {
        let i = (y * image.width + x) * 4;
        image.pixels[i..i + 4].to_vec()
    };
    let first = QUIET_ZONE * scale;
    assert_eq!(at(first, first), vec![0, 0, 0, 255]);
    assert_eq!(at(first + scale - 1, first + scale - 1), vec![0, 0, 0, 255]);
    assert_eq!(at(first + scale, first), vec![255, 255, 255, 255]);
    assert_eq!(at(first - 1, first), vec![255, 255, 255, 255]);
    assert_eq!(at(0, 0), vec![255, 255, 255, 255]);
    // The largest code has modules of five pixels.
    assert_eq!(module_pixels(177), 5);
    assert!(picture(&grid, 20).is_none());
}
