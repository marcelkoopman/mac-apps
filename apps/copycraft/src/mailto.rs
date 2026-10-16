//! The feedback mail: the pure part. It builds the subject and body of a feature suggestion or a
//! problem report, the `mailto:` link, the preview and the clipboard text.
//!
//! Privacy: the body holds only what the user typed, plus the app version and the macOS version.
//! Never clipboard content, history, file names, paths or settings. Nothing here sends anything:
//! the link opens the user's own mail app, and the user sends the mail.

/// Where the mail goes.
pub const SUPPORT_ADDRESS: &str = "mac-apps-support@proton.me";

/// The longest `mailto:` link we hand to the system, in characters of the encoded link. Mail
/// apps and `NSWorkspace` cope with more, but some drop or cut a very long link; longer text goes
/// to the clipboard instead.
pub const MAX_MAILTO_LEN: usize = 2000;

/// What the user wants to tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A feature suggestion.
    Feature,
    /// A problem report.
    Problem,
}

impl Kind {
    /// The start of the subject line (Dutch in both languages: the mail goes to the developer).
    pub fn subject_prefix(self) -> &'static str {
        match self {
            Kind::Feature => "Feature voorstel: ",
            Kind::Problem => "Probleem: ",
        }
    }
}

/// The two version lines that end every body. They are the only text that is not the user's.
pub const APP_VERSION_LABEL: &str = "Copycraft versie: ";
pub const MACOS_VERSION_LABEL: &str = "macOS versie: ";

/// A mail as it will be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub subject: String,
    pub body: String,
}

/// Collapse every run of whitespace (newlines too) to one space and trim: a subject is one line,
/// so typed text can never add a header.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Line endings as `\n`, no NUL or other control characters except tab and newline, trimmed.
fn clean_block(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let kept: String = unified
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    kept.trim().to_string()
}

/// Whether there is anything to send: a subject or a text.
pub fn has_content(subject: &str, first: &str, second: &str) -> bool {
    [subject, first, second]
        .iter()
        .any(|part| !part.trim().is_empty())
}

/// The mail for the typed `subject` and the two answers (what / why, or what went wrong / what
/// was expected). The body is the answers, then a blank line and the two version lines.
pub fn compose(
    kind: Kind,
    subject: &str,
    first: &str,
    second: &str,
    app_version: &str,
    macos_version: &str,
) -> Mail {
    let subject = one_line(subject);
    let mut blocks: Vec<String> = [first, second]
        .iter()
        .map(|text| clean_block(text))
        .filter(|text| !text.is_empty())
        .collect();
    blocks.push(format!(
        "{APP_VERSION_LABEL}{}\n{MACOS_VERSION_LABEL}{}",
        one_line(app_version),
        one_line(macos_version)
    ));
    Mail {
        subject: format!("{}{subject}", kind.subject_prefix())
            .trim_end()
            .to_string(),
        body: blocks.join("\n\n"),
    }
}

/// Percent-encode `text` for a `mailto:` link: letters, digits and `-._~` stay; every other byte
/// of the UTF-8 text (space, `& = ? # + %`, non-ASCII) becomes `%XX`; a line break of any kind
/// becomes `%0D%0A`; NUL is dropped.
pub fn percent_encode(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(unified.len() * 3);
    for byte in unified.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            b'\n' => out.push_str("%0D%0A"),
            0 => {}
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The `mailto:` link for `mail`, or `None` when it is longer than [`MAX_MAILTO_LEN`].
pub fn mailto_url(mail: &Mail) -> Option<String> {
    let url = format!(
        "mailto:{SUPPORT_ADDRESS}?subject={}&body={}",
        percent_encode(&mail.subject),
        percent_encode(&mail.body)
    );
    (url.chars().count() <= MAX_MAILTO_LEN).then_some(url)
}

/// What "Kopieer naar klembord" puts on the clipboard: the whole mail as plain text, for the user
/// to paste into a mail app.
pub fn clipboard_text(mail: &Mail) -> String {
    format!(
        "{SUPPORT_ADDRESS}\n{}\n\n{}",
        mail.subject.trim_end(),
        mail.body
    )
}

/// The preview under the form: recipient, subject and body, each with the caller's label.
pub fn preview_text(mail: &Mail, to: &str, subject: &str) -> String {
    format!(
        "{to} {SUPPORT_ADDRESS}\n{subject} {}\n\n{}",
        mail.subject, mail.body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(first: &str, second: &str) -> Mail {
        compose(
            Kind::Feature,
            "Titel",
            first,
            second,
            "1.2.3",
            "Version 15.0",
        )
    }

    #[test]
    fn encodes_special_characters() {
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("a&b=c?d#e+f%g"), "a%26b%3Dc%3Fd%23e%2Bf%25g");
        assert_eq!(percent_encode("-._~Az09"), "-._~Az09");
        assert_eq!(percent_encode("a/b:c@d,e;f"), "a%2Fb%3Ac%40d%2Ce%3Bf");
        assert_eq!(percent_encode("\"<>'"), "%22%3C%3E%27");
    }

    #[test]
    fn newlines_become_crlf() {
        assert_eq!(percent_encode("a\nb"), "a%0D%0Ab");
        assert_eq!(percent_encode("a\r\nb"), "a%0D%0Ab");
        assert_eq!(percent_encode("a\rb"), "a%0D%0Ab");
        assert_eq!(percent_encode("a\n\nb"), "a%0D%0A%0D%0Ab");
    }

    #[test]
    fn unicode_is_utf8_percent_encoded() {
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode("€"), "%E2%82%AC");
        assert_eq!(percent_encode("😀"), "%F0%9F%98%80");
        assert_eq!(percent_encode("⌘"), "%E2%8C%98");
    }

    #[test]
    fn nul_is_dropped_and_other_controls_are_escaped() {
        assert_eq!(percent_encode("a\0b"), "ab");
        assert_eq!(percent_encode("a\tb"), "a%09b");
    }

    #[test]
    fn subject_prefix_per_kind() {
        let feature = compose(Kind::Feature, "Dark mode", "", "", "1", "2");
        assert_eq!(feature.subject, "Feature voorstel: Dark mode");
        let problem = compose(Kind::Problem, "Crash", "", "", "1", "2");
        assert_eq!(problem.subject, "Probleem: Crash");
        assert_eq!(
            compose(Kind::Problem, "  ", "", "", "1", "2").subject,
            "Probleem:"
        );
    }

    #[test]
    fn subject_is_one_line() {
        let m = compose(Kind::Feature, "a\r\nBcc: x@y.z\n b", "", "", "1", "2");
        assert_eq!(m.subject, "Feature voorstel: a Bcc: x@y.z b");
        assert!(!m.subject.contains('\n'));
    }

    #[test]
    fn body_is_the_text_plus_the_two_version_lines_only() {
        let m = mail("Ik wil X", "Omdat Y");
        assert_eq!(
            m.body,
            "Ik wil X\n\nOmdat Y\n\nCopycraft versie: 1.2.3\nmacOS versie: Version 15.0"
        );
        // Take the user's text and the version lines away: nothing is left.
        let rest = m
            .body
            .replace("Ik wil X", "")
            .replace("Omdat Y", "")
            .replace("Copycraft versie: 1.2.3", "")
            .replace("macOS versie: Version 15.0", "");
        assert!(rest.trim().is_empty(), "{rest:?}");
    }

    #[test]
    fn empty_answers_leave_only_the_versions() {
        let m = mail("", "  ");
        assert_eq!(
            m.body,
            "Copycraft versie: 1.2.3\nmacOS versie: Version 15.0"
        );
        let one = mail("", "Alleen dit");
        assert_eq!(
            one.body,
            "Alleen dit\n\nCopycraft versie: 1.2.3\nmacOS versie: Version 15.0"
        );
    }

    #[test]
    fn has_content_needs_some_text() {
        assert!(!has_content(" ", "\n", ""));
        assert!(has_content("a", "", ""));
        assert!(has_content("", "", "b"));
    }

    #[test]
    fn link_has_the_address_subject_and_body() {
        let url = mailto_url(&mail("a&b", "c\nd")).unwrap();
        assert!(url.starts_with(
            "mailto:mac-apps-support@proton.me?subject=Feature%20voorstel%3A%20Titel&body="
        ));
        assert!(url.contains("a%26b%0D%0A%0D%0Ac%0D%0Ad"));
        assert!(
            url.contains("Copycraft%20versie%3A%201.2.3%0D%0AmacOS%20versie%3A%20Version%2015.0")
        );
        // Only one `?`, and the two separators are the only raw `&` and `=` pairs we made.
        assert_eq!(url.matches('?').count(), 1);
        assert_eq!(url.matches('&').count(), 1);
        assert_eq!(url.matches('=').count(), 2);
    }

    #[test]
    fn link_limit() {
        let fits = "x".repeat(MAX_MAILTO_LEN - 200);
        assert!(mailto_url(&mail(&fits, "")).is_some());
        let long = "x".repeat(MAX_MAILTO_LEN);
        assert!(mailto_url(&mail(&long, "")).is_none());
        // The limit counts the encoded text: 700 euro signs are 4200 characters.
        assert!(mailto_url(&mail(&"€".repeat(700), "")).is_none());
        // The longest text that still fits ends exactly at the limit; one more letter does not.
        let len_for = |n: usize| {
            let m = compose(
                Kind::Problem,
                "t",
                &"x".repeat(n),
                "",
                "1.2.3",
                "Version 15.0",
            );
            (mailto_url(&m).is_some(), m)
        };
        let best = (0..MAX_MAILTO_LEN)
            .take_while(|n| len_for(*n).0)
            .last()
            .unwrap();
        let (_, at_limit) = len_for(best);
        assert_eq!(
            mailto_url(&at_limit).unwrap().chars().count(),
            MAX_MAILTO_LEN
        );
        assert!(!len_for(best + 1).0);
    }

    #[test]
    fn clipboard_text_has_the_whole_mail() {
        let text = clipboard_text(&mail("Ik wil X", ""));
        assert!(
            text.starts_with("mac-apps-support@proton.me\nFeature voorstel: Titel\n\nIk wil X")
        );
        assert!(text.ends_with("macOS versie: Version 15.0"));
    }

    #[test]
    fn preview_shows_what_will_be_created() {
        let text = preview_text(&mail("Ik wil X", ""), "Aan:", "Onderwerp:");
        assert!(text.starts_with(
            "Aan: mac-apps-support@proton.me\nOnderwerp: Feature voorstel: Titel\n\nIk wil X"
        ));
    }

    #[test]
    fn user_text_is_cleaned() {
        let m = compose(Kind::Problem, "s", "a\r\nb\0c\u{7}d", "", "1", "2");
        assert!(m.body.starts_with("a\nbcd\n\n"), "{:?}", m.body);
    }
}
