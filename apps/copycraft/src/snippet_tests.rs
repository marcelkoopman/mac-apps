use super::*;

fn auto() -> Automatic {
    Automatic {
        date: "28-10-2026".into(),
        time: "14:05".into(),
        uuid: "0f8fad5b-d9cb-469f-a165-70867728950e".into(),
        clipboard: zeroize::Zeroizing::new("PR #42".to_string()),
    }
}

#[test]
fn placeholders_ask_once_in_order() {
    let text = "Hoi {{naam}},\n\nZie {{ link }} (voor {{Naam}}). {{datum}} {{bedrijf}}";
    assert_eq!(fields(text), vec!["naam", "link", "bedrijf"]);
    assert!(fields("Alleen {{datum}} en {{uuid}}").is_empty());
    assert!(fields("Gewone tekst").is_empty());
}

#[test]
fn a_snippet_fills_in_typed_and_automatic_values() {
    let text = "Hoi {{naam}}, op {{datum}} om {{tijd}}: {{klembord}} ({{uuid}}). Groet, {{Naam}}";
    let answers = vec!["Ada".to_string()];
    assert_eq!(
        fill(text, &answers, &auto()),
        "Hoi Ada, op 28-10-2026 om 14:05: PR #42 (0f8fad5b-d9cb-469f-a165-70867728950e). Groet, Ada"
    );
    // English names, and case does not matter.
    assert_eq!(
        fill("{{DATE}} {{time}} {{Clipboard}}", &[], &auto()),
        "28-10-2026 14:05 PR #42"
    );
    // A missing answer is empty.
    assert_eq!(fill("[{{a}}|{{b}}]", &["x".into()], &auto()), "[x|]");
}

#[test]
fn other_braces_stay_as_they_are() {
    for text in [
        "fn main() { let x = {{1, 2}}; }",
        "{{ }} and {{}} and {{a/b}} and {{",
        "{{ niet afgesloten",
        "{\"json\": {\"a\": 1}}",
    ] {
        assert!(fields(text).is_empty(), "{text}");
        assert_eq!(fill(text, &[], &auto()), text);
    }
    // A name over 40 characters is not a placeholder.
    let long = format!("{{{{{}}}}}", "a".repeat(41));
    assert!(fields(&long).is_empty());
    assert_eq!(fields("{{{naam}}}"), vec!["naam"]);
    assert_eq!(fill("{{{naam}}}", &["Ada".into()], &auto()), "{Ada}");
}

#[test]
fn at_most_eight_fields_are_asked() {
    let text: String = (1..=10).map(|n| format!("{{{{v{n}}}}} ")).collect();
    let names = fields(&text);
    assert_eq!(names.len(), MAX_FIELDS);
    let answers: Vec<String> = (1..=8).map(|n| n.to_string()).collect();
    assert_eq!(
        fill(&text, &answers, &auto()),
        "1 2 3 4 5 6 7 8 {{v9}} {{v10}} "
    );
}

#[test]
fn the_date_follows_the_language_and_the_time_is_local() {
    // 2026-10-28 13:05:00 UTC, one hour east.
    let moment = crate::generate::Moment {
        unix_ms: 1_793_192_700_000,
        offset: 3600,
    };
    let nl = Automatic::at(moment, [0; 16], "x", true);
    assert_eq!(nl.date, "28-10-2026");
    assert_eq!(nl.time, "14:05");
    assert_eq!(nl.clipboard.as_str(), "x");
    assert_eq!(nl.uuid, "00000000-0000-4000-8000-000000000000");
    let en = Automatic::at(moment, [0; 16], "", false);
    assert_eq!(en.date, "2026-10-28");
}
