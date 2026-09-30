use std::sync::OnceLock;

use leakguard::{Kind, Match, Redactor};

use crate::format::FormatKind;

/// Clipboard categories shown on the card. Order is the meta-line order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    Credential,
    Pii,
    Financial,
}

impl Label {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Credential => "credential",
            Self::Pii => "PII",
            Self::Financial => "financial",
        }
    }
}

pub fn label_line(labels: &[Label]) -> String {
    labels
        .iter()
        .map(|label| label.name())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// One sensitivity name inside a meta line. Indexes are byte offsets.
pub struct WarningMark {
    pub start: usize,
    pub end: usize,
    pub label: Label,
}

/// Labels at the end of a meta line, such as `0.249 KB  ·  PII · financial`.
/// The size and filename stay outside these ranges.
pub fn warning_marks(meta: &str) -> Vec<WarningMark> {
    let Some(suffix_at) = meta.rfind("  ·  ") else {
        return Vec::new();
    };
    let suffix_at = suffix_at + "  ·  ".len();
    let suffix = &meta[suffix_at..];
    if suffix.is_empty() {
        return Vec::new();
    }
    let mut marks = Vec::new();
    let mut offset = suffix_at;
    for part in suffix.split(" · ") {
        let start = offset;
        let end = start + part.len();
        let Some(label) = label_named(part) else {
            return Vec::new();
        };
        marks.push(WarningMark { start, end, label });
        offset = end + " · ".len();
    }
    marks
}

fn label_named(name: &str) -> Option<Label> {
    match name {
        "credential" => Some(Label::Credential),
        "PII" => Some(Label::Pii),
        "financial" => Some(Label::Financial),
        _ => None,
    }
}

/// Kinds present in `text`, one label each, in [`Label`] order.
/// Named columns such as Naam or Salaris join the leakguard hits.
/// Rust and Java are source: a path or a field name is not personal data,
/// so those cards do not pick up a PII label. A real key or account still does.
pub fn labels(text: &str) -> Vec<Label> {
    let code = is_code(crate::format::detect(text));
    let mut found: Vec<Label> = guard()
        .find(text)
        .into_iter()
        .filter(|hit| counts(text, hit))
        .map(|hit| label_for(&hit.kind))
        .filter(|label| !(code && *label == Label::Pii))
        .collect();
    if !code {
        for class in crate::redact::field_classes(text) {
            found.push(match class {
                crate::redact::FieldClass::Pii => Label::Pii,
                crate::redact::FieldClass::Financial => Label::Financial,
            });
        }
        if crate::redact::has_bare_bsn(text) {
            found.push(Label::Pii);
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

fn is_code(kind: FormatKind) -> bool {
    matches!(kind, FormatKind::Rust | FormatKind::Java)
}

/// `std::io` is hex letters around `::`, which is the shape of a compressed
/// IPv6 address. An address has a digit (`fe80::1`). A rust path does not.
fn counts(text: &str, hit: &Match) -> bool {
    hit.kind != Kind::IpV6
        || text[hit.start..hit.end]
            .bytes()
            .any(|byte| byte.is_ascii_digit())
}

/// `Redactor::new` leaves phone numbers and high-entropy tokens off.
fn guard() -> &'static Redactor {
    static GUARD: OnceLock<Redactor> = OnceLock::new();
    GUARD.get_or_init(Redactor::new)
}

fn label_for(kind: &Kind) -> Label {
    match kind {
        Kind::CreditCard | Kind::Iban => Label::Financial,
        Kind::Email
        | Kind::UsSsn
        | Kind::PhoneNumber
        | Kind::IpV4
        | Kind::IpV6
        | Kind::MacAddress => Label::Pii,
        Kind::Jwt
        | Kind::AwsAccessKey
        | Kind::UrlCredentials
        | Kind::GitHubToken
        | Kind::SlackToken
        | Kind::StripeKey
        | Kind::GoogleApiKey
        | Kind::OpenAiKey
        | Kind::PrivateKey
        | Kind::AzureConnectionString
        | Kind::TelegramToken
        | Kind::DiscordToken
        | Kind::GenericSecret
        | Kind::Custom(_) => Label::Credential,
        _ => Label::Credential,
    }
}

#[cfg(test)]
mod tests {
    use super::{Label, label_line, labels, warning_marks};

    #[test]
    fn maps_kinds_onto_card_labels() {
        assert_eq!(
            labels("mail me at jan.devries@email.nl please"),
            vec![Label::Pii]
        );
        assert_eq!(labels("pay to NL91ABNA0417164300"), vec![Label::Financial]);
        assert_eq!(labels("card 4111 1111 1111 1111"), vec![Label::Financial]);
        assert_eq!(
            labels("aws creds AKIAIOSFODNN7EXAMPLE rotated"),
            vec![Label::Credential]
        );
        assert_eq!(
            labels("host 203.0.113.42 device 00:1A:2B:3C:4D:5E"),
            vec![Label::Pii]
        );
        assert_eq!(
            label_line(&labels(
                "user jan@example.com key AKIAIOSFODNN7EXAMPLE iban NL91ABNA0417164300"
            )),
            "credential · PII · financial"
        );
    }

    #[test]
    fn plain_text_and_opt_in_detectors_stay_quiet() {
        assert!(labels("hello world").is_empty());
        assert!(labels("fn main() {}").is_empty());
        assert!(labels("call +1 (415) 555-0132 later").is_empty());
        assert!(labels("Ak9f8s7d6g5h4j3k2l1m0n9b8v7c6x5z").is_empty());
    }

    #[test]
    fn salary_table_labels_pii_and_financial() {
        let src = "\
Naam\tGeboortedatum\tAdres\tTelefoonnummer\tSalaris
Jan de Vries\t1984-05-12\tHoofdstraat 45, Groningen\t06-12345678\t3450
Anja Bakker\t1991-11-23\tKerkplein 2, Utrecht\t06-87654321\t2900
Mohammed El Amin\t1978-02-05\tStationstraat 120, Rotterdam\t06-11223344\t4200";
        assert_eq!(label_line(&labels(src)), "PII · financial");
        assert_eq!(labels("Telefoonnummer: 06-12345678"), vec![Label::Pii]);
    }

    #[test]
    fn warning_marks_pick_out_the_labels() {
        let meta = "4 lines  0.249 KB  ·  PII · financial";
        let marks = warning_marks(meta);
        assert_eq!(marks.len(), 2);
        assert_eq!(&meta[marks[0].start..marks[0].end], "PII");
        assert_eq!(marks[0].label, Label::Pii);
        assert_eq!(&meta[marks[1].start..marks[1].end], "financial");
        assert_eq!(marks[1].label, Label::Financial);

        let named = "people.tsv  ·  4 lines  0.249 KB  ·  credential · PII · financial";
        let marks = warning_marks(named);
        assert_eq!(
            marks
                .iter()
                .map(|mark| &named[mark.start..mark.end])
                .collect::<Vec<_>>(),
            vec!["credential", "PII", "financial"]
        );
        assert!(warning_marks("0.005 KB").is_empty());
        assert!(warning_marks("people.tsv  ·  0.005 KB").is_empty());
        assert!(warning_marks("0.249 KB  ·  PII · notes").is_empty());
    }

    #[test]
    fn rust_source_with_a_key_is_a_credential() {
        let key = "fn main() {\n    let key = \"AKIAIOSFODNN7EXAMPLE\";\n}\n";
        assert_eq!(labels(key), vec![Label::Credential]);
    }

    #[test]
    fn rust_and_java_source_is_not_pii() {
        let rust = "\
use std::io;
use std::collections::HashMap;

fn main() {
    let name: String = String::new();
    let host = \"127.0.0.1\";
    println!(\"{name} {host}\");
}
";
        assert!(labels(rust).is_empty(), "{:?}", labels(rust));
        assert!(labels("use std::io;").is_empty());
        assert_eq!(labels("fe80::1"), vec![Label::Pii]);

        let java = "public class Hi {\n    String host = \"10.0.0.1\";\n}\n";
        assert!(labels(java).is_empty(), "{:?}", labels(java));
    }

    #[test]
    fn bare_bsn_that_passes_the_elfproef_is_pii() {
        assert_eq!(labels("111222333"), vec![Label::Pii]);
        assert_eq!(labels("nummer 12345672 einde"), vec![Label::Pii]);
    }

    #[test]
    fn bare_digits_that_fail_the_elfproef_are_not_pii() {
        assert!(labels("123456789").is_empty());
        assert!(labels("ref 12345678 later").is_empty());
    }

    #[test]
    fn labeled_bsn_stays_pii_when_the_elfproef_fails() {
        assert_eq!(labels("BSN: 123456789"), vec![Label::Pii]);
        assert_eq!(labels("Sofinummer: 12345678"), vec![Label::Pii]);
        let column = "id,bsn\n1,123456789\n";
        assert_eq!(labels(column), vec![Label::Pii]);
    }

    #[test]
    fn iban_stays_financial_and_is_not_a_bsn() {
        assert_eq!(labels("pay to NL91ABNA0417164300"), vec![Label::Financial]);
        assert_eq!(
            label_line(&labels("NL91ABNA0417164300 en 111222333")),
            "PII · financial"
        );
    }
}
