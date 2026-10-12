use super::*;

#[test]
fn pasted_size_uses_kilobytes_when_small() {
    assert_eq!(crate::commands::format_bytes(0), "0 KB");
    assert_eq!(crate::commands::format_bytes(12), "0.012 KB");
    assert_eq!(crate::commands::format_bytes(1_500), "1.5 KB");
    assert_eq!(crate::commands::format_bytes(9_000), "9.0 KB");
    assert_eq!(crate::commands::format_bytes(184_000), "0.18 MB");
    assert_eq!(crate::commands::format_bytes(2_000_000), "2.00 MB");
    assert_eq!(crate::commands::format_bytes(15_500_000), "15.5 MB");
    assert_eq!(crate::commands::format_bytes(250_000_000), "250 MB");
    let card = work_card(&data(SubjectKind::Text, Some("hello")));
    assert_eq!(card.meta, "0.005 KB");
}

#[test]
fn a_long_image_name_keeps_its_dimensions() {
    fn width(text: &str) -> f64 {
        text.chars().count() as f64
    }
    let tail = "HEIC  4032×3024  2.00 MB";
    let line = format!("{}  ·  {tail}", "n".repeat(80));
    assert_eq!(crate::commands::fit_leading_name(&line, 200.0, width), line);
    let fitted = crate::commands::fit_leading_name(&line, 40.0, width);
    assert!(fitted.contains("4032×3024"), "{fitted}");
    assert!(fitted.contains('…'), "{fitted}");
    assert!(!fitted.contains('\n'), "{fitted}");
    assert!(width(&fitted) <= 40.0, "{fitted}");
    let tight = crate::commands::fit_leading_name(&line, 10.0, width);
    assert!(!tight.contains('\n'), "{tight}");
    assert!(tight.contains('…'), "{tight}");
    assert!(width(&tight) <= 10.0, "{tight}");
}
