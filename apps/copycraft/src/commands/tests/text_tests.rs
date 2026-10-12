use super::*;

#[test]
fn dataframe_view_of_a_phone_csv_stays_up() {
    let src = "\
Id,Naam,Telefoonnummer,Salaris
1,Jan de Vries,06-12345678,3450
2,Anja Bakker,06-87654321,2900";
    let body = transformed_text(src, CardView::Dataframe).expect("dataframe");
    assert!(body.contains('┆'));
    assert!(body.contains("Jan de Vries"));
    let mut input = data(SubjectKind::Text, Some(src));
    input.view = CardView::Dataframe;
    let card = work_card(&input);
    assert_eq!(card.title, "Table");
    assert_eq!(card.highlight, Some(crate::format::FormatKind::Dataframe));
    assert!(card.selectable);
    assert!(card.excerpt.contains("┌"), "{}", card.excerpt);
    assert!(card.excerpt.contains("Jan de Vries"), "{}", card.excerpt);
    assert!(card.excerpt.contains("Anja Bakker"), "{}", card.excerpt);
    assert!(card.excerpt.contains("Telefoonnummer"), "{}", card.excerpt);
    assert!(card.excerpt.contains("Salaris"), "{}", card.excerpt);
    assert!(card.excerpt.contains('┆'), "{}", card.excerpt);
    assert!(card.meta.contains("PII · financial"), "{}", card.meta);
}

#[test]
fn save_in_the_dataframe_view_writes_csv() {
    let tsv = "Naam\tWanneer\nJan\t13/03/2026\nPiet\t14/03/2026";
    let file = text_save_file(tsv, CardView::Dataframe).expect("file");
    assert_eq!(
        (file.filename.as_str(), file.extension),
        ("clipboard.csv", "csv")
    );
    assert_eq!(
        std::str::from_utf8(&file.bytes).expect("UTF-8"),
        "Naam,Wanneer\nJan,2026-03-13\nPiet,2026-03-14\n"
    );
    assert_eq!(
        deferred_save_name(tsv, CardView::Dataframe),
        Some(("clipboard.csv".to_string(), "csv"))
    );
    assert_eq!(
        crate::open_file::save_name("export.tsv", "csv"),
        "export.csv"
    );
}

#[test]
fn deferred_save_name_matches_the_built_file() {
    let sources = [
        "{\"name\":\"copycraft\",\"n\":3}",
        "name: copycraft\nn: 3\n",
        "<a><b>x</b></a>",
        "# Title\n\n- item\n",
        "name,age\nalice,30\nbob,40",
        "plain words",
    ];
    for src in sources {
        for view in [CardView::Original, CardView::Format, CardView::Dataframe] {
            let Some(file) = text_save_file(src, view) else {
                continue;
            };
            let (filename, extension) = deferred_save_name(src, view).unwrap();
            assert_eq!(filename, file.filename, "{view:?} {src:?}");
            assert_eq!(extension, file.extension, "{view:?} {src:?}");
        }
    }
    assert!(deferred_save_name("{}", CardView::Convert).is_none());
    assert!(deferred_save_name("{}", CardView::Schema).is_none());
}

#[test]
fn copy_and_save_use_the_whole_table_not_the_preview() {
    let src = csv_rows(300);
    let save = text_save_file(&src, CardView::Original).unwrap();
    assert_eq!(save.bytes, src.as_bytes());
    let copied = transformed_text(&src, CardView::Original).unwrap();
    assert_eq!(copied, src);
}
