use super::{Band, FnScore, Lang, analyze, total};

fn scores(text: &str, lang: Lang) -> Vec<(String, u32)> {
    analyze(text, lang)
        .into_iter()
        .map(|f| (f.name, f.score))
        .collect()
}

fn one(text: &str, lang: Lang) -> u32 {
    let all = analyze(text, lang);
    assert_eq!(all.len(), 1, "{all:?}");
    all[0].score
}

// SonarSource whitepaper, "Cognitive Complexity", examples.
const JAVA_SUM_OF_PRIMES: &str = "int sumOfPrimes(int max) {
    int total = 0;
    OUT: for (int i = 1; i <= max; ++i) {
        for (int j = 2; j < i; ++j) {
            if (i % j == 0) {
                continue OUT;
            }
        }
        total += i;
    }
    return total;
}
";

const JAVA_GET_WORDS: &str = "String getWords(int number) {
    switch (number) {
      case 1:
        return \"one\";
      case 2:
        return \"a couple\";
      case 3:
        return \"a few\";
      default:
        return \"lots\";
    }
}
";

#[test]
fn java_whitepaper_examples() {
    assert_eq!(one(JAVA_SUM_OF_PRIMES, Lang::Java), 7);
    assert_eq!(one(JAVA_GET_WORDS, Lang::Java), 1);
    let f = &analyze(JAVA_SUM_OF_PRIMES, Lang::Java)[0];
    assert_eq!(
        (f.name.as_str(), f.first_line, f.last_line),
        ("sumOfPrimes", 1, 12)
    );
}

#[test]
fn rust_whitepaper_examples() {
    let sum = "fn sum_of_primes(max: u32) -> u32 {
    let mut total = 0;
    'out: for i in 1..=max {
        for j in 2..i {
            if i % j == 0 {
                continue 'out;
            }
        }
        total += i;
    }
    total
}
";
    assert_eq!(one(sum, Lang::Rust), 7);
    let words = "fn get_words(n: u32) -> &'static str {
    match n {
        1 => \"one\",
        2 => \"a couple\",
        3 => \"a few\",
        _ => \"lots\",
    }
}
";
    assert_eq!(one(words, Lang::Rust), 1);
}

#[test]
fn python_whitepaper_examples() {
    // No labelled continue in Python: 1 + 2 + 3.
    let sum = "def sum_of_primes(max):
    total = 0
    for i in range(1, max + 1):
        for j in range(2, i):
            if i % j == 0:
                break
        total += i
    return total
";
    assert_eq!(one(sum, Lang::Python), 6);
    let words = "def get_words(number):
    match number:
        case 1:
            return \"one\"
        case _:
            return \"lots\"
";
    assert_eq!(one(words, Lang::Python), 1);
    let chain = "def kind(n):\n    if n == 1:\n        return 'a'\n    elif n == 2:\n        return 'b'\n    else:\n        return 'c'\n";
    assert_eq!(one(chain, Lang::Python), 3);
}

#[test]
fn else_if_chains_cost_one_each() {
    let java = "void m(int a) { if (a == 1) { x(); } else if (a == 2) { y(); } else { z(); } }";
    assert_eq!(one(java, Lang::Java), 3);
    let rust = "fn m(a: i32) { if a == 1 { x() } else if a == 2 { y() } else { z() } }";
    assert_eq!(one(rust, Lang::Rust), 3);
}

#[test]
fn boolean_sequences_count_per_change_of_operator() {
    assert_eq!(one("void m() { if (a && b && c || d) {} }", Lang::Java), 3);
    assert_eq!(one("fn m() { if a && b || c && d {} }", Lang::Rust), 4);
    assert_eq!(
        one(
            "def m():\n    if a and b or c:\n        pass\n",
            Lang::Python
        ),
        3
    );
    // `&&x` is a reference, not an operator.
    assert_eq!(one("fn m() { f(&&x); }", Lang::Rust), 0);
}

#[test]
fn lambdas_and_closures_nest() {
    let java = "void run() {\n  list.forEach(x -> {\n    if (x > 0) { go(); }\n  });\n}\n";
    assert_eq!(one(java, Lang::Java), 2);
    let java_expr = "void run() { list.map(x -> x > 0 ? 1 : 2); }";
    assert_eq!(one(java_expr, Lang::Java), 2);
    let rust = "fn run(v: &[i32]) {\n    v.iter().for_each(|x| {\n        if *x > 0 { go() }\n    });\n}\n";
    assert_eq!(one(rust, Lang::Rust), 2);
    let rust_move = "fn run() { spawn(move || { loop { } }); }";
    assert_eq!(one(rust_move, Lang::Rust), 2);
    let python = "def run(xs):\n    f = lambda x: 1 if x else 0\n    return f\n";
    assert_eq!(one(python, Lang::Python), 2);
}

#[test]
fn recursion_counts() {
    let java = "int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }";
    assert_eq!(one(java, Lang::Java), 3);
    let rust = "fn fact(n: u64) -> u64 { if n < 2 { 1 } else { n * fact(n - 1) } }";
    assert_eq!(one(rust, Lang::Rust), 3);
    let python = "def fact(n):\n    if n < 2:\n        return 1\n    return n * fact(n - 1)\n";
    assert_eq!(one(python, Lang::Python), 2);
    // A call on another object is not recursion.
    assert_eq!(one("void size() { other.size(); }", Lang::Java), 0);
}

#[test]
fn java_statements_that_are_not_methods() {
    let src = "class A {
    A() { super(); }
    void m() {
        do { step(); } while (more());
        switch (k) { case 1 -> a(); default -> b(); }
        try { a(); } catch (Exception e) { log(); }
        List<?> l = new ArrayList<>();
        synchronized (this) { }
    }
}
";
    // do 1, switch 1, catch 1.
    let all = scores(src, Lang::Java);
    assert!(all.contains(&("m".into(), 3)), "{all:?}");
    assert!(all.contains(&("A".into(), 0)), "{all:?}");
}

#[test]
fn keywords_in_strings_and_comments_do_not_count() {
    let java = "void m() {\n  String s = \"if while for\"; // if if\n  /* for (;;) { if } */\n  String t = \"\"\"\n    if (x) { while }\n    \"\"\";\n  char c = '{';\n}\n";
    assert_eq!(one(java, Lang::Java), 0);
    let rust = "fn m() {\n    let s = \"if {\"; // if\n    /* if /* nested { */ while */\n    let r = r#\"for \" { \"#;\n    let c = '{';\n    let l: &'static str = \"\";\n}\n";
    assert_eq!(one(rust, Lang::Rust), 0);
    let python = "def m():\n    s = 'if x'  # if for\n    t = \"\"\"\nif a:\n    while b:\n\"\"\"\n    return s\n";
    assert_eq!(one(python, Lang::Python), 0);
}

#[test]
fn empty_input_and_no_functions() {
    for lang in [Lang::Rust, Lang::Java, Lang::Python] {
        assert!(analyze("", lang).is_empty());
    }
    assert!(analyze("let x = 1;\nstruct A { b: u8 }\n", Lang::Rust).is_empty());
    assert!(analyze("x = 1\nif x:\n    print(x)\n", Lang::Python).is_empty());
    assert!(analyze("interface I { void a(); }", Lang::Java).is_empty());
    assert_eq!(total(&[]), (0, 0.0));
}

#[test]
fn python_methods_and_nested_functions_are_separate() {
    let src = "class A:\n    def a(self):\n        if x:\n            pass\n\n    async def b(self):\n        def inner():\n            for i in y:\n                pass\n        return inner\n";
    let mut all = scores(src, Lang::Python);
    all.sort();
    assert_eq!(
        all,
        vec![("a".into(), 1), ("b".into(), 0), ("inner".into(), 1)]
    );
}

#[test]
fn sorted_highest_first_with_bands_and_totals() {
    let src = "fn a() {}\nfn b() { if x { if y { if z {} } } }\nfn c() { if x {} }\n";
    let all = analyze(src, Lang::Rust);
    let names: Vec<&str> = all.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["b", "c", "a"]);
    assert_eq!(all[0].score, 6);
    assert_eq!(total(&all), (7, 2.3));
    assert_eq!(Band::of(15), Band::Normal);
    assert_eq!(Band::of(16), Band::High);
    assert_eq!(Band::of(25), Band::High);
    assert_eq!(Band::of(26), Band::VeryHigh);
    let _: Option<FnScore> = None;
}

#[test]
fn a_hundred_thousand_lines_stays_quick() {
    let body =
        "fn f(a: i32) -> i32 {\n    if a > 0 && a < 9 {\n        return a;\n    }\n    0\n}\n";
    let text = body.repeat(100_000 / 6);
    let started = std::time::Instant::now();
    let all = analyze(&text, Lang::Rust);
    assert_eq!(all.len(), 100_000 / 6);
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}

#[test]
fn line_offsets_count_utf16_units() {
    use super::line_offset_utf16;
    let text = "a\né😀\nc";
    assert_eq!(line_offset_utf16(text, 1), 0);
    assert_eq!(line_offset_utf16(text, 2), 2);
    assert_eq!(line_offset_utf16(text, 3), 6);
    assert_eq!(line_offset_utf16(text, 99), 7);
}

#[test]
fn menu_rows_say_the_band_in_words_and_go_to_the_first_line() {
    use crate::commands::{CommandId, complexity_footer, complexity_rows};
    let scores = vec![
        FnScore {
            name: "big".into(),
            first_line: 10,
            last_line: 90,
            score: 30,
        },
        FnScore {
            name: "mid".into(),
            first_line: 3,
            last_line: 8,
            score: 20,
        },
        FnScore {
            name: "ok".into(),
            first_line: 1,
            last_line: 2,
            score: 1,
        },
    ];
    let rows = complexity_rows(&scores);
    assert_eq!(rows[0].0.id, CommandId::GotoLine(10));
    assert_eq!(rows[0].1, Band::VeryHigh);
    assert!(
        rows[0].0.title.starts_with("30  big  ·  "),
        "{}",
        rows[0].0.title
    );
    assert!(
        rows[0]
            .0
            .title
            .ends_with(crate::locale::t("complexity_very_high"))
    );
    assert!(
        rows[1]
            .0
            .title
            .ends_with(crate::locale::t("complexity_high"))
    );
    assert!(
        !rows[2]
            .0
            .title
            .contains(crate::locale::t("complexity_high"))
    );
    let footer = complexity_footer(&scores);
    assert_eq!(footer.len(), 1);
    assert!(
        footer[0].contains("51") && footer[0].contains("17"),
        "{footer:?}"
    );
    assert_eq!(
        complexity_footer(&[]),
        vec![crate::locale::t("complexity_none").to_string()]
    );
    let many: Vec<FnScore> = (0..310)
        .map(|i| FnScore {
            name: format!("f{i}"),
            first_line: i,
            last_line: i,
            score: 1,
        })
        .collect();
    assert_eq!(
        complexity_rows(&many).len(),
        crate::commands::COMPLEXITY_ROWS
    );
    assert_eq!(complexity_footer(&many).len(), 2);
}

#[test]
fn lines_menu_offers_complexity_only_on_code() {
    use crate::commands::{CommandId, TextShown, text_menu_items};
    let mut edit = TextShown {
        labels: vec!["Original".into()],
        version: 0,
        note: None,
        code: false,
        agent: false,
        tracking: false,
    };
    let has = |edit: &TextShown| {
        text_menu_items(edit)
            .iter()
            .any(|(c, _)| c.id == CommandId::TextComplexity)
    };
    assert!(!has(&edit));
    edit.code = true;
    assert!(has(&edit));
}

#[test]
fn texts_over_the_limit_are_refused_not_scored() {
    use super::{MAX_BYTES, MAX_LINES, Refused, analyze_guarded};
    let small = "fn a() { if x {} }\n";
    assert_eq!(analyze_guarded(small, Lang::Rust).map(|s| s.len()), Ok(1));
    let many_lines = "x\n".repeat(MAX_LINES + 1);
    assert_eq!(
        analyze_guarded(&many_lines, Lang::Python),
        Err(Refused::TooLarge)
    );
    let wide = "a".repeat(MAX_BYTES + 1);
    assert_eq!(analyze_guarded(&wide, Lang::Java), Err(Refused::TooLarge));
    assert_eq!(
        crate::commands::complexity_refused(Refused::TooLarge),
        crate::locale::t("complexity_too_large")
    );
    let event = crate::diag::LogEvent::ComplexityTooLarge {
        lang: Lang::Java,
        bytes: 9,
    };
    assert_eq!(event.line(), "complexity: Java too large (9 bytes)");
}

/// xorshift64*: a small, seeded generator, so a failure repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn random_and_malformed_input_never_panics_or_hangs() {
    const PIECES: &[&str] = &[
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        "\"",
        "'",
        "\"\"\"",
        "'''",
        "/*",
        "*/",
        "//",
        "#",
        "\n",
        "    ",
        "\t",
        "if ",
        "else ",
        "for ",
        "while ",
        "loop ",
        "match ",
        "switch ",
        "case ",
        "default ",
        "catch ",
        "try ",
        "do ",
        "fn ",
        "def ",
        "lambda ",
        "class ",
        "void ",
        "public ",
        "int ",
        "x",
        "f(",
        "&&",
        "||",
        " and ",
        " or ",
        "?",
        ":",
        ";",
        ",",
        "->",
        "=>",
        "|",
        "'a",
        "r#\"",
        "\"#",
        "b'",
        "\\",
        "break ",
        "continue ",
        "goto ",
        "é",
        "😀",
        "async ",
        "return ",
        "new ",
        "::",
        ".",
        "=",
        "<",
        ">",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let started = std::time::Instant::now();
    let mut inputs: Vec<String> = vec![
        String::new(),
        "{".repeat(5000),
        "}".repeat(5000),
        "(".repeat(5000),
        "\"never closed".into(),
        "/* never closed".into(),
        "'''never closed".into(),
        "\"\"\"never closed".into(),
        "fn".into(),
        "def".into(),
        "fn f(".into(),
        "if (".repeat(2000),
        "def f():\n if x:\n".to_string()
            + &(1..400)
                .map(|d| format!("{}if x:\n", " ".repeat(d)))
                .collect::<String>(),
    ];
    for _ in 0..3000 {
        let len = rng.below(200);
        let text: String = (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect();
        inputs.push(text);
    }
    for text in &inputs {
        for lang in [Lang::Rust, Lang::Java, Lang::Python] {
            let result = std::panic::catch_unwind(|| analyze(text, lang));
            assert!(result.is_ok(), "{lang:?} panicked on {text:?}");
        }
    }
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs() < 30, "took {elapsed:?}");
}
