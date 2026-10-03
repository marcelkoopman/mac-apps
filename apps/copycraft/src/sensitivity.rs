use std::sync::OnceLock;

use leakguard::{Kind, Match, Redactor};
use zeroize::Zeroizing;

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

/// Copies shorter than this are checked while the card is built. Longer ones go to the
/// background checker, and the card says [`CHECKING`] until their labels arrive.
pub const SYNC_LIMIT: usize = 32 * 1024;
/// Copies up to this long get every recognizer. Longer ones only the fast linear ones, and the
/// card says [`PARTIAL`].
pub const FULL_LIMIT: usize = 256 * 1024;
/// Meta-line status while the labels are not known yet. The card cannot be revealed meanwhile.
pub const CHECKING: &str = "Checking…";
/// Meta-line status of a copy only the fast recognizers looked at.
pub const PARTIAL: &str = "partially checked";

/// The labels found in a copy, and whether only part of the recognizers ran ([`FULL_LIMIT`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub labels: Vec<Label>,
    pub partial: bool,
}

/// What the card knows about a copy's labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Labeling {
    Known(Found),
    /// The background checker is on it; [`on_checked`]'s hook runs when it is done.
    Checking,
}

/// The labels for the card: a short copy is checked now, a longer one is remembered or handed
/// to the background checker ([`Labeling::Checking`]).
pub fn labeling(text: &str) -> Labeling {
    if text.len() < SYNC_LIMIT {
        return Labeling::Known(found(text));
    }
    if let Some(known) = LABELS.get(text) {
        return Labeling::Known(known);
    }
    checker::request(text);
    Labeling::Checking
}

/// Run `hook` (from the checker thread) whenever the checker has labels for a copy.
pub fn on_checked(hook: fn()) {
    checker::on_ready(hook);
}

/// The meta-line status at the end of `meta`, if any: [`CHECKING`] or [`PARTIAL`].
pub fn meta_status(meta: &str) -> Option<&'static str> {
    [CHECKING, PARTIAL]
        .into_iter()
        .find(|status| meta.ends_with(&format!("  ·  {status}")))
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
    let meta = match meta_status(meta) {
        Some(status) => &meta[..meta.len() - status.len() - "  ·  ".len()],
        None => meta,
    };
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
/// Remembered for large texts (see [`crate::memo`]): scanning a large copy takes hundreds of
/// milliseconds and the card asks on every redraw.
#[cfg(test)]
pub fn labels(text: &str) -> Vec<Label> {
    found(text).labels
}

/// [`labels`] with the partial flag, computed here and now.
pub fn found(text: &str) -> Found {
    LABELS.get_or_compute(text, |text| scan(text, &|| false).unwrap_or_default())
}

static LABELS: crate::memo::Memo<Found> = crate::memo::Memo::new(8);

/// Drop the remembered labels and any check in progress (Wipe).
pub fn forget_labels() {
    checker::forget();
    LABELS.clear();
}

/// Scan `text`. `None` when `cancelled` said so between two stages (a newer copy came in).
///
/// First copycraft's own recognizers ([`crate::recognize`]); then leakguard, only for the
/// classes they did not find; then redact-core's analyzer for personal and financial fields,
/// only when those are still missing and the copy is at most [`FULL_LIMIT`] long.
fn scan(text: &str, cancelled: &dyn Fn() -> bool) -> Option<Found> {
    let partial = text.len() > FULL_LIMIT;
    let code = is_code(crate::format::detect(text));
    if cancelled() {
        return None;
    }
    let mut classes = crate::recognize::classes(text, code);
    if !classes.all() {
        if cancelled() {
            return None;
        }
        for hit in guard_for(classes).find(text) {
            let label = label_for(&hit.kind);
            if counts(text, &hit) && !(code && label == Label::Pii) {
                classes = with(classes, label);
            }
        }
    }
    let fields_missing = !(classes.pii && classes.financial);
    if !code && fields_missing && !partial {
        if cancelled() {
            return None;
        }
        // The analyzer is far from linear on large tables (seconds per megabyte).
        for class in crate::redact::field_classes(text) {
            classes = with(
                classes,
                match class {
                    crate::redact::FieldClass::Pii => Label::Pii,
                    crate::redact::FieldClass::Financial => Label::Financial,
                },
            );
        }
    }
    Some(Found {
        labels: classes.labels(),
        partial: partial && fields_missing && !code,
    })
}

fn with(mut classes: crate::recognize::Classes, label: Label) -> crate::recognize::Classes {
    match label {
        Label::Credential => classes.credential = true,
        Label::Pii => classes.pii = true,
        Label::Financial => classes.financial = true,
    }
    classes
}

/// Leakguard with only the detectors of the classes not found yet, built once per combination.
fn guard_for(found: crate::recognize::Classes) -> &'static Redactor {
    static GUARDS: [OnceLock<Redactor>; 8] = [const { OnceLock::new() }; 8];
    let missing = usize::from(!found.credential)
        | usize::from(!found.pii) << 1
        | usize::from(!found.financial) << 2;
    GUARDS[missing].get_or_init(|| {
        let mut kinds = Vec::new();
        if !found.credential {
            kinds.extend([
                Kind::PrivateKey,
                Kind::AzureConnectionString,
                Kind::TelegramToken,
                Kind::DiscordToken,
                Kind::Jwt,
                Kind::GitHubToken,
                Kind::SlackToken,
                Kind::StripeKey,
                Kind::OpenAiKey,
                Kind::GoogleApiKey,
                Kind::AwsAccessKey,
                Kind::UrlCredentials,
            ]);
        }
        if !found.pii {
            kinds.extend([
                Kind::Email,
                Kind::IpV6,
                Kind::IpV4,
                Kind::MacAddress,
                Kind::UsSsn,
            ]);
        }
        if !found.financial {
            kinds.extend([Kind::Iban, Kind::CreditCard]);
        }
        Redactor::only(&kinds)
    })
}

/// The background checker: one thread, one job at a time. A new request supersedes the waiting
/// job and cancels the running one at its next stage (generation counter); a result whose
/// generation is no longer current is dropped. Its copy of the text is zeroized with the job.
mod checker {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

    use super::Zeroizing;

    struct Job {
        generation: u64,
        text: Zeroizing<String>,
    }

    struct State {
        job: Option<Job>,
        /// Key of the text waiting or being checked, so the card asking again adds nothing.
        asked: Option<(usize, u64)>,
    }

    static STATE: Mutex<State> = Mutex::new(State {
        job: None,
        asked: None,
    });
    static WAKE: Condvar = Condvar::new();
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    static READY: OnceLock<fn()> = OnceLock::new();
    static STARTED: OnceLock<()> = OnceLock::new();

    fn state() -> MutexGuard<'static, State> {
        STATE.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn on_ready(hook: fn()) {
        let _ = READY.set(hook);
    }

    pub fn request(text: &str) {
        let key = crate::memo::text_key(text);
        let mut state = state();
        if state.asked == Some(key) {
            return;
        }
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        state.job = Some(Job {
            generation,
            text: Zeroizing::new(text.to_string()),
        });
        state.asked = Some(key);
        drop(state);
        start();
        WAKE.notify_one();
    }

    pub fn forget() {
        GENERATION.fetch_add(1, Ordering::SeqCst);
        let mut state = state();
        state.job = None;
        state.asked = None;
    }

    fn start() {
        STARTED.get_or_init(|| {
            if let Err(e) = std::thread::Builder::new()
                .name("copycraft-checker".into())
                .spawn(run)
            {
                eprintln!("sensitivity checker not started: {e}");
            }
        });
    }

    fn run() {
        loop {
            let job = {
                let mut state = state();
                loop {
                    if let Some(job) = state.job.take() {
                        break job;
                    }
                    state = WAKE.wait(state).unwrap_or_else(|err| err.into_inner());
                }
            };
            let current = || GENERATION.load(Ordering::SeqCst) == job.generation;
            let started = std::time::Instant::now();
            let found = super::scan(&job.text, &|| !current());
            if !current() {
                continue;
            }
            state().asked = None;
            let Some(found) = found else {
                continue;
            };
            #[cfg(debug_assertions)]
            eprintln!(
                "copycraft: checked {} KB in {} ms{}",
                job.text.len() / 1024,
                started.elapsed().as_millis(),
                if found.partial { " (partially)" } else { "" }
            );
            #[cfg(not(debug_assertions))]
            let _ = started;
            super::LABELS.put(&job.text, found);
            if let Some(hook) = READY.get() {
                hook();
            }
        }
    }
}

fn is_code(kind: FormatKind) -> bool {
    matches!(
        kind,
        FormatKind::Rust | FormatKind::Java | FormatKind::Python
    )
}

/// `std::io` is hex letters around `::`, which is the shape of a compressed
/// IPv6 address. An address has a digit (`fe80::1`). A rust path does not.
fn counts(text: &str, hit: &Match) -> bool {
    hit.kind != Kind::IpV6
        || text[hit.start..hit.end]
            .bytes()
            .any(|byte| byte.is_ascii_digit())
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
    fn long_copies_are_checked_in_the_background() {
        use super::{FULL_LIMIT, Found, Labeling, SYNC_LIMIT, labeling, meta_status};
        let short = "mail jan@example.com";
        assert_eq!(
            labeling(short),
            Labeling::Known(Found {
                labels: vec![Label::Pii],
                partial: false
            })
        );
        let long = format!(
            "{}\nmail jan@example.com\n",
            "lorem ipsum ".repeat(SYNC_LIMIT / 10)
        );
        let mut answer = labeling(&long);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while answer == Labeling::Checking && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            answer = labeling(&long);
        }
        assert_eq!(
            answer,
            Labeling::Known(Found {
                labels: vec![Label::Pii],
                partial: false
            })
        );
        let huge = format!(
            "{}\nkey AKIAIOSFODNN7EXAMPLE\n",
            "lorem ipsum ".repeat(FULL_LIMIT / 10)
        );
        assert_eq!(
            super::found(&huge),
            Found {
                labels: vec![Label::Credential],
                partial: true
            }
        );
        assert_eq!(
            meta_status("1.2 MB  ·  credential  ·  partially checked"),
            Some("partially checked")
        );
        assert_eq!(meta_status("1.2 MB  ·  Checking…"), Some("Checking…"));
        assert_eq!(meta_status("1.2 MB  ·  PII"), None);
        let marks = warning_marks("1.2 MB  ·  PII · financial  ·  partially checked");
        assert_eq!(marks.len(), 2);
        assert!(warning_marks("1.2 MB  ·  Checking…").is_empty());
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
