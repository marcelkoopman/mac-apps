use super::*;

#[test]
fn hidden_copy_shows_no_text_and_no_actions() {
    let hidden = data(SubjectKind::Hidden, None);
    let card = work_card(&hidden);
    assert_eq!(card.title, "Hidden content");
    assert!(card.excerpt.is_empty() && card.meta.is_empty());
    assert!(chips(&hidden).is_empty());
    let actions = content_actions(&hidden);
    assert!(!actions.copy && !actions.save);
}

#[test]
fn broken_json_stays_json_says_where_and_marks_the_line() {
    let src = "{\n  \"id\": 42,\n  \"tags\": [\"a\", \"b\"],\n}";
    let input = data(SubjectKind::Text, Some(src));
    let card = work_card(&input);
    assert_eq!(card.title, "JSON · Invalid");
    assert_eq!(card.excerpt, src);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    assert_eq!(
        card.meta,
        "4 lines  0.037 KB  ·  Line 4, column 1: trailing comma"
    );
    assert_eq!(card.error_line, Some(4));
    assert_eq!(
        ids(&chips(&input)),
        vec![CommandId::TextMenu],
        "Text ▾ only: no Schema, no To JSON"
    );
    // Labels stay at the end of the meta line.
    let mail = "{\"mail\": \"jan.devries@example.nl\",}";
    let card = work_card(&data(SubjectKind::Text, Some(mail)));
    assert!(
        card.meta
            .ends_with("Line 1, column 35: trailing comma  ·  PII"),
        "{}",
        card.meta
    );
    // `{a: 1}` and a normal YAML document stay YAML.
    for yaml in [
        "{a: 1}",
        "name: \"x\"\ntags: [\"a\", \"b\"]\nnested:\n  key: \"v\"\n",
    ] {
        let card = work_card(&data(SubjectKind::Text, Some(yaml)));
        assert_eq!(card.title, "YAML", "{yaml}");
        assert_eq!(card.error_line, None);
    }
    assert_eq!(crate::commands::line_range("ab\ncd\r\nef", 2), Some(3..5));
    assert_eq!(crate::commands::line_range("ab", 2), None);
}

#[test]
fn yaml_to_json_is_a_view_that_copy_and_save_take_as_json() {
    let src = "base: &b\n  email: jan.devries@example.nl\nuser:\n  <<: *b\n  id: 7\n";
    let mut input = data(SubjectKind::Text, Some(src));
    let to_json = chips(&input)
        .into_iter()
        .find(|cmd| cmd.title == "To JSON")
        .expect("To JSON chip");
    assert_eq!(to_json.id, CommandId::Convert);
    input.view = CardView::from_command(&to_json.id).expect("view");
    let card = work_card(&input);
    let body = transformed_text(src, CardView::Convert).expect("json");
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(value["user"]["email"], "jan.devries@example.nl");
    assert_eq!(value["user"]["id"], 7);
    assert_eq!(card.excerpt, body);
    assert_eq!(card.title, "JSON");
    // The result is colored like the JSON card, not shown as plain text.
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    let toks = crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Json);
    assert!(
        toks.iter().any(|(kind, text)| {
            *kind == crate::highlight::TokenKind::Key && text == "\"user\""
        })
    );
    // Original goes back to the YAML, highlighted as YAML.
    let mut original = data(SubjectKind::Text, Some(src));
    original.view = CardView::Original;
    assert_eq!(
        work_card(&original).highlight,
        Some(crate::format::FormatKind::Yaml)
    );
    // The labels are the result's; the well is masked like any copied text.
    assert!(card.meta.ends_with("  ·  PII"), "{}", card.meta);
    assert!(masks_content(&card, CardView::Convert));
    let save = text_save_file(src, CardView::Convert).expect("save");
    assert_eq!(
        (save.filename.as_str(), save.extension),
        ("clipboard.json", "json")
    );
    assert_eq!(save.bytes, body.as_bytes());
    // Never on JSON (also YAML), nor on YAML that does not parse.
    for not_yaml in [r#"{"a": 1}"#, "[1, 2, 3]", "a: [1, 2\nb: 3\n"] {
        let shown = chips(&data(SubjectKind::Text, Some(not_yaml)));
        assert!(shown.iter().all(|cmd| cmd.title != "To JSON"), "{not_yaml}");
    }
}

#[test]
fn table_opens_formatted_and_coloured() {
    let samples = [
        (
            "name,age\nalice,30\nbob,40",
            crate::format::FormatKind::Csv,
            "age",
            "30",
            "40",
        ),
        (
            "name\tage\nalice\t30\nbob\t40",
            crate::format::FormatKind::Tsv,
            "age",
            "30",
            "40",
        ),
        (
            "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900",
            crate::format::FormatKind::Csv,
            "Salaris",
            "3450",
            "2900",
        ),
    ];
    for (src, kind, header, first, second) in samples {
        let input = data(SubjectKind::Text, Some(src));
        assert_eq!(presented_view(src, CardView::Original), CardView::Original);
        let card = work_card(&input);
        assert_eq!(card.highlight, Some(kind), "{src}");
        assert!(card.selectable, "{src}");
        assert!(!card.excerpt.contains('┆'), "{src}\n{}", card.excerpt);
        assert!(!card.excerpt.contains("shape"), "{src}");
        let lines: Vec<&str> = card.excerpt.lines().collect();
        assert!(lines.len() >= 3, "{src}");
        let column = lines[0].find(header).expect(header);
        assert_eq!(
            lines[1].find(first),
            Some(column),
            "{src}\n{}",
            card.excerpt
        );
        assert_eq!(
            lines[2].find(second),
            Some(column),
            "{src}\n{}",
            card.excerpt
        );
        let painted: String = crate::highlight::tokens(&card.excerpt, kind)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(painted, card.excerpt, "{src}");
        assert!(
            !chips(&input).iter().any(|cmd| cmd.id == CommandId::Format),
            "{src}"
        );
        assert!(
            chips(&input)
                .iter()
                .any(|cmd| cmd.id == CommandId::TableOpenWindow),
            "{src}"
        );
        let file = text_save_file(src, presented_view(src, CardView::Original)).unwrap();
        assert_eq!(file.bytes, src.as_bytes(), "{src}");
    }
    let quoted = "name,city\nalice,\"Den Haag, NL\"\nbob,Utrecht";
    let card = work_card(&data(SubjectKind::Text, Some(quoted)));
    let lines: Vec<&str> = card.excerpt.lines().collect();
    let city = lines[0].find("city").expect("city");
    assert_eq!(lines[1].find('"'), Some(city), "{}", card.excerpt);
    assert_eq!(lines[2].find("Utrecht"), Some(city), "{}", card.excerpt);
    assert!(card.excerpt.contains("\"Den Haag, NL\""));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Csv));
    let salary = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
    let card = work_card(&data(SubjectKind::Text, Some(salary)));
    assert_eq!(card.title, "CSV");
    assert!(card.meta.contains("PII · financial"), "{}", card.meta);
    assert!(!card.excerpt.contains('\u{00b7}'));
    let tsv = work_card(&data(
        SubjectKind::Text,
        Some("name\tage\nalice\t30\nbob\t40"),
    ));
    assert!(tsv.excerpt.contains('\u{00b7}'), "{}", tsv.excerpt);
    assert!(!tsv.excerpt.contains('\t'), "{}", tsv.excerpt);
}

#[test]
fn decoded_jwt_stays_a_masked_credential() {
    let token = crate::decode::testdata("jwt_expired.txt");
    let original = data(SubjectKind::Text, Some(&token));
    let decoder: Vec<_> = chips(&original)
        .into_iter()
        .filter(|cmd| cmd.id == CommandId::Decode)
        .collect();
    assert_eq!(decoder.len(), 1);
    assert_eq!(decoder[0].title, "JWT");
    assert!(work_card(&original).meta.contains("credential"));
    let mut decoded = original.clone();
    decoded.view = CardView::Decode;
    let card = work_card(&decoded);
    assert_eq!(card.title, "JWT");
    assert!(card.meta.contains("credential"), "{}", card.meta);
    // Header and payload JSON are drawn in color; the notes stay in the view.
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Jwt));
    let toks = crate::highlight::tokens(&card.excerpt, crate::format::FormatKind::Jwt);
    assert!(
        toks.iter()
            .any(|(kind, text)| { *kind == crate::highlight::TokenKind::Key && text == "\"alg\"" })
    );
    assert!(card.excerpt.starts_with("Signature not verified"));
    // Original goes back, uncolored by the JWT highlighter.
    assert_ne!(
        work_card(&original).highlight,
        Some(crate::format::FormatKind::Jwt)
    );
    assert!(card.excerpt.starts_with("Signature not verified"));
    assert!(masks_content(&card, CardView::Decode));
}

#[test]
fn decoded_base64_json_is_colored_like_the_json_card() {
    let text = crate::decode::testdata("base64url_json_no_padding.txt");
    let mut input = data(SubjectKind::Text, Some(&text));
    input.view = CardView::Decode;
    let card = work_card(&input);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
}

#[test]
fn decoded_picture_is_drawn_blurred_with_its_description() {
    let text = crate::decode::testdata("base64_png_image.txt");
    let mut input = data(SubjectKind::Text, Some(&text));
    input.view = CardView::Decode;
    // Until the picture is decoded (on its thread) the card describes it.
    let pending = work_card(&input);
    assert!(!pending.shows_image);
    assert!(pending.excerpt.starts_with("PNG image\n4 × 4 pixels"));
    let bytes = crate::decode::picture_bytes(&text).expect("picture bytes");
    input.picture = crate::clipboard::SecretBytes::new(bytes);
    let card = work_card(&input);
    assert!(card.shows_image);
    assert!(card.excerpt.is_empty());
    assert_eq!(card.title, "Base64");
    assert!(
        card.meta.starts_with("PNG image  ·  4 × 4 pixels  ·  "),
        "{}",
        card.meta
    );
    assert!(card.meta.contains("bytes"), "{}", card.meta);
    // Blurred until clicked, like any picture; the same item in every view.
    assert!(masks_content(&card, CardView::Decode));
    let mut original = input.clone();
    original.view = CardView::Original;
    let back = work_card(&original);
    assert!(!back.shows_image);
    assert_eq!(back.excerpt, text.trim());
    assert_eq!(content_key(&input), content_key(&original));
    let mut without = input.clone();
    without.picture = None;
    assert_eq!(content_key(&input), content_key(&without));
    // Copy and Save still take the description, never the picture.
    assert!(
        transformed_text(&text, CardView::Decode)
            .expect("text")
            .starts_with("PNG image")
    );
}

#[test]
fn only_a_base64_picture_is_drawn_in_the_decode_view() {
    let words = crate::decode::testdata("base64_text.txt");
    let mut other = data(SubjectKind::Text, Some(&words));
    other.view = CardView::Decode;
    other.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    let card = work_card(&other);
    assert!(!card.shows_image);
    assert!(card.excerpt.starts_with("Copycraft decodes"));
    // Nor in another view of a picture copy.
    let text = crate::decode::testdata("base64_png_image.txt");
    let mut picture = data(SubjectKind::Text, Some(&text));
    picture.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    assert!(!work_card(&picture).shows_image);
}

#[test]
fn prose_with_email_has_no_redact_button() {
    let text = "mail me at jan.devries@email.nl please";
    let input = data(SubjectKind::Text, Some(text));
    let card = work_card(&input);
    assert!(card.meta.contains("PII"), "{}", card.meta);
}

#[test]
fn sensitive_copy_labels_the_meta_without_a_redact_button() {
    let text = "aws creds AKIAIOSFODNN7EXAMPLE rotated";
    let input = data(SubjectKind::Text, Some(text));
    let card = work_card(&input);
    assert!(card.meta.contains("credential"));
    assert!(!card.meta.contains("AKIAIOSFODNN7EXAMPLE"));
    let rust = "fn main() {\n    let key = \"AKIAIOSFODNN7EXAMPLE\";\n}\n";
    let rust_input = data(SubjectKind::Text, Some(rust));
    let card = work_card(&rust_input);
    assert!(card.meta.contains("credential"));
    assert!(!card.meta.contains("PII"), "{}", card.meta);
    assert!(card.excerpt.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn rust_code_is_not_labeled_pii_and_hides_redact() {
    let src = "\
use std::io;
use std::collections::HashMap;

fn main() {
    let name: String = String::new();
    let host = \"127.0.0.1\";
}
";
    let input = data(SubjectKind::Text, Some(src));
    let card = work_card(&input);
    assert!(!card.meta.contains("PII"), "{}", card.meta);
    assert!(!card.meta.contains("financial"), "{}", card.meta);
    assert!(!card.meta.contains("credential"), "{}", card.meta);
}

#[test]
fn credential_url_stays_a_link_card() {
    let url = "https://deploy:s3cr3t@github.com/acme/app.git";
    let input = data(SubjectKind::Text, Some(url));
    let card = work_card(&input);
    assert!(card.link_page.is_some());
    assert_eq!(card.title, "Link");
    assert!(!card.title.contains("deploy"));
    assert!(!card.title.contains("s3cr3t"));
    assert!(card.meta.contains("credential"));
    assert!(!card.meta.contains("s3cr3t"));
    assert!(!card.meta.contains("deploy"));
    assert!(crate::page_preview::url_has_userinfo(url));
    assert!(!crate::page_preview::url_has_userinfo(
        "https://github.com/acme/app"
    ));
    assert_eq!(
        chips(&input)
            .iter()
            .map(|cmd| cmd.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Visit"]
    );
}

#[test]
fn privacy_filter_decides_whether_content_is_masked() {
    // On: masked until revealed, only for content that masks.
    assert!(effective_mask(true, true, false));
    assert!(!effective_mask(true, true, true));
    assert!(!effective_mask(true, false, false));
    // Off: never masked, even when nothing was revealed.
    assert!(!effective_mask(false, true, false));
    assert!(!effective_mask(false, true, true));
    assert!(!effective_mask(false, false, false));
}

#[test]
fn failed_blur_hides_the_excerpt_until_reveal() {
    let secret = "https://deploy:s3cr3t@github.com/acme/app.git";
    let failed = well_mask(true, false);
    assert!(!failed.blur);
    assert!(failed.shade);
    assert!(!failed.show_body);
    let shown = if failed.show_body { secret } else { "" };
    assert!(shown.is_empty());
    assert!(!shown.contains("deploy"));
    assert!(!shown.contains("s3cr3t"));
    let blurred = well_mask(true, true);
    assert!(blurred.blur);
    assert!(!blurred.shade);
    assert!(blurred.show_body);
    let open = well_mask(false, false);
    assert!(!open.blur);
    assert!(!open.shade);
    assert!(open.show_body);
    let card = work_card(&data(SubjectKind::Text, Some(secret)));
    assert!(masks_content(&card, CardView::Original));
    assert_ne!(
        content_key(&data(SubjectKind::Text, Some(secret))),
        content_key(&data(SubjectKind::Text, Some("another copy")))
    );
}

#[test]
fn python_cards_are_highlighted_and_checked() {
    let sound = "def greet(name):\n    print(f\"Hello {name}\")\n";
    let input = data(SubjectKind::Text, Some(sound));
    let card = work_card(&input);
    assert_eq!(card.title, "Python");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Python));
    assert_eq!(card.excerpt, sound, "no formatter: shown as copied");
    let shown = chips(&input);
    assert!(!shown.iter().any(|cmd| matches!(
        cmd.id,
        CommandId::Format | CommandId::Convert | CommandId::Schema
    )));

    let missing = "def greet(name):\n    print(f\"Hello {name}\"\n";
    assert_eq!(
        work_card(&data(SubjectKind::Text, Some(missing))).title,
        "Python · missing )"
    );
    let no_body = "import os\n\ndef main():\nprint(os.getcwd())\n";
    assert_eq!(
        work_card(&data(SubjectKind::Text, Some(no_body))).title,
        "Python · expected indent"
    );
    let mixed = "def main():\n    x = 1\n\tprint(x)\n";
    assert_eq!(
        work_card(&data(SubjectKind::Text, Some(mixed))).title,
        "Python · mixed tabs and spaces"
    );
    assert_eq!(
        work_card(&data(SubjectKind::Text, Some("f = open(\"demofile.txt\")"))).title,
        "Content"
    );
}

#[test]
fn unbalanced_code_says_so_in_the_title() {
    let java = "public class Main {\n  public static void main(String[] args) {\n    System.out.println(\"Hello World\");\n  }\n";
    let input = data(SubjectKind::Text, Some(java));
    let card = work_card(&input);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Java));
    assert_eq!(card.title, "Java · missing }");
    // Still formatted, and no bracket is added.
    assert!(card.excerpt.contains("        System.out.println"));
    assert_eq!(card.excerpt.matches('}').count(), java.matches('}').count());

    let rust = "fn main() {\n    run());\n}\n";
    let card = work_card(&data(SubjectKind::Text, Some(rust)));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Rust));
    assert_eq!(card.title, "Rust · unbalanced )");

    let balanced = format!("{java}}}\n");
    assert_eq!(
        work_card(&data(SubjectKind::Text, Some(&balanced))).title,
        "Formatted Java"
    );
}

#[test]
fn a_step_copy_shows_its_label_and_is_another_entry() {
    let original = edited_picture(None, false, false);
    assert_eq!(work_card(&original).meta, "PNG  1024×768  0.18 MB");
    let mut resized = edited_picture(Some("Resized to 512×384"), true, false);
    resized.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 512,
        height: 384,
        byte_len: 52_000,
    });
    resized.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    assert_eq!(
        work_card(&resized).meta,
        "PNG  512×384  0.05 MB  ·  Resized to 512×384"
    );
    // One copy keeps its key while a step runs on it; the step's copy is another one.
    let mut running = original.clone();
    running.image_edit.as_mut().unwrap().working = true;
    assert_eq!(content_key(&original), content_key(&running));
    let mut other = resized.clone();
    other.image_edit.as_mut().unwrap().entry = 8;
    assert_ne!(content_key(&other), content_key(&resized));

    let mut working = resized.clone();
    working.image_edit.as_mut().unwrap().working = true;
    assert!(work_card(&working).meta.ends_with("  ·  Working…"));
    let mut noted = original.clone();
    noted.image_edit.as_mut().unwrap().note = Some("Already that size or smaller".into());
    assert_eq!(
        work_card(&noted).meta,
        "PNG  1024×768  0.18 MB  ·  Already that size or smaller"
    );
}

#[test]
fn ocr_and_qr_bodies_label_secrets_and_redact_scrubs_them() {
    let ocr = "pay NL91ABNA0417164300 key AKIAIOSFODNN7EXAMPLE";
    let mut input = data(SubjectKind::Image, None);
    input.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 32,
        height: 32,
        byte_len: 1_200,
    });
    input.image_scan = Some(ImageScan {
        info: "PNG\nSize: 32×32".into(),
        data_url: None,
        ocr: Some(ocr.into()),
        qr: None,
    });
    let picture = work_card(&input);
    assert!(picture.shows_image);
    assert!(!picture.meta.contains("credential"));
    input.view = CardView::Ocr;
    let card = work_card(&input);
    assert!(card.meta.contains("credential"), "{}", card.meta);
    assert!(card.meta.contains("financial"), "{}", card.meta);
    assert!(card.excerpt.contains("NL91ABNA0417164300"));
    assert!(card.excerpt.contains("AKIAIOSFODNN7EXAMPLE"));

    let mut qr = data(SubjectKind::Image, None);
    qr.image_scan = Some(ImageScan {
        info: "Image".into(),
        data_url: None,
        ocr: None,
        qr: Some("NL91ABNA0417164300".into()),
    });
    qr.view = CardView::Qr;
    let card = work_card(&qr);
    assert!(card.meta.contains("financial"), "{}", card.meta);
    assert!(card.excerpt.contains("NL91ABNA0417164300"));
}

#[test]
fn html_url_previews_like_a_page() {
    let url = "https://www.example.com/news/story";
    let input = data(SubjectKind::Text, Some(url));
    let card = work_card(&input);
    assert_eq!(card.title, "Link");
    assert_eq!(card.link_page.as_deref(), Some(url));
    // Domain without `www.`, then the path; the URL itself is in `link_page`.
    assert_eq!(card.excerpt, "example.com\n/news/story");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Url));
    assert!(card.selectable);
    // Blurred until revealed, like any copy.
    assert!(masks_content(&card, CardView::Original));
    assert_eq!(titles(&chips(&input)), vec!["Visit"]);
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: false,
            preview: false,
        }
    );
}

#[test]
fn bare_host_is_a_link_card() {
    let input = data(SubjectKind::Text, Some("grok.com"));
    let card = work_card(&input);
    assert_eq!(card.title, "Link");
    assert_eq!(card.link_page.as_deref(), Some("grok.com"));
    assert_eq!(titles(&chips(&input)), vec!["Visit", "Format"]);
}

#[test]
fn link_with_a_query_shows_its_url_view() {
    let link = "https://example.com/docs?page=2&sort=name";
    let input = data(SubjectKind::Text, Some(link));
    assert_eq!(work_card(&input).title, "Link");
    assert_eq!(titles(&chips(&input)), vec!["Visit", "URL"]);
    let mut decoded = input.clone();
    decoded.view = CardView::Decode;
    let card = work_card(&decoded);
    assert_eq!(card.title, "URL");
    assert!(card.link_page.is_none());
    assert!(
        card.excerpt.contains("page = 2\nsort = name"),
        "{}",
        card.excerpt
    );
    assert!(masks_content(&card, CardView::Decode));
    // A link without a query or escapes has no decoder chip.
    let plain = data(SubjectKind::Text, Some("https://example.com/docs"));
    assert_eq!(titles(&chips(&plain)), vec!["Visit"]);
}

#[test]
fn hide_button_masks_the_entry_whatever_the_settings() {
    use crate::commands::{hide_button_look, toggle_hidden, well_masked};
    // Hidden by the button: masked even with the privacy filter off and content that never masks.
    assert!(well_masked(false, false, false, false, true));
    assert!(well_masked(false, false, false, true, true));
    assert!(!well_masked(false, true, false, false, false));
    assert!(well_masked(true, true, false, false, false));
    // A click on visible content hides it; on masked content shows it.
    assert_eq!(toggle_hidden(false), (true, false));
    assert_eq!(toggle_hidden(true), (false, true));
    assert_eq!(hide_button_look(false), ("eye.slash", "hide_content"));
    assert_eq!(hide_button_look(true), ("eye", "show_content"));
    assert_eq!(crate::locale::t("hide_content"), "Hide");
    assert_eq!(crate::locale::t("show_content"), "Show");
}

#[test]
fn the_eye_choice_sticks_per_item() {
    use crate::commands::BlurChoices;
    let mut choices = BlurChoices::default();
    // No choice: what the card had (first display masked by the filter).
    assert_eq!(choices.apply(1, false, false), (false, false));
    // Shown with the eye: item 1 stays shown when it comes back; item 2 is untouched.
    choices.remember(1, false);
    assert_eq!(choices.apply(1, false, false), (false, true));
    assert_eq!(choices.apply(2, false, false), (false, false));
    // Hidden with the eye: stays hidden, even after a reveal elsewhere.
    choices.remember(2, true);
    assert_eq!(choices.apply(2, false, true), (true, false));
    // A new choice replaces the old one.
    choices.remember(1, true);
    assert_eq!(choices.choice(1), Some(true));
    // Only the last KEEP items are kept; Wipe forgets all.
    for key in 100..100 + BlurChoices::KEEP as u64 {
        choices.remember(key, false);
    }
    assert_eq!(choices.choice(1), None);
    choices.clear();
    assert_eq!(choices.choice(150), None);
}

#[test]
fn with_the_privacy_filter_off_nothing_covers_the_well() {
    use crate::commands::well_masked;
    // Filter off: no cover for content that masks, a sensitive diff, or both; revealed or not.
    for content_masks in [false, true] {
        for diff_lock in [false, true] {
            for revealed in [false, true] {
                assert!(
                    !well_masked(false, content_masks, diff_lock, revealed, false),
                    "{content_masks} {diff_lock} {revealed}"
                );
            }
        }
    }
    // Filter on: a sensitive diff is masked until reveal, like content that masks.
    assert!(well_masked(true, false, true, false, false));
    assert!(!well_masked(true, false, true, true, false));
    assert!(!well_masked(true, false, false, false, false));
    // Only the hide button covers with the filter off: the user asked for it.
    assert!(well_masked(false, true, true, false, true));
}

#[test]
fn an_opaque_cover_says_to_click_it() {
    use crate::commands::cover_hint;
    assert_eq!(cover_hint(false, false), None);
    assert_eq!(cover_hint(false, true), None);
    assert_eq!(
        cover_hint(true, false),
        Some(crate::locale::t("click_to_reveal"))
    );
    assert_eq!(
        cover_hint(true, true),
        Some(crate::locale::t("checking_sensitive"))
    );
    assert!(!crate::locale::t("click_to_reveal").is_empty());
}
