use super::*;

#[test]
fn count_empty_trailing_crlf_and_unicode() {
    assert_eq!(
        count(""),
        Counts {
            lines: 0,
            words: 0,
            chars: 0
        }
    );
    assert_eq!(
        count("a b\nc\n"),
        Counts {
            lines: 2,
            words: 3,
            chars: 6
        }
    );
    assert_eq!(
        count("a b\nc"),
        Counts {
            lines: 2,
            words: 3,
            chars: 5
        }
    );
    assert_eq!(
        count("a\r\nb\r\n"),
        Counts {
            lines: 2,
            words: 2,
            chars: 6
        }
    );
    assert_eq!(
        count("héllo wörld 🎉\n"),
        Counts {
            lines: 1,
            words: 3,
            chars: 14
        }
    );
}

#[test]
fn head_keeps_line_endings() {
    assert_eq!(head("", 3), "");
    assert_eq!(head("a\nb\nc\n", 2), "a\nb\n");
    assert_eq!(head("a\nb\nc", 3), "a\nb\nc");
    assert_eq!(head("a\r\nb\r\nc\r\n", 1), "a\r\n");
    assert_eq!(head("ä\nö\n", 1), "ä\n");
    assert_eq!(head("a\nb\n", 10), "a\nb\n");
    assert_eq!(head("a\nb\n", 0), "");
}

#[test]
fn tail_keeps_line_endings() {
    assert_eq!(tail("", 3), "");
    assert_eq!(tail("a\nb\nc\n", 2), "b\nc\n");
    assert_eq!(tail("a\nb\nc", 2), "b\nc");
    assert_eq!(tail("a\r\nb\r\nc\r\n", 1), "c\r\n");
    assert_eq!(tail("ä\nö\n", 1), "ö\n");
    assert_eq!(tail("a\nb\n", 10), "a\nb\n");
    assert_eq!(tail("a\nb\n", 0), "");
}

#[test]
fn reverse_keeps_the_ending() {
    assert_eq!(reverse(""), "");
    assert_eq!(reverse("a\nb\nc\n"), "c\nb\na\n");
    assert_eq!(reverse("a\nb\nc"), "c\nb\na");
    assert_eq!(reverse("a\r\nb\r\n"), "b\r\na\r\n");
    assert_eq!(reverse("ä\n😀\n"), "😀\nä\n");
    assert_eq!(reverse("one\n"), "one\n");
}

#[test]
fn column_skips_lines_without_the_field() {
    assert_eq!(column("", Separator::Whitespace, 1), "");
    assert_eq!(column("a 1\nb  2\nc\n", Separator::Whitespace, 2), "1\n2\n");
    assert_eq!(column("a,1\r\nb,2\r\n", Separator::Comma, 2), "1\r\n2\r\n");
    assert_eq!(column("a|é\nb|ü", Separator::Pipe, 2), "é\nü");
    assert_eq!(column("a\tb\n", Separator::Tab, 5), "");
    assert_eq!(column("a b\n", Separator::Whitespace, 0), "");
}

#[test]
fn column_preview_takes_the_first_lines() {
    let text = "x 1\ny 2\nz 3\n";
    assert_eq!(column_preview(text, Separator::Whitespace, 2, 2), "1\n2");
}

#[test]
fn separator_detection() {
    assert_eq!(detect_separator(""), Separator::Whitespace);
    assert_eq!(detect_separator("a\tb\nc\td\n"), Separator::Tab);
    assert_eq!(detect_separator("a,b,c\r\nd,e,f\r\n"), Separator::Comma);
    assert_eq!(detect_separator("a;b\nc;d\n"), Separator::Semicolon);
    assert_eq!(detect_separator("a|b\nc|d\n"), Separator::Pipe);
    // Commas in prose are not columns: the count differs per line.
    assert_eq!(
        detect_separator("one, two\nthree four\n"),
        Separator::Whitespace
    );
    assert_eq!(
        detect_separator("2026-10-12 INFO start\n2026-10-12 WARN slow\n"),
        Separator::Whitespace
    );
    // The most frequent consistent separator wins.
    assert_eq!(detect_separator("a,b;c,d\ne,f;g,h\n"), Separator::Comma);
}

#[test]
fn apply_leaves_nothing_as_none() {
    assert_eq!(TextOp::Head(2).apply(""), None);
    assert_eq!(TextOp::Column(Separator::Comma, 3).apply("a,b\n"), None);
    assert_eq!(TextOp::Reverse.apply("a\nb\n").as_deref(), Some("b\na\n"));
    assert_eq!(TextOp::Tail(1).apply("a\nb\n").as_deref(), Some("b\n"));
}

#[test]
fn long_texts_stay_quick() {
    let text: String = (0..100_000).map(|i| format!("{i} field{i} x\n")).collect();
    let start = std::time::Instant::now();
    assert_eq!(count(&text).lines, 100_000);
    assert_eq!(head(&text, 5).lines().count(), 5);
    assert_eq!(tail(&text, 5).lines().count(), 5);
    assert_eq!(reverse(&text).lines().next(), Some("99999 field99999 x"));
    assert_eq!(
        column(&text, Separator::Whitespace, 2).lines().count(),
        100_000
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn parse_count_takes_whole_numbers_from_one() {
    assert_eq!(parse_count(" 5 "), Some(5));
    assert_eq!(parse_count("0"), None);
    assert_eq!(parse_count("x"), None);
}
