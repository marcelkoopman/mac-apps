use zeroize::Zeroize;

struct ForceBlurOff;

impl ForceBlurOff {
    fn arm() -> Self {
        super::FAIL_BLUR.with(|cell| cell.set(true));
        Self
    }
}

impl Drop for ForceBlurOff {
    fn drop(&mut self) {
        super::FAIL_BLUR.with(|cell| cell.set(false));
    }
}

#[test]
fn header_title_is_the_kind_and_the_hotkey() {
    assert_eq!(super::header_title("Rust"), "Rust · ⌃⌘C");
    assert_eq!(super::header_title("Clipboard"), "Clipboard · ⌃⌘C");
    // Checks stay in the kind part; no date or time follows.
    let title = super::header_title("Java · missing }");
    assert_eq!(title, "Java · missing } · ⌃⌘C");
    assert!(!title.chars().any(|ch| ch.is_ascii_digit()), "{title}");
}

#[test]
fn well_radius_is_the_shared_eight() {
    assert_eq!(super::WELL_RADIUS, mac_ui::metrics::WELL_RADIUS);
    assert_eq!(super::WELL_RADIUS, mac_ui::metrics::SPACE_S);
    // The well shares the 16 pt margin, so this is not the clamped concentric radius.
    assert_ne!(
        super::WELL_RADIUS,
        mac_ui::corners::concentric_radius(super::PANEL_RADIUS, super::PAD)
    );
}

#[test]
fn text_and_pictures_get_their_own_blur() {
    // Light enough to show the shape, the picture's blur stronger than the text's.
    assert_eq!(super::TEXT_BLUR_RADIUS, 6.0);
    assert_eq!(super::IMAGE_BLUR_RADIUS, 10.0);
    assert!(super::gaussian_blurs().is_some());
}

#[test]
fn blur_filters_are_built_once() {
    let (text, image) = super::gaussian_blurs().expect("blurs");
    let (again_text, again_image) = super::gaussian_blurs().expect("blurs");
    let ptr = super::Retained::as_ptr;
    assert_eq!(ptr(&text), ptr(&again_text));
    assert_eq!(ptr(&image), ptr(&again_image));
    assert_ne!(ptr(&text), ptr(&image));
}

#[test]
fn blur_hook_fails_closed() {
    let _force = ForceBlurOff::arm();
    assert!(super::gaussian_blurs().is_none());
    let masked = crate::commands::well_mask(true, false);
    assert!(masked.shade);
    assert!(!masked.show_body);
    assert!(!masked.blur);
    let revealed = crate::commands::well_mask(false, false);
    assert!(revealed.show_body);
    assert!(!revealed.shade);
}

#[test]
fn failed_blur_covers_the_well_until_reveal() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            super::MASKS.set(false);
            super::REVEALED.set(false);
            super::SHADE_ON.set(false);
            super::BLUR_ON.set(false);
        }
    }
    let _force = ForceBlurOff::arm();
    let _reset = Reset;
    super::MASKS.set(true);
    super::REVEALED.set(false);
    super::BLUR_ON.set(false);
    super::SHADE_ON.set(false);
    super::finish_preview();
    assert!(super::SHADE_ON.get());
    assert!(!super::BLUR_ON.get());
    super::REVEALED.set(true);
    super::finish_preview();
    assert!(!super::SHADE_ON.get());
    assert!(!super::BLUR_ON.get());
    super::REVEALED.set(false);
    super::finish_preview();
    assert!(super::SHADE_ON.get());
}

struct ResetFind;

impl ResetFind {
    fn arm() -> Self {
        Self
    }
}

impl Drop for ResetFind {
    fn drop(&mut self) {
        super::MASKS.set(false);
        super::DIFF_LOCK.set(false);
        super::CARD_DIFF.set(false);
        super::DIFF_WIDE.set(false);
        super::REVEALED.set(false);
        super::FIND_ON.set(false);
        super::FIND_INDEX.set(0);
        super::ITEM_QUERY.with(|slot| slot.borrow_mut().zeroize());
        super::ITEM_TEXT.with(|slot| slot.borrow_mut().zeroize());
        super::CONTENT_KEY.set(0);
        super::OPEN.set(false);
        super::SEARCHING.set(false);
        super::SHADE_ON.set(false);
        super::BLUR_ON.set(false);
    }
}

fn copied(text: &str) -> crate::commands::LaunchData {
    crate::commands::LaunchData {
        subject_kind: crate::commands::SubjectKind::Text,
        subject_text: Some(text.to_string()),
        image: None,
        history: Vec::new(),
        can_clear_history: false,
        can_clear_pins: false,
        paused: false,
        ignore_next: false,
        can_ignore_app: false,
        notice: None,
        history_nav: None,
        strip: Vec::new(),
        strip_cursor: 0,
        theme: crate::appearance::Theme::System,
        settings: crate::settings::Settings::default(),
        view: crate::commands::CardView::Original,
        image_scan: None,
        pdf: None,
        pdf_page: 0,
        pdf_zoom: crate::pdf::ZOOM_FIT,
        pdf_zoom_picture: None,
        document_preview: false,
        source_name: None,
        source_note: None,
        full: false,
        picture: None,
        table: None,
        image_edit: None,
        text_edit: None,
        shown_id: None,
        shown_mark: None,
        diff_picks: Vec::new(),
        diff: None,
    }
}

fn assert_find(on: bool) {
    assert_eq!(super::FIND_ON.get(), on);
    super::ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            assert_eq!(field.isHidden(), !on);
        }
    });
}

fn assert_query_empty() {
    assert!(super::item_query().is_empty());
    super::ITEM_FIELD.with(|slot| {
        if let Some(field) = slot.borrow().as_ref() {
            assert!(field.stringValue().to_string().is_empty());
        }
    });
}

fn picture() -> crate::commands::LaunchData {
    crate::commands::LaunchData {
        subject_kind: crate::commands::SubjectKind::Image,
        subject_text: None,
        image: Some(crate::commands::ImageFacts {
            format: "png".to_string(),
            width: 4,
            height: 4,
            byte_len: 64,
        }),
        history: Vec::new(),
        can_clear_history: false,
        can_clear_pins: false,
        paused: false,
        ignore_next: false,
        can_ignore_app: false,
        notice: None,
        history_nav: None,
        strip: Vec::new(),
        strip_cursor: 0,
        theme: crate::appearance::Theme::System,
        settings: crate::settings::Settings::default(),
        view: crate::commands::CardView::Original,
        image_scan: None,
        pdf: None,
        pdf_page: 0,
        pdf_zoom: crate::pdf::ZOOM_FIT,
        pdf_zoom_picture: None,
        document_preview: false,
        source_name: None,
        source_note: None,
        full: false,
        picture: None,
        table: None,
        image_edit: None,
        text_edit: None,
        shown_id: None,
        shown_mark: None,
        diff_picks: Vec::new(),
        diff: None,
    }
}

#[test]
fn a_sensitive_diff_stays_masked_with_the_filter_off() {
    let _reset = ResetFind::arm();
    super::DIFF_LOCK.set(true);
    super::REVEALED.set(false);
    super::MASKS.set(false);
    assert!(super::well_is_masked());
    super::REVEALED.set(true);
    assert!(!super::well_is_masked());
}

#[test]
fn find_stays_off_while_the_well_is_masked() {
    let _reset = ResetFind::arm();
    let _force = ForceBlurOff::arm();
    super::FIND_ON.set(true);
    super::MASKS.set(true);
    super::REVEALED.set(false);
    super::finish_preview();
    assert_find(false);
}

#[test]
fn find_turns_on_after_reveal() {
    let _reset = ResetFind::arm();
    let _force = ForceBlurOff::arm();
    super::MASKS.set(true);
    super::REVEALED.set(false);
    super::finish_preview();
    super::FIND_ON.set(false);
    super::REVEALED.set(true);
    super::finish_preview();
    assert_find(true);
}

#[test]
fn find_turns_off_after_mask_new_item_wipe_or_close() {
    let _reset = ResetFind::arm();
    let _force = ForceBlurOff::arm();
    let first = copied("correct horse battery staple");
    let second = copied("another copy");
    super::store(first.clone());
    super::REVEALED.set(true);
    super::finish_preview();
    assert_find(true);
    super::set_item_query("horse");

    super::store(first);
    assert!(super::REVEALED.get());
    super::finish_preview();
    assert_find(true);
    assert_eq!(super::item_query(), "horse");

    super::REVEALED.set(false);
    super::finish_preview();
    assert_find(false);
    assert_query_empty();

    super::REVEALED.set(true);
    super::finish_preview();
    super::set_item_query("horse");
    super::store(second);
    super::finish_preview();
    assert!(!super::REVEALED.get());
    assert_find(false);
    assert_query_empty();

    super::REVEALED.set(true);
    super::finish_preview();
    super::set_item_query("horse");
    super::wipe_shown();
    assert_find(false);
    assert_query_empty();

    super::REVEALED.set(true);
    super::finish_preview();
    super::set_item_query("horse");
    super::OPEN.set(true);
    super::hide();
    assert_find(false);
    assert_query_empty();
    assert!(!super::OPEN.get());
}

#[test]
fn a_revealed_entry_stays_revealed_through_views_rebuilds_and_labels() {
    use crate::commands::{CardView, TableShown, work_card};
    let _reset = ResetFind::arm();
    let _force = ForceBlurOff::arm();
    let src = "Id;Naam;Salaris\n1;Jan;3450\n2;Anja;2900";
    let first = copied(src);
    super::store(first.clone());
    super::REVEALED.set(true);
    for view in [
        CardView::Dataframe,
        CardView::Format,
        CardView::Convert,
        CardView::Schema,
        CardView::Sample,
        CardView::Info,
        CardView::Original,
    ] {
        let mut shown = first.clone();
        shown.view = view;
        super::store(shown);
        assert!(super::REVEALED.get(), "{view:?}");
    }
    // The table's job running, then done: the card is rebuilt each time.
    let mut table = first.clone();
    table.view = CardView::Dataframe;
    table.table = Some(TableShown {
        frame: None,
        frame_id: 0,
        version: 1,
        labels: vec!["Original".into(), "Duplicates removed".into()],
        working: true,
        error: None,
        options: crate::dataframe::ReadOptions::default(),
        notes: None,
        overview: None,
    });
    super::store(table.clone());
    assert!(super::REVEALED.get());
    // A rebuilt card with other labels, or still checking them, is the same entry.
    for meta in ["3 lines  ·  PII", "3 lines", "3 lines  ·  Checking…"] {
        let mut card = work_card(&table);
        card.meta = meta.to_string();
        super::store_with_card(table.clone(), card);
        assert!(super::REVEALED.get(), "{meta}");
    }
    // Another entry is masked.
    super::store(copied("Id;Naam\n1;Piet"));
    assert!(!super::REVEALED.get());
}

#[test]
fn failed_blur_leaves_find_off() {
    let _reset = ResetFind::arm();
    let _force = ForceBlurOff::arm();
    super::MASKS.set(true);
    super::REVEALED.set(true);
    super::BLUR_ON.set(false);
    super::finish_preview();
    assert_find(true);
    super::set_item_query("horse");
    super::REVEALED.set(false);
    super::finish_preview();
    assert!(super::SHADE_ON.get());
    assert!(!super::BLUR_ON.get());
    assert_find(false);
    assert_query_empty();
}

#[test]
fn search_after_image_or_link_finds_no_old_text() {
    let _reset = ResetFind::arm();
    super::store(copied("correct horse battery staple"));
    assert_eq!(
        super::find_matches(&super::current_item_text(), "horse").len(),
        1
    );
    super::store(picture());
    assert!(super::current_item_text().is_empty());
    assert!(super::find_matches(&super::current_item_text(), "horse").is_empty());
    super::store(copied("https://example.com/docs"));
    assert!(super::current_item_text().is_empty());
    assert!(super::find_matches(&super::current_item_text(), "horse").is_empty());
    assert!(super::find_matches(&super::current_item_text(), "example").is_empty());
}
