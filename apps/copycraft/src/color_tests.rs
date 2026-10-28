use super::{contrast_value, parse, picked_text};

fn hex_of(text: &str) -> String {
    parse(text).unwrap_or_else(|| panic!("{text}")).swatch.hex()
}

#[test]
fn every_css_form_is_read() {
    assert_eq!(hex_of("#36c"), "#3366CC");
    assert_eq!(hex_of("#3366cc"), "#3366CC");
    assert_eq!(hex_of(" #3366CC80 "), "#3366CC80");
    assert_eq!(hex_of("#36c8"), "#3366CC88");
    assert_eq!(hex_of("#000"), "#000000");
    assert_eq!(hex_of("\"#fff\""), "#FFFFFF");
    assert_eq!(hex_of("#ff0000;"), "#FF0000");
    assert_eq!(hex_of("rgb(51, 102, 204)"), "#3366CC");
    assert_eq!(hex_of("rgb(51 102 204 / 50%)"), "#3366CC80");
    assert_eq!(hex_of("rgba(51,102,204,0.5)"), "#3366CC80");
    assert_eq!(hex_of("rgb(20% 40% 80%)"), "#3366CC");
    assert_eq!(hex_of("hsl(220, 60%, 50%)"), "#3366CC");
    assert_eq!(hex_of("HSL(220deg 60% 50%)"), "#3366CC");
    assert_eq!(hex_of("hsla(0.5turn, 100%, 50%, 1)"), "#00FFFF");
    assert_eq!(hex_of("oklch(62.8% 0.2577 29.23)"), "#FF0000");
    assert_eq!(hex_of("oklch(1 0 0)"), "#FFFFFF");
    assert!(!parse("oklch(62.8% 0.2577 29.23)").unwrap().clipped);
}

#[test]
fn other_text_is_no_colour() {
    for text in [
        "#123",
        "#2024",
        "#12345",
        "#hashtag",
        "#bad1dea",
        "rgb(300, 0, 0)",
        "rgb(1, 2)",
        "rgb(1, 2, 3, 4, 5)",
        "rgb(1 2 3",
        "hsl(10, 200%, 50%)",
        "rgba(0,0,0,2)",
        "oklch(2 0.1 10)",
        "colour: #fff",
        "calc(1 2 3)",
        "",
        "#",
    ] {
        assert_eq!(parse(text), None, "{text}");
    }
}

#[test]
fn the_rows_show_every_notation() {
    crate::locale::set_test_language(crate::settings::Language::En);
    let parsed = parse("#3366CC").unwrap();
    let text = parsed.text();
    assert!(text.contains("HEX                #3366CC"), "{text}");
    assert!(
        text.contains("RGB                rgb(51 102 204)"),
        "{text}"
    );
    assert!(text.contains("hsl(220 60% 50%)"), "{text}");
    assert!(text.contains("oklch(53.2% 0.168 262.3)"), "{text}");
    assert!(
        text.contains("Color(red: 0.2, green: 0.4, blue: 0.8)"),
        "{text}"
    );
    assert!(
        text.contains("NSColor(srgbRed: 0.2, green: 0.4, blue: 0.8, alpha: 1)"),
        "{text}"
    );
    assert!(
        text.contains("Contrast on white  5.36:1  ·  AA ✓  ·  AAA ✗"),
        "{text}"
    );
    assert!(
        text.contains("Contrast on black  3.91:1  ·  AA ✗  ·  AAA ✗"),
        "{text}"
    );
    let half = parse("rgb(255 0 0 / 0.5)").unwrap().text();
    assert!(half.contains("#FF000080"), "{half}");
    assert!(half.contains("rgb(255 0 0 / 0.5)"), "{half}");
    assert!(
        half.contains("Color(red: 1, green: 0, blue: 0, opacity: 0.5)"),
        "{half}"
    );
    // Outside sRGB: clipped, and the card says so.
    let wide = parse("oklch(70% 0.4 145)").unwrap();
    assert!(wide.clipped);
    assert!(wide.text().ends_with(crate::locale::t("color_clipped")));
    crate::locale::set_test_language(crate::settings::Language::Nl);
    assert!(parse("#000").unwrap().text().contains("Contrast met wit"));
    crate::locale::set_test_language(crate::settings::Language::System);
}

#[test]
fn contrast_follows_wcag() {
    let black = parse("#000").unwrap().swatch;
    assert!((black.contrast([1.0; 3]) - 21.0).abs() < 1e-9);
    assert!((black.contrast([0.0; 3]) - 1.0).abs() < 1e-9);
    // #767676 is the lightest grey with AA on white.
    assert!(parse("#767676").unwrap().swatch.contrast([1.0; 3]) >= 4.5);
    assert!(parse("#777777").unwrap().swatch.contrast([1.0; 3]) < 4.5);
    assert_eq!(contrast_value(7.0), "7:1  ·  AA ✓  ·  AAA ✓");
    assert_eq!(contrast_value(4.499), "4.49:1  ·  AA ✗  ·  AAA ✗");
    // A see-through colour is laid over the background first.
    let clear = parse("#00000000").unwrap().swatch;
    assert!((clear.contrast([1.0; 3]) - 1.0).abs() < 1e-9);
}

#[test]
fn a_picked_colour_is_hex() {
    assert_eq!(picked_text(0.2, 0.4, 0.8, 1.0), "#3366CC");
    assert_eq!(picked_text(1.0, 1.0, 1.0, 0.5), "#FFFFFF80");
}
