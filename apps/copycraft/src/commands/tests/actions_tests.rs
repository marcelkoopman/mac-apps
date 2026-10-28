use super::*;

#[test]
fn messy_json_hides_format_convert_and_dataframe() {
    let input = data(SubjectKind::Text, Some(r#"{"name":"copycraft"}"#));
    let shown = chips(&input);
    assert_eq!(shown[0].id, CommandId::TextMenu);
    assert_eq!(shown.last().unwrap().id, CommandId::Schema);
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Copy));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Save));
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Dataframe));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Decode));
    assert!(shown.iter().all(|cmd| !housekeeping(&cmd.id)));
}

#[test]
fn schema_view_shows_the_avro_schema() {
    let src = r#"{"name":"copycraft","version":1,"ok":true}"#;
    let mut input = data(SubjectKind::Text, Some(src));
    assert_eq!(
        ids(&matching(&search_pool(&input), "avro")),
        vec![CommandId::Schema]
    );
    input.view = CardView::Schema;
    let card = work_card(&input);
    let body = transformed_text(src, CardView::Schema).unwrap();
    assert_eq!(card.title, "Avro schema");
    assert_eq!(copy_tip(&card.title), "Copy Avro schema");
    assert_eq!(save_tip(&card.title), "Save Avro schema");
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    assert_eq!(card.excerpt, body);
    assert!(card.excerpt.contains("\"type\": \"record\""));
    assert!(card.excerpt.contains("\"name\": \"document\""));
    assert!(card.excerpt.contains("\"version\""));
    assert!(!card.excerpt.contains("copycraft"));
    assert!(!card.excerpt.ends_with('…'));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    assert!(card.selectable);
    let file = text_save_file(src, CardView::Schema).unwrap();
    assert_eq!(file.filename, "clipboard.avsc");
    assert_eq!(file.extension, "avsc");
    assert_eq!(file.bytes, body.as_bytes());
}

#[test]
fn xsd_opens_formatted_and_coloured_with_sample() {
    let src = r#"<?xml version="1.0"?><xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="person"><xs:complexType><xs:sequence><xs:element name="name" type="xs:string"/><xs:element name="age" type="xs:int"/></xs:sequence><xs:attribute name="id" type="xs:int" use="required"/></xs:complexType></xs:element></xs:schema>"#;
    let mut input = data(SubjectKind::Text, Some(src));
    assert_eq!(
        ids(&matching(&search_pool(&input), "sample")),
        vec![CommandId::Sample]
    );
    let shown = chips(&input);
    assert_eq!(ids(&shown), vec![CommandId::TextMenu, CommandId::Sample]);
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Format));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Schema));
    assert!(!shown.iter().any(|cmd| cmd.id == CommandId::Convert));
    assert_eq!(presented_view(src, CardView::Original), CardView::Format);
    let card = work_card(&input);
    let pretty = transformed_text(src, CardView::Format).unwrap();
    assert_ne!(pretty, src);
    assert_eq!(card.excerpt, pretty);
    assert!(card.excerpt.contains('\n'));
    assert!(card.excerpt.contains("<xs:schema"));
    assert!(!card.excerpt.ends_with('…'));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
    assert_eq!(card.title, "Valid XML");
    input.view = CardView::Sample;
    let card = work_card(&input);
    let body = transformed_text(src, CardView::Sample).unwrap();
    assert_eq!(card.title, "Sample XML");
    assert_eq!(copy_tip(&card.title), "Copy Sample XML");
    assert_eq!(save_tip(&card.title), "Save Sample XML");
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    assert_eq!(card.excerpt, body);
    assert!(card.excerpt.contains("<person"));
    assert!(card.excerpt.contains("<name>sample</name>"));
    assert!(card.excerpt.contains("<age>1</age>"));
    assert!(!card.excerpt.ends_with('…'));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Xml));
    assert!(card.selectable);
    let file = text_save_file(src, CardView::Sample).unwrap();
    assert_eq!(file.filename, "clipboard.xml");
    assert_eq!(file.extension, "xml");
    assert_eq!(file.bytes, body.as_bytes());
}

#[test]
fn long_prose_keeps_copy_and_save_on_the_content() {
    let text = "please summarize these notes for the team. ".repeat(40);
    let input = data(SubjectKind::Text, Some(&text));
    assert_eq!(ids(&chips(&input)), vec![CommandId::TextMenu]);
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
}

#[test]
fn image_card_is_a_picture_with_encoding_chips() {
    let mut input = data(SubjectKind::Image, None);
    input.image = Some(ImageFacts {
        format: "PNG".into(),
        width: 1280,
        height: 720,
        byte_len: 184_000,
    });
    let card = work_card(&input);
    assert_eq!(card.title, "Image");
    assert_eq!(card.meta, "PNG  1280×720  0.18 MB");
    assert!(card.shows_image);
    assert!(card.excerpt.is_empty());
    assert_eq!(titles(&chips(&input)), vec!["Info"]);
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: false,
            save: true,
            preview: true,
        }
    );
    input.view = CardView::Info;
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: false,
            save: true,
            preview: false,
        }
    );
    let info = work_card(&input);
    assert_eq!(info.title, "Image info");
    assert!(!info.shows_image);
    assert!(info.selectable);
    assert!(info.excerpt.contains("1280×720"));
}

#[test]
fn a_picture_opens_in_preview() {
    let input = data(SubjectKind::Image, None);
    assert!(content_actions(&input).preview);
    assert!(!keeps_card_open(&CommandId::OpenPreview));
    let found = matching(&search_pool(&input), "voorvertoning");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, CommandId::OpenPreview);
    assert_eq!(found[0].title, crate::locale::t("open_preview"));
    assert_eq!(found[0].detail, crate::locale::t("detail_open_preview"));
    let mut info = input.clone();
    info.view = CardView::Info;
    assert!(!content_actions(&info).preview);
    assert!(matching(&search_pool(&info), "preview").is_empty());
    let text = data(SubjectKind::Text, Some("hello"));
    assert!(!content_actions(&text).preview);
    let mut decoded = text;
    decoded.view = CardView::Decode;
    decoded.picture = crate::clipboard::SecretBytes::new(vec![1]);
    assert!(content_actions(&decoded).preview);
    assert!(
        matching(&search_pool(&decoded), "preview")
            .iter()
            .any(|cmd| cmd.id == CommandId::OpenPreview)
    );
}

#[test]
fn plain_text_offers_copy_and_save_on_the_content() {
    let input = data(SubjectKind::Text, Some("Emmi Silvennoinen"));
    assert_eq!(ids(&chips(&input)), vec![CommandId::TextMenu]);
    let card = work_card(&input);
    assert_eq!(copy_tip(&card.title), "Copy Content");
    assert_eq!(save_tip(&card.title), "Save Content");
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
}

#[test]
fn json_opens_formatted_and_coloured() {
    let src = r#"{"name":"copycraft","ok":true}"#;
    let input = data(SubjectKind::Text, Some(src));
    assert!(!chips(&input).iter().any(|cmd| cmd.id == CommandId::Format));
    assert_eq!(presented_view(src, CardView::Original), CardView::Format);
    let card = work_card(&input);
    let pretty = transformed_text(src, CardView::Format).unwrap();
    assert_ne!(pretty, src);
    assert_eq!(card.excerpt, pretty);
    assert!(card.excerpt.contains("\"ok\""));
    assert!(!card.excerpt.ends_with('…'));
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Json));
    assert_eq!(card.title, "Valid JSON");
    assert_eq!(copy_tip(&card.title), "Copy Valid JSON");
    assert_eq!(save_tip(&card.title), "Save Valid JSON");
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
}

#[test]
fn image_scan_adds_the_actions_from_the_old_preview() {
    let mut input = data(SubjectKind::Image, None);
    let url = format!("data:image/png;base64,{}", "a".repeat(80));
    input.image_scan = Some(ImageScan {
        info: "Image\nSize: 10×10".into(),
        data_url: Some(url.clone()),
        ocr: Some("hello".into()),
        qr: Some("https://example.com".into()),
    });
    // After OCR one toggle, named after where it goes; no Original chip.
    assert_eq!(titles(&chips(&input)), vec!["Info", "Show Text", "QR"]);
    let mut on_text = input.clone();
    on_text.view = CardView::Ocr;
    let toggle = chips(&on_text);
    assert_eq!(titles(&toggle), vec!["Info", "Show Image", "QR"]);
    assert_eq!(toggle[1].id, CommandId::Original);
    assert_eq!(chips(&input)[1].id, CommandId::Ocr);
    let mut no_ocr = input.clone();
    no_ocr.image_scan.as_mut().unwrap().ocr = None;
    assert_eq!(titles(&chips(&no_ocr)), vec!["Info", "QR"]);
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: false,
            save: true,
            preview: true,
        }
    );
    input.view = CardView::Info;
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    assert_eq!(copy_tip("Image info"), "Copy Image info");
    input.view = CardView::Ocr;
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    input.view = CardView::Qr;
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: true,
            save: true,
            preview: false,
        }
    );
    input.view = CardView::Info;
    let info = work_card(&input);
    assert_eq!(info.title, "Image info");
    assert!(!info.shows_image);
    assert!(info.selectable);
    assert!(info.excerpt.contains("10×10"));
    assert!(info.excerpt.contains(&url));
}

#[test]
fn youtube_url_is_a_link_card() {
    let url = "https://www.youtube.com/watch?v=bEN9Dyg48b0";
    let card = work_card(&data(SubjectKind::Text, Some(url)));
    assert_eq!(card.title, "YouTube");
    assert_eq!(card.link_page.as_deref(), Some(url));
    assert_eq!(card.excerpt.lines().next(), Some("youtube.com"));
    assert!(card.excerpt.contains("YouTube · Video"), "{}", card.excerpt);
    assert!(card.selectable);
    assert!(!card.shows_image);
    let input = data(SubjectKind::Text, Some(url));
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
fn file_urls_are_link_cards() {
    // A copy that is one URL is a Link card, also when it points at a file.
    let pdf = data(SubjectKind::Text, Some("https://example.com/report.pdf"));
    let card = work_card(&pdf);
    assert_eq!(card.title, "Link");
    assert_eq!(
        card.link_page.as_deref(),
        Some("https://example.com/report.pdf")
    );
    assert!(
        card.excerpt.starts_with("example.com\n/report.pdf"),
        "{}",
        card.excerpt
    );
    assert_eq!(ids(&chips(&pdf))[0], CommandId::Visit);
    assert_eq!(
        content_actions(&pdf),
        ContentActions {
            copy: true,
            save: false,
            preview: false,
        }
    );
    // A `file://` link has a card but nothing for the browser.
    let local = data(SubjectKind::Text, Some("file:///Users/someone/notes.txt"));
    assert_eq!(work_card(&local).title, "Link");
    assert!(!ids(&chips(&local)).contains(&CommandId::Visit));
}

#[test]
fn empty_clipboard_has_no_chips_and_keeps_quit_in_the_menu() {
    let input = data(SubjectKind::Empty, None);
    assert!(chips(&input).is_empty());
    let card = work_card(&input);
    // History off (the default of a new install): the card asks for a paste.
    assert_eq!(card.placeholder, crate::locale::t("paste_here_hint"));
    assert!(card.excerpt.is_empty());
    let mut watching = data(SubjectKind::Empty, None);
    watching.settings.clipboard_history = true;
    assert_eq!(
        work_card(&watching).placeholder,
        crate::locale::t("empty_history"),
    );
    // Both empty cards link to Settings › Privacy; other placeholders do not.
    assert!(crate::commands::offers_privacy_settings(&card.placeholder));
    assert!(crate::commands::offers_privacy_settings(
        &work_card(&watching).placeholder
    ));
    assert!(!crate::commands::offers_privacy_settings(crate::locale::t(
        "private_copy_note"
    )));
    assert_eq!(
        content_actions(&input),
        ContentActions {
            copy: false,
            save: false,
            preview: false,
        }
    );
    let quiet = data(SubjectKind::NoText, None);
    assert_eq!(
        content_actions(&quiet),
        ContentActions {
            copy: false,
            save: false,
            preview: false,
        }
    );
    let menu = overflow(&input);
    assert_eq!(menu.last().unwrap().id, CommandId::Quit);
    assert!(menu.iter().all(|cmd| cmd.id != CommandId::Copy));
    assert!(menu.iter().all(|cmd| cmd.id != CommandId::Save));
    assert!(matching(&search_pool(&input), "quit").is_empty());
}

#[test]
fn image_menu_is_off_on_the_recognized_text() {
    let mut input = data(SubjectKind::Image, None);
    input.image_scan = Some(ImageScan {
        info: "Image".into(),
        data_url: None,
        ocr: Some("hello".into()),
        qr: None,
    });
    input.image_edit = Some(crate::commands::ImageShown {
        entry: 1,
        label: None,
        can_undo: false,
        can_redo: false,
        working: false,
        note: None,
    });
    let menu = |input: &crate::commands::LaunchData| {
        chips(input)
            .into_iter()
            .find(|c| c.id == CommandId::ImageMenu)
            .unwrap()
    };
    assert!(menu(&input).enabled);
    input.view = CardView::Ocr;
    let off = menu(&input);
    assert!(!off.enabled);
    assert_eq!(off.detail, "Image view only");
    // The step itself is refused there too (a search or a stale menu), with no switch.
    assert!(!crate::commands::image_steps_allowed(CardView::Ocr));
    assert!(crate::commands::image_steps_allowed(CardView::Original));
    assert!(crate::commands::image_steps_allowed(CardView::Info));
}

#[test]
fn an_svg_is_a_picture_with_a_code_toggle_and_its_size() {
    let svg = r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="48" height="24"><rect width="4" height="4"/></svg>"#;
    let mut input = data(SubjectKind::Text, Some(svg));
    // Not drawable (no picture): the highlighted code, titled SVG, no toggle.
    let card = crate::commands::work_card(&input);
    assert_eq!(card.title, "SVG");
    assert!(!card.shows_image);
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Svg));
    assert!(card.meta.starts_with("48 × 24 · "), "{}", card.meta);
    assert!(!chips(&input).iter().any(|c| c.title == "Show Code"));
    // Drawn: the picture, with Show code.
    input.picture = crate::clipboard::SecretBytes::new(svg.as_bytes().to_vec());
    let card = crate::commands::work_card(&input);
    assert!(card.shows_image);
    assert_eq!(card.title, "SVG");
    assert!(card.meta.starts_with("48 × 24 · "), "{}", card.meta);
    let shown = chips(&input);
    assert!(shown.iter().any(|cmd| cmd.id == CommandId::TextMenu));
    let toggle = shown
        .iter()
        .find(|cmd| cmd.id == CommandId::Format)
        .expect("Show Code");
    assert_eq!(toggle.title.as_str(), "Show Code");
    // On the code: Show image goes back.
    input.view = CardView::Format;
    let card = crate::commands::work_card(&input);
    assert!(!card.shows_image);
    assert_eq!(card.title, "SVG");
    let shown = chips(&input);
    let toggle = shown
        .iter()
        .find(|cmd| cmd.id == CommandId::Original)
        .expect("Show Image");
    assert_eq!(toggle.title.as_str(), "Show Image");
}

#[test]
fn pause_ignore_next_and_ignore_app_are_greyed_out_without_history() {
    let ids = [
        CommandId::Pause,
        CommandId::IgnoreNext,
        CommandId::IgnoreFrontApp,
    ];
    let mut input = data(SubjectKind::Text, Some("hello"));
    input.can_ignore_app = true;
    input.settings.clipboard_history = false;
    let off = crate::commands::capture_commands(&input);
    assert_eq!(
        off.iter().map(|cmd| cmd.id.clone()).collect::<Vec<_>>(),
        ids
    );
    assert!(off.iter().all(|cmd| !cmd.enabled));
    input.settings.clipboard_history = true;
    let on = crate::commands::capture_commands(&input);
    assert!(on.iter().all(|cmd| cmd.enabled));
    // Ignore-frontmost still needs a foreign app.
    input.can_ignore_app = false;
    let none = crate::commands::capture_commands(&input);
    assert!(none[0].enabled && none[1].enabled && !none[2].enabled);
}

#[test]
fn keyed_tip_puts_the_shortcut_after_the_name() {
    assert_eq!(keyed_tip("Close", "Esc"), "Close (Esc)");
    assert_eq!(keyed_tip(&save_tip("JSON"), "⌘S"), "Save JSON (⌘S)");
}
