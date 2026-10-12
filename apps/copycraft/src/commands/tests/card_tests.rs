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
    assert!(chips(&input).is_empty(), "no Schema, no To JSON");
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
        assert_ne!(file.extension, "parquet", "{src}");
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
fn a_picture_version_shows_its_label_and_stays_the_same_entry() {
    let original = edited_picture(0, &["Original", "Resized to 512×384"]);
    assert_eq!(work_card(&original).meta, "PNG  1024×768  0.18 MB");
    let mut resized = edited_picture(1, &["Original", "Resized to 512×384"]);
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
    // Every version is the same entry: the reveal stays.
    assert_eq!(content_key(&original), content_key(&resized));
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
    assert_eq!(card.excerpt, url);
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
    assert_eq!(titles(&chips(&input)), vec!["Visit", "Original", "URL"]);
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
fn a_table_below_a_definition_line_opens_as_a_table_with_a_header_note() {
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Csv));
    assert!(card.meta.contains("  ·  Header on line 2"), "{}", card.meta);
    // The definition line stays above the aligned table, as copied.
    let first = card.excerpt.lines().next().unwrap_or_default();
    assert!(
        first
            .trim_start_matches('\u{feff}')
            .starts_with("Definition:")
    );
    let second = card.excerpt.lines().nth(1).unwrap_or_default();
    assert!(second.starts_with("Date"), "{:?}", second.get(..12));
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    // 20 columns: Table shows the grid directly (shortened names); Copy keeps the real ones.
    let table = read(src, crate::dataframe::ReadOptions::default());
    input.table = Some(table);
    let grid = work_card(&input);
    assert_eq!(grid.title, "Table");
    assert!(!grid.excerpt.contains("shape:"), "{}", grid.excerpt);
    assert!(grid.excerpt.contains("┌"), "{}", grid.excerpt);
    assert!(grid.excerpt.contains("┆ … PV3"), "{}", grid.excerpt);
    assert!(!grid.excerpt.contains("Sunbox"), "{}", grid.excerpt);
    assert!(grid.meta.contains("Header on line 2"), "{}", grid.meta);
    assert!(grid.preview_note.is_none());
    // No Show columns / Show table chip — only Original and Table ▾.
    assert!(ids(&crate::commands::chips(&input)).contains(&CommandId::TableOpenWindow));
    let copied = transformed_text(src, CardView::Dataframe).expect("copy");
    assert!(copied.contains("Sunbox 7"), "{copied}");
    assert!(
        ids(&crate::commands::chips(&data(SubjectKind::Text, Some(src))))
            .contains(&CommandId::TableOpenWindow)
    );
}

#[test]
fn the_dataframe_view_shows_the_table_version() {
    let src = "name,n\na,1\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let original = work_card(&input);
    assert!(original.excerpt.contains("┌"), "{}", original.excerpt);
    input.table = Some(deduped(src));
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    assert!(card.meta.starts_with("3 lines"), "{}", card.meta);
    // The step of the version shown, right after the size.
    assert!(
        card.meta.contains("KB  ·  Duplicates removed"),
        "{}",
        card.meta
    );
    // Another version is the same entry: it stays revealed ([`stays_revealed`]).
    let plain = data(SubjectKind::Text, Some(src));
    let mut versioned = plain.clone();
    versioned.table = input.table.clone();
    assert_eq!(content_key(&plain), content_key(&versioned));
    // The Original view shows the text as copied.
    versioned.view = CardView::Original;
    assert_eq!(work_card(&versioned).excerpt, work_card(&plain).excerpt);
}

#[test]
fn a_table_version_being_worked_out_says_so_and_errors_show_in_the_meta() {
    let src = "name,n\na,1\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let mut table = deduped(src);
    table.frame = None;
    table.working = true;
    input.table = Some(table.clone());
    let card = work_card(&input);
    assert!(card.excerpt.is_empty());
    assert_eq!(card.placeholder, "Working on the table…");
    table = deduped(src);
    table.error = Some("20 versions at most".to_string());
    input.table = Some(table);
    assert!(work_card(&input).meta.contains("20 versions at most"));
    // The original with a table attached is the plain Dataframe card.
    let mut original = deduped(src);
    original.version = 0;
    original.labels.truncate(1);
    input.table = None;
    let plain = work_card(&input);
    input.table = Some(original);
    assert_eq!(work_card(&input).excerpt, plain.excerpt);
}

#[test]
fn table_chip_row_is_original_and_table_only_no_show_columns() {
    use crate::table::TableOp;
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    let names: Vec<String> = crate::dataframe::parse_table(src)
        .expect("table")
        .get_column_names()
        .iter()
        .take(6)
        .map(|name| name.to_string())
        .collect();
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let mut table = stepped(
        src,
        TableOp::SelectColumns {
            columns: names,
            kept_of: None,
        },
    );
    // Even if overview were requested, display is the grid.
    table.overview = Some(true);
    input.table = Some(table.clone());
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    assert!(ids(&chips(&input)).contains(&CommandId::TableOpenWindow));
    table.overview = Some(false);
    input.table = Some(table);
    let card = work_card(&input);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    let narrow = "name,n\na,1\nb,2";
    let mut input = data(SubjectKind::Text, Some(narrow));
    input.view = CardView::Dataframe;
    let table = read(narrow, crate::dataframe::ReadOptions::default());
    input.table = Some(table);
    assert!(work_card(&input).excerpt.contains("┌"));
    assert!(ids(&chips(&input)).contains(&CommandId::TableOpenWindow));
}

#[test]
fn a_revealed_entry_stays_revealed_in_every_view_version_and_step() {
    use crate::sensitivity::Label;
    let energy = crate::dataframe::tests::ENERGY_FIXTURE;
    let pii = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
    for src in [energy, pii] {
        let key = content_key(&data(SubjectKind::Text, Some(src)));
        for (what, shown) in shown_again(src) {
            assert_eq!(content_key(&shown), key, "{what}");
            assert!(stays_revealed(true, key, content_key(&shown)), "{what}");
            // The card is built whatever it shows.
            let _ = work_card(&shown);
        }
    }
    // Views of one entry show different labels; that does not mask it again.
    let mut original = data(SubjectKind::Text, Some(pii));
    let labels = meta_labels(&work_card(&original).meta);
    assert_eq!(labels, [Label::Pii, Label::Financial]);
    original.view = CardView::Dataframe;
    original.table = Some(stepped(
        pii,
        crate::table::TableOp::SelectColumns {
            columns: vec!["Id".into(), "Naam".into()],
            kept_of: Some(3),
        },
    ));
    let card = work_card(&original);
    assert!(
        !meta_labels(&card.meta).contains(&Label::Financial),
        "{}",
        card.meta
    );
    let key = content_key(&original);
    assert!(stays_revealed(true, key, key));
    // Not revealed stays not revealed.
    assert!(!stays_revealed(false, key, key));
}

#[test]
fn a_revealed_entry_stays_revealed_when_its_labels_arrive() {
    // Long enough for the background checker: "Checking…" first, then the labels.
    let mut src = String::from("name,email\n");
    while src.len() <= crate::sensitivity::SYNC_LIMIT {
        src.push_str("Jan,jan@example.com\n");
    }
    let shown = data(SubjectKind::Text, Some(&src));
    let key = content_key(&shown);
    let checking = work_card(&shown);
    // Neither the card's labels nor their status are part of the entry's identity: the
    // card rebuilt when they arrive is the same entry, and stays revealed.
    let _ = crate::sensitivity::labeling(&src);
    let arrived = work_card(&shown);
    assert_eq!(content_key(&shown), key);
    assert!(stays_revealed(true, key, content_key(&shown)));
    assert!(!checking.excerpt.is_empty() && !arrived.excerpt.is_empty());
}

#[test]
fn another_entry_is_masked_again() {
    let first = data(SubjectKind::Text, Some("Id;Naam\n1;Piet"));
    let second = data(SubjectKind::Text, Some("Id;Naam\n1;Jan"));
    assert!(!stays_revealed(
        true,
        content_key(&first),
        content_key(&second)
    ));
    let mut opened = first.clone();
    opened.source_name = Some("piet.csv".into());
    assert!(!stays_revealed(
        true,
        content_key(&first),
        content_key(&opened)
    ));
}

#[test]
fn dataframe_meta_says_when_dates_could_be_either_order() {
    let mut input = data(
        SubjectKind::Text,
        Some("when,amount\n01/02/2024,1\n03/04/2024,2"),
    );
    input.view = CardView::Dataframe;
    let card = work_card(&input);
    assert!(
        card.meta.contains("  ·  Dates read as dd/mm/yyyy"),
        "{}",
        card.meta
    );
    input.full = true;
    assert!(work_card(&input).meta.contains("Dates read as dd/mm/yyyy"));
}

#[test]
fn a_long_table_below_a_title_counts_rows_from_the_header() {
    let mut src = String::from("Title of the export\nname,age\n");
    for i in 0..(PREVIEW_ROWS + 50) {
        src.push_str(&format!("p{i},{i}\n"));
    }
    let card = work_card(&data(SubjectKind::Text, Some(&src)));
    assert_eq!(
        card.preview_note.as_deref(),
        Some(format!("Showing 199 of {} rows", PREVIEW_ROWS + 50).as_str())
    );
}

#[test]
fn salary_table_puts_the_labels_on_the_size_line() {
    let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert!(card.meta.contains("lines"));
    assert!(card.meta.contains("KB"));
    assert!(card.meta.contains("PII · financial"));
    assert!(!card.meta.contains("Jan de Vries"));
    let mut grid = data(SubjectKind::Text, Some(src));
    grid.view = CardView::Dataframe;
    let card = work_card(&grid);
    assert_eq!(card.title, "Table");
    assert!(card.meta.contains("PII · financial"), "{}", card.meta);
}

#[test]
fn chosen_file_keeps_the_kind_title_and_names_the_meta_line() {
    let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
    let mut input = data(SubjectKind::Text, Some(src));
    input.source_name = Some("people.tsv".to_string());
    let card = work_card(&input);
    assert_eq!(card.title, "TSV");
    assert!(card.meta.starts_with("people.tsv"));
    assert!(card.meta.contains("PII · financial"));
    assert!(card.excerpt.contains("Jan de Vries"));
}

#[test]
fn dropped_picture_is_an_image_card_with_its_name() {
    let mut input = data(SubjectKind::Image, None);
    input.source_name = Some("photo.heic".to_string());
    input.image = Some(ImageFacts {
        format: "HEIC".into(),
        width: 4032,
        height: 3024,
        byte_len: 2_000_000,
    });
    input.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    let card = work_card(&input);
    assert_eq!(card.title, "Image");
    assert_eq!(card.meta, "photo.heic  ·  HEIC  4032×3024  2.00 MB");
    assert!(card.shows_image);
    assert!(masks_content(&card, CardView::Original));
    assert_eq!(titles(&chips(&input)), vec!["Original", "Info"]);
    assert!(
        overflow(&input)
            .iter()
            .any(|cmd| cmd.id == CommandId::UseClipboard)
    );

    // Another picture alike in name, format and size is another item: not revealed yet.
    let mut other = data(SubjectKind::Image, None);
    other.source_name = input.source_name.clone();
    other.image = input.image.clone();
    other.picture = crate::clipboard::SecretBytes::new(vec![1, 2, 3]);
    assert_ne!(content_key(&input), content_key(&other));
    other.picture = input.picture.clone();
    assert_eq!(content_key(&input), content_key(&other));
}

#[test]
fn unreadable_file_uses_its_name_and_a_note() {
    let mut input = data(SubjectKind::NoText, None);
    input.source_name = Some("photo.png".to_string());
    input.source_note = Some("This file is not text".to_string());
    let card = work_card(&input);
    assert_eq!(card.title, "photo.png");
    assert_eq!(card.placeholder, "This file is not text");
    assert!(card.excerpt.is_empty());
    assert!(card.meta.is_empty());
    assert!(!card.shows_image);
}

#[test]
fn text_card_shows_the_payload_not_a_label() {
    let card = work_card(&data(SubjectKind::Text, Some("{\n  \"a\": 1\n}")));
    assert_eq!(card.title, "Valid JSON");
    assert_eq!(card.excerpt, "{\n  \"a\": 1\n}");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    assert!(card.meta.contains("lines"));
    assert!(card.meta.contains("KB"));
    assert!(!card.excerpt.contains("Preview"));
    assert!(!card.shows_image);
}

#[test]
fn a_clipboard_without_text_points_to_dropping_a_file() {
    let card = work_card(&data(SubjectKind::NoText, None));
    assert_eq!(
        card.placeholder,
        "No text on the clipboard\nDrop a text file or image here to open it"
    );
    assert!(card.excerpt.is_empty());
}

#[test]
fn a_copy_not_read_yet_says_so_and_the_first_one_explains_the_alert() {
    let pending = data(SubjectKind::Pending, None);
    let card = work_card(&pending);
    assert!(
        card.placeholder
            .starts_with("New copy — open the card to view\n")
    );
    assert!(card.excerpt.is_empty() && card.meta.is_empty());
    assert!(chips(&pending).is_empty());
    assert!(!content_actions(&pending).copy);
    let ask = data(SubjectKind::PasteAsk, None);
    let card = work_card(&ask);
    assert!(
        card.placeholder.contains("Choose Allow"),
        "{}",
        card.placeholder
    );
    assert!(card.placeholder.contains("Paste from Other Apps"));
    assert!(chips(&ask).is_empty());
}

#[test]
fn always_deny_says_so_instead_of_nothing_copied() {
    let input = data(SubjectKind::Denied, None);
    let card = work_card(&input);
    assert!(
        card.placeholder
            .starts_with("Clipboard access denied in Privacy & Security\n"),
        "{}",
        card.placeholder
    );
    assert!(!card.placeholder.contains("Nothing copied"));
    assert!(card.excerpt.is_empty() && card.meta.is_empty());
    assert!(chips(&input).is_empty());
    assert!(!content_actions(&input).copy);
}

#[test]
fn well_masks_copied_content_until_it_changes() {
    let secret = data(SubjectKind::Text, Some("correct horse battery staple"));
    let card = work_card(&secret);
    assert!(masks_content(&card, CardView::Original));
    assert!(!card.excerpt.contains("Click to reveal"));
    let empty = work_card(&data(SubjectKind::Empty, None));
    assert!(!masks_content(&empty, CardView::Original));
    assert_eq!(
        content_key(&secret),
        content_key(&data(
            SubjectKind::Text,
            Some("correct horse battery staple")
        ))
    );
    let mut formatted = secret.clone();
    formatted.view = CardView::Format;
    assert_eq!(content_key(&secret), content_key(&formatted));
    assert_ne!(
        content_key(&secret),
        content_key(&data(SubjectKind::Text, Some("another copy")))
    );
    let mut picture = data(SubjectKind::Image, None);
    picture.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 1280,
        height: 720,
        byte_len: 184_000,
    });
    assert!(masks_content(&work_card(&picture), CardView::Original));
    let mut info = picture.clone();
    info.view = CardView::Info;
    assert!(!masks_content(&work_card(&info), CardView::Info));
    assert_eq!(content_key(&picture), content_key(&info));
    let mut other = picture.clone();
    if let Some(image) = other.image.as_mut() {
        image.byte_len = 200_000;
    }
    assert_ne!(content_key(&picture), content_key(&other));
}

#[test]
fn history_label_is_the_mark_and_size() {
    let secret = "correct horse battery staple";
    let label = history_label("Aa", secret.len());
    assert_eq!(label, "Aa  0.028 KB");
    assert!(!label.contains(secret));
    assert!(!label.contains("horse"));
    assert_eq!(history_label("{}", 1_500), "{}  1.5 KB");
    assert_eq!(history_label("img", 184_000), "img  0.18 MB");
}

#[test]
fn prose_card_shows_the_whole_document() {
    let src = "\
# AGENTS.md

## Stack

- Rust Cargo workspace, edition 2024 zoals in `Cargo.toml`. Wijzig edition niet.
- Errors: `thiserror` in libraries, `anyhow` alleen in bins/CLIs.

## Commands

Kleinste opdracht die de change dekt.
";
    let card = work_card(&data(SubjectKind::Text, Some(src)));
    assert_eq!(card.excerpt, src);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Markdown));
    assert_eq!(card.title, "Markdown");
    assert!(
        chips(&data(SubjectKind::Text, Some(src)))
            .iter()
            .all(|cmd| cmd.id != CommandId::Format)
    );
    assert!(
        card.excerpt
            .contains("Kleinste opdracht die de change dekt.")
    );
    assert!(card.excerpt.contains("thiserror"));
    assert!(!card.excerpt.contains('…'));
}

#[test]
fn excerpt_keeps_lines_and_cuts_a_long_one() {
    assert_eq!(payload_excerpt("alpha\nbeta"), "alpha\nbeta");
    let long = "x".repeat(80);
    let cut = payload_excerpt(&long);
    assert!(cut.ends_with('…'));
    let many = (0..12)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let clipped = payload_excerpt(&many);
    assert_eq!(clipped.lines().count(), 6);
    assert!(clipped.ends_with('…'));
    assert!(cut.chars().count() <= 49);
}

#[test]
fn large_table_card_previews_rows_and_show_all_renders_them() {
    let src = csv_rows(300);
    let mut d = data(SubjectKind::Text, Some(&src));
    for view in [CardView::Original, CardView::Dataframe] {
        d.view = view;
        d.full = false;
        let card = work_card(&d);
        assert_eq!(
            card.preview_note.as_deref(),
            Some("Showing 200 of 300 rows"),
            "{view:?}"
        );
        assert!(card.excerpt.contains("name199"), "{view:?}");
        assert!(!card.excerpt.contains("name299"), "{view:?}");
        d.full = true;
        let full = work_card(&d);
        assert!(full.preview_note.is_none(), "{view:?}");
        if view == CardView::Original {
            assert!(full.excerpt.contains("name299"));
        }
    }
}

#[test]
fn small_table_card_has_no_preview_note() {
    let src = csv_rows(20);
    let mut d = data(SubjectKind::Text, Some(&src));
    for view in [CardView::Original, CardView::Dataframe] {
        d.view = view;
        assert!(work_card(&d).preview_note.is_none(), "{view:?}");
    }
}

#[test]
fn a_diff_chip_sits_beside_original_and_waits_to_be_copied() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.diff = Some(diff_shown(crate::diff::Phase::Working));
    let titles: Vec<_> = chips(&input).iter().map(|cmd| cmd.title.clone()).collect();
    assert_eq!(
        titles,
        [crate::locale::t("original"), crate::locale::t("chip_diff")]
    );
    // The spinner text is the diff view. The chip is already there on the source view.
    input.view = CardView::Diff;
    let card = work_card(&input);
    assert_eq!(card.placeholder, crate::locale::t("diff_working"));
    assert!(card.excerpt.is_empty());
    assert!(masks_content(&card, CardView::Diff));
    assert!(!content_actions(&input).copy && !content_actions(&input).save);

    input.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let card = work_card(&input);
    assert_eq!(card.meta, crate::locale::t("diff_none"));
    assert!(content_actions(&input).copy && content_actions(&input).save);

    let lines = match crate::diff::compare("alpha\n", "alpha\nbeta\n") {
        crate::diff::Outcome::Lines(lines) => lines,
        other => panic!("expected lines, got {other:?}"),
    };
    input.diff = Some(diff_shown(crate::diff::Phase::Lines(lines)));
    let card = work_card(&input);
    assert_eq!(card.meta, crate::locale::tf("diff_counts", &[&1, &0]));
    assert!(card.excerpt.contains("+beta"));

    input.diff = Some(diff_shown(crate::diff::Phase::TooLarge));
    assert_eq!(
        work_card(&input).excerpt,
        crate::locale::t("diff_too_large")
    );
}

#[test]
fn the_shown_mark_prefixes_the_meta_line_except_on_the_diff() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.shown_mark = Some("A");
    let card = work_card(&input);
    assert!(card.meta.starts_with("A  ·  "), "{}", card.meta);
    input.view = CardView::Diff;
    input.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let card = work_card(&input);
    assert!(!card.meta.starts_with("A"), "{}", card.meta);
}

#[test]
fn diff_labels_and_a_running_check_stay_on_the_meta_line() {
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.view = CardView::Diff;
    let mut shown = diff_shown(crate::diff::Phase::Same(Default::default()));
    shown.labels = crate::sensitivity::Labeling::Known(crate::sensitivity::Found {
        labels: vec![crate::sensitivity::Label::Pii],
    });
    shown.sensitive = true;
    input.diff = Some(shown);
    let card = work_card(&input);
    assert!(card.meta.contains("PII"), "{}", card.meta);
    assert!(!crate::commands::is_checking(&card));

    let mut shown = diff_shown(crate::diff::Phase::Working);
    shown.labels = crate::sensitivity::Labeling::Checking;
    input.diff = Some(shown);
    let card = work_card(&input);
    assert!(crate::commands::is_checking(&card), "{}", card.meta);
}

#[test]
fn a_diff_has_its_own_content_key_and_keeps_the_card_open() {
    let mut left = data(SubjectKind::Text, Some("hello"));
    left.view = CardView::Diff;
    left.diff = Some(diff_shown(crate::diff::Phase::Same(Default::default())));
    let mut right = left.clone();
    right.diff.as_mut().expect("diff").token = 8;
    assert_ne!(content_key(&left), content_key(&right));
    assert_ne!(
        content_key(&left),
        content_key(&data(SubjectKind::Text, Some("hello")))
    );
    assert!(keeps_card_open(&CommandId::Diff));
    assert!(keeps_card_open(&CommandId::DiffToggle(1)));
    assert!(keeps_card_open(&CommandId::DiffShown));
    assert!(keeps_card_open(&CommandId::DiffSwap));
}
