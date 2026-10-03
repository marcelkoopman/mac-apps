//! A synthetic corpus (no real secrets: documented example keys, test cards, the example IBAN
//! and BSN test numbers) with the labels each sample must get.

use super::{Label, found};

const C: &[Label] = &[Label::Credential];
const P: &[Label] = &[Label::Pii];
const F: &[Label] = &[Label::Financial];
const PF: &[Label] = &[Label::Pii, Label::Financial];
const CP: &[Label] = &[Label::Credential, Label::Pii];
const NONE: &[Label] = &[];

pub const SAMPLES: &[(&str, &[Label])] = &[
    // Credentials.
    (
        concat!(
            "AWS_ACCESS_KEY_ID=AK",
            "IAIOSFODNN7EXAMPLE\nAWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/",
            "K7MDENG/bPxRfiCYEXAMPLEKEY"
        ),
        C,
    ),
    (
        concat!(
            "export GITHUB_TOKEN=gh",
            "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"
        ),
        C,
    ),
    (
        concat!(
            "token: github_",
            "pat_11AAAAAAA0aaaaaaaaaaaa_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ),
        C,
    ),
    (
        concat!(
            "SLACK_BOT_TOKEN=xo",
            "xb-000000000000-000000000000-abcdefghijklmnopqrstuvwx"
        ),
        C,
    ),
    (
        concat!("stripe.api_key = \"sk_", "test_4eC39HqLyjWDarjtT1zdp7dc\""),
        C,
    ),
    (
        concat!(
            "GOOGLE_MAPS_KEY=AI",
            "zaSyD-EXAMPLE-0123456789abcdefghijklm"
        ),
        C,
    ),
    (
        concat!(
            "OPENAI_API_KEY=sk-",
            "proj-EXAMPLEexampleEXAMPLEexample0123"
        ),
        C,
    ),
    (
        concat!(
            "-----BEGIN RSA PRIVATE ",
            "KEY-----\nMIIEowIBAAKCAQEAexample\n-----END RSA PRIVATE ",
            "KEY-----"
        ),
        C,
    ),
    (
        concat!(
            "-----BEGIN EC PRIVATE ",
            "KEY-----\nMHcCAQEEIexample\n-----END EC PRIVATE ",
            "KEY-----"
        ),
        C,
    ),
    (
        concat!(
            "Authorization: Bearer ey",
            "JhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.ey",
            "JzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
        ),
        C,
    ),
    (
        concat!(
            "DATABASE_URL=postgres://admin:",
            "hunter2@db.internal:5432/app"
        ),
        C,
    ),
    (
        concat!(
            "DefaultEndpointsProtocol=https;AccountName=example;Account",
            "Key=ZXhhbXBsZWtleWV4YW1wbGVrZXlleGFtcGxla2V5ZXhhbXBsZWtleQ==;EndpointSuffix=core.windows.net"
        ),
        C,
    ),
    (concat!("GITLAB_TOKEN=gl", "pat-EXAMPLEexample012345"), C),
    (
        concat!(
            "//registry.npmjs.org/:_authToken=np",
            "m_EXAMPLEexampleEXAMPLEexample012345ab"
        ),
        C,
    ),
    // Personal data.
    ("Neem contact op met jan.devries@example.nl voor vragen.", P),
    ("Bel me op 06-12345678 of +31 6 1234 5678.", P),
    (
        "Naam: Jan de Vries\nAdres: Dorpsstraat 1\nWoonplaats: Utrecht",
        P,
    ),
    ("BSN: 111222333", P),
    ("klantnummer 123456782 bevestigd", P),
    (
        "id;naam;email\n1;Jan de Vries;jan@example.nl\n2;Piet Jansen;piet@example.nl",
        P,
    ),
    ("server 192.168.10.24 antwoordt niet", P),
    ("SSN 078-05-1120 on file", P),
    ("MAC 00:1A:2B:3C:4D:5E", P),
    // Financial.
    (
        "Graag overmaken naar NL91ABNA0417164300 t.n.v. Stichting",
        F,
    ),
    ("IBAN: NL91 ABNA 0417 1643 00", F),
    ("Visa 4111 1111 1111 1111 exp 12/29", F),
    ("Mastercard 5555-5555-5555-4444", F),
    ("Bedrag: € 1.250,00", F),
    ("Salaris: 3450", F),
    // Combinations.
    (
        "Naam;Salaris;IBAN\nJan de Vries;3450;NL91ABNA0417164300",
        PF,
    ),
    (
        concat!(
            "{\"name\": \"Jan\", \"email\": \"jan@example.nl\", \"api_key\": \"sk_",
            "test_4eC39HqLyjWDarjtT1zdp7dc\"}"
        ),
        CP,
    ),
    // Not sensitive.
    ("De vergadering is verplaatst naar donderdag 14:00.", NONE),
    (
        "fn main() {\n    let email = \"jan@example.nl\";\n    println!(\"{email}\");\n}\n",
        NONE,
    ),
    ("order 1234567890123 shipped", NONE),
    ("version 1.2.3, build 20260101", NONE),
    ("https://example.com/docs?page=2", NONE),
    ("id;product;prijs\n1;fiets;299\n2;slot;25", NONE),
    ("ghp_short and sk-short are not tokens", NONE),
    ("NL92ABNA0417164300 fails mod-97", NONE),
    ("4111 1111 1111 1112 fails Luhn", NONE),
    ("ref 123456789 fails the elfproef", NONE),
];

#[test]
fn corpus_labels() {
    let mut wrong = Vec::new();
    for (text, expected) in SAMPLES {
        let labels = found(text).labels;
        if labels != *expected {
            wrong.push(format!("{text:?}: {labels:?}, expected {expected:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The version of `src` after `op`, as CSV: what the card checks for a table version.
fn version_csv(src: &str, op: crate::table::TableOp) -> String {
    let mut versions = crate::table::TableVersions::default();
    let job = versions.push(op, src).expect("push");
    let done = job
        .run(&std::sync::atomic::AtomicBool::new(false))
        .expect("job");
    assert!(versions.finish(done));
    crate::dataframe::frame_csv(versions.frame().expect("frame")).expect("csv")
}

#[test]
fn table_versions_get_no_labels_their_source_has_not() {
    use crate::table::TableOp;
    let src = crate::dataframe::tests::ENERGY_FIXTURE;
    assert_eq!(found(src).labels, NONE);
    // Their CSV has ISO dates at the start of a row, after a value ending in `06`.
    for op in [
        TableOp::Dedupe,
        TableOp::DropConstant,
        TableOp::Sort {
            column: "Date".into(),
            descending: true,
        },
        TableOp::Transpose,
        TableOp::SelectColumns {
            columns: vec!["Date".into(), "Home Usage (kWh)".into()],
            kept_of: Some(20),
        },
    ] {
        let csv = version_csv(src, op.clone());
        assert_eq!(found(&csv).labels, NONE, "{op:?}");
    }
    // A phone number in a version still counts.
    let phones = "id,ref\n1,06-12345678\n1,06-12345678\n2,x";
    let csv = version_csv(phones, TableOp::Dedupe);
    assert_eq!(found(&csv).labels, P);
}
