//! The Display text of every error enum (thiserror): the card's NL/EN texts stay exactly what
//! the hand-written impls gave, and the fixed English texts hold no content.

use crate::locale::{t, tf};

#[test]
fn table_error_display_is_the_card_text() {
    use crate::table::{MAX_VERSIONS, TableError};
    assert_eq!(
        TableError::Full.to_string(),
        tf("max_versions", &[&MAX_VERSIONS])
    );
    assert_eq!(TableError::NotATable.to_string(), t("not_a_table"));
    assert_eq!(TableError::Cancelled.to_string(), t("cancelled"));
    assert_eq!(
        TableError::Failed("fake message".into()).to_string(),
        "fake message"
    );
    assert_eq!(TableError::Unchanged("fake note").to_string(), "fake note");
}

#[test]
fn image_error_display_is_the_card_text() {
    use crate::image_edit::ImageError;
    assert_eq!(ImageError::Cancelled.to_string(), t("cancelled"));
    assert_eq!(
        ImageError::Failed("fake message".into()).to_string(),
        "fake message"
    );
    assert_eq!(ImageError::Unchanged("fake note").to_string(), "fake note");
}

#[test]
fn docx_error_display_is_fixed() {
    use crate::docx::DocxError as E;
    let all = [
        (E::NotDocx, "not a Word document"),
        (E::Locked, "the Word document is password protected"),
        (E::Unreadable, "the Word document cannot be read"),
        (E::Bomb, "the Word document unpacks too large"),
        (E::TooLarge, "the text is too large"),
        (E::NoText, "the Word document has no text"),
    ];
    for (error, text) in all {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn zip_error_display_is_fixed() {
    use crate::zip_archive::ZipError as E;
    let all = [
        (E::NotZip, "not a zip archive"),
        (E::Corrupt, "the zip archive is damaged"),
        (E::Locked, "the zip archive is password protected"),
        (
            E::Unsupported,
            "the zip archive uses an unsupported compression method",
        ),
        (E::Unsafe, "the zip archive has an unsafe entry"),
        (E::Empty, "the zip archive is empty"),
        (E::TooLarge, "the zip archive is too large"),
        (E::Read, "the zip archive cannot be read"),
        (E::Write, "writing to the chosen folder failed"),
        (E::NoSpace, "not enough free space"),
        (E::Cancelled, "unpacking was stopped"),
        (E::WorkFolderName, "an entry has the work folder name"),
    ];
    for (error, text) in all {
        assert_eq!(error.to_string(), text);
        // The card text is still the locale message, not Display.
        assert_ne!(error.message(), "");
    }
}

#[test]
fn json_style_error_display_is_fixed_and_note_unchanged() {
    use crate::text_ops::json_style::JsonStyleError as E;
    let broken = E::Broken(crate::validate::broken_json("{\"fake\": 1,}").expect("broken"));
    assert_eq!(broken.to_string(), "the JSON does not parse");
    assert!(!broken.to_string().contains("fake"));
    assert_eq!(E::DuplicateKeys.to_string(), "the JSON has a duplicate key");
    assert_eq!(E::DuplicateKeys.note(), t("json_duplicate_keys"));
    assert_eq!(E::LossyNumbers.to_string(), "a JSON number would change");
    assert_eq!(E::LossyNumbers.note(), t("json_numbers_lossy"));
}

#[cfg(target_os = "macos")]
#[test]
fn mailto_error_display_is_fixed() {
    use crate::macos_open::MailtoError as E;
    assert_eq!(E::Invalid.to_string(), "the mail link is not valid");
    assert_eq!(E::NoMailApp.to_string(), "no mail app is set up");
    assert_eq!(
        E::NotOpened.to_string(),
        "the mail app could not open the link"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn pasteboard_write_error_display_is_the_old_text() {
    use crate::macos_pasteboard::WriteError as E;
    assert_eq!(E::Text.to_string(), "could not write text");
    assert_eq!(E::Image.to_string(), "could not write image");
    // diag maps it the same as the old String: no code in it.
    assert_eq!(
        crate::diag::ErrorKind::from_text(&E::Text.to_string()),
        crate::diag::ErrorKind::Other
    );
}
