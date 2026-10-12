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

mod filter_tests {
    use super::super::filter::{LineFilter, filter, preview};

    #[test]
    fn keeps_matching_lines() {
        let f = LineFilter::new("ERROR", false, false).unwrap();
        assert_eq!(filter("a ERROR\nb ok\nc ERROR\n", &f), "a ERROR\nc ERROR\n");
        assert_eq!(filter("", &f), "");
        assert_eq!(filter("x\r\nERROR y\r\n", &f), "ERROR y\r\n");
    }

    #[test]
    fn invert_drops_matching_lines() {
        let f = LineFilter::new("ERROR", false, true).unwrap();
        assert_eq!(filter("a ERROR\nb ok\nc\n", &f), "b ok\nc\n");
        assert!(f.marks("b ok").is_empty());
    }

    #[test]
    fn ignore_case() {
        let f = LineFilter::new("error", true, false).unwrap();
        assert_eq!(filter("Error a\nb\nERROR c", &f), "Error a\nERROR c");
        let strict = LineFilter::new("error", false, false).unwrap();
        assert_eq!(filter("Error a\nb\n", &strict), "");
    }

    #[test]
    fn invalid_or_huge_patterns_are_errors() {
        assert!(LineFilter::new("(", false, false).is_err());
        assert!(LineFilter::new("[z-a]", false, false).is_err());
        let huge = LineFilter::new(r"\w{1000}{1000}", false, false);
        assert!(huge.is_err());
        assert!(!huge.unwrap_err().is_empty());
    }

    #[test]
    fn unicode_matches_and_marks() {
        let f = LineFilter::new("caf.", false, false).unwrap();
        assert_eq!(filter("café ☕\nthee\n", &f), "café ☕\n");
        let shown = preview("x\ncafé ☕ café\n", &f, 5);
        assert_eq!(shown.text, "café ☕ café");
        assert_eq!(shown.marks.len(), 2);
        assert_eq!(&shown.text[shown.marks[1].clone()], "café");
        assert_eq!(shown.kept, 1);
    }

    #[test]
    fn preview_counts_every_kept_line() {
        let f = LineFilter::new("a", false, false).unwrap();
        let shown = preview("a1\na2\na3\nb\n", &f, 2);
        assert_eq!(shown.text, "a1\na2");
        assert_eq!(shown.kept, 3);
    }
}

mod pager_tests {
    use super::super::pager::{LineIndex, page_after_resize, page_text, per_page};

    #[test]
    fn page_count_and_last_page() {
        let text: String = (1..=25).map(|i| format!("{i}\n")).collect();
        let index = LineIndex::new(&text);
        assert_eq!(index.lines(), 25);
        assert_eq!(index.pages(10), 3);
        assert_eq!(page_text(&text, index.page(0, 10)).lines().count(), 10);
        let last = page_text(&text, index.page(2, 10));
        assert_eq!(last, "21\n22\n23\n24\n25");
        // Past the end: the last page.
        assert_eq!(index.page(9, 10), index.page(2, 10));
        assert_eq!(index.percent(0, 10), 40);
        assert_eq!(index.percent(2, 10), 100);
    }

    #[test]
    fn empty_text_is_one_page() {
        let index = LineIndex::new("");
        assert_eq!(index.lines(), 0);
        assert_eq!(index.pages(10), 1);
        assert_eq!(page_text("", index.page(0, 10)), "");
        assert_eq!(index.percent(0, 10), 100);
    }

    #[test]
    fn crlf_and_no_final_newline() {
        let index = LineIndex::new("a\r\nb\r\nc");
        assert_eq!(index.lines(), 3);
        assert_eq!(page_text("a\r\nb\r\nc", index.page(0, 2)), "a\r\nb");
        assert_eq!(page_text("a\r\nb\r\nc", index.page(1, 2)), "c");
    }

    #[test]
    fn resize_keeps_the_first_line_in_view() {
        assert_eq!(per_page(300.0, 15.0), 20);
        assert_eq!(per_page(5.0, 15.0), 1);
        assert_eq!(per_page(100.0, 0.0), 1);
        // Page 3 of 20 lines starts at line 60: with 30 lines a page that is page 2.
        assert_eq!(page_after_resize(3, 20, 30), 2);
        assert_eq!(page_after_resize(0, 20, 7), 0);
    }

    #[test]
    fn long_texts_index_quickly() {
        let text: String = (0..100_000).map(|i| format!("line {i}\n")).collect();
        let start = std::time::Instant::now();
        let index = LineIndex::new(&text);
        assert_eq!(index.pages(40), 2500);
        assert_eq!(
            page_text(&text, index.page(2499, 40)).lines().last(),
            Some("line 99999")
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }
}
