//! First layer of the sensitivity check: copycraft's own recognizers. One [`RegexSet`] pass
//! (linear time) says which patterns occur at all; only those are searched again, and each hit
//! is validated (Luhn for cards, mod-97 for an IBAN, the elfproef for a BSN, word boundaries for
//! tokens). Labeled fields (`Naam: …`) and tables with named columns (`Naam;Salaris`) are read
//! line by line. A class stops looking once it has one validated hit.

use std::sync::OnceLock;

use regex::{Regex, RegexSet};

use crate::sensitivity::Label;

/// Which of the three classes are present.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Classes {
    pub credential: bool,
    pub pii: bool,
    pub financial: bool,
}

impl Classes {
    fn has(self, label: Label) -> bool {
        match label {
            Label::Credential => self.credential,
            Label::Pii => self.pii,
            Label::Financial => self.financial,
        }
    }

    fn add(&mut self, label: Label) {
        match label {
            Label::Credential => self.credential = true,
            Label::Pii => self.pii = true,
            Label::Financial => self.financial = true,
        }
    }

    pub fn all(self) -> bool {
        self.credential && self.pii && self.financial
    }

    pub fn labels(self) -> Vec<Label> {
        [Label::Credential, Label::Pii, Label::Financial]
            .into_iter()
            .filter(|label| self.has(*label))
            .collect()
    }
}

/// How a pattern's hit is checked before it counts.
#[derive(Debug, Clone, Copy)]
enum Check {
    /// A token with a distinctive prefix: no letter right before it, no token character after.
    Token,
    /// Luhn over the digits, no digit right before or after.
    Card,
    /// Mod-97 over the compact form, 15 to 34 characters, bounded on both sides.
    Iban,
    /// The local part does not start or end with a dot.
    Email,
    /// No digit right before or after.
    Digits,
    /// Nothing beyond the pattern.
    None,
}

struct Pattern {
    label: Label,
    check: Check,
    source: &'static str,
}

const PATTERNS: &[Pattern] = &[
    // Credentials.
    Pattern {
        label: Label::Credential,
        check: Check::None,
        source: r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY[A-Z ]*-----",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"(?:AKIA|ASIA|AGPA|AIDA|AROA|AIPA|ANPA)[A-Z0-9]{16}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"github_pat_[A-Za-z0-9_-]{20,200}|gh[pousr]_[A-Za-z0-9_-]{30,255}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"xox[bpaorsx]-[A-Za-z0-9_-]{10,255}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9_-]{10,255}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"AIza[A-Za-z0-9_-]{35}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"sk-(?:proj-)?[A-Za-z0-9_-]{20,255}",
    },
    Pattern {
        label: Label::Credential,
        check: Check::Token,
        source: r"glpat-[A-Za-z0-9_-]{20,255}|npm_[A-Za-z0-9]{36}",
    },
    // Financial.
    Pattern {
        label: Label::Financial,
        check: Check::Iban,
        source: r"[A-Z]{2}[0-9]{2}(?:[A-Z0-9]{11,30}|(?: [A-Z0-9]{4}){2,7}(?: [A-Z0-9]{1,3})?)",
    },
    Pattern {
        label: Label::Financial,
        check: Check::Card,
        source: r"[0-9](?:[ -]?[0-9]){12,18}",
    },
    // Personal data.
    Pattern {
        label: Label::Pii,
        check: Check::Email,
        source: r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}",
    },
    Pattern {
        label: Label::Pii,
        check: Check::Digits,
        // Dutch mobile numbers.
        source: r"(?:\+31[\s\-]?6|06)[\s\-]?(?:[0-9]{8}|[0-9]{2}[\s\-]?[0-9]{6}|[0-9]{2}[\s\-]?[0-9]{2}[\s\-]?[0-9]{2}[\s\-]?[0-9]{2})",
    },
];

struct Compiled {
    set: RegexSet,
    each: Vec<Regex>,
}

fn compiled() -> &'static Compiled {
    static COMPILED: OnceLock<Compiled> = OnceLock::new();
    COMPILED.get_or_init(|| Compiled {
        set: RegexSet::new(PATTERNS.iter().map(|pattern| pattern.source))
            .expect("recognizer patterns compile"),
        each: PATTERNS
            .iter()
            .map(|pattern| Regex::new(pattern.source).expect("recognizer pattern compiles"))
            .collect(),
    })
}

/// Compile the patterns now, so the first card does not wait for them.
pub fn warm() {
    compiled();
}

/// The classes the first layer finds in `text`. `code` (Rust, Java, Python source) skips
/// personal data: there a path or a field name is not a person.
pub fn classes(text: &str, code: bool) -> Classes {
    let mut found = Classes::default();
    let compiled = compiled();
    let present = compiled.set.matches(text);
    for index in present.iter() {
        let pattern = &PATTERNS[index];
        if found.has(pattern.label) || (code && pattern.label == Label::Pii) {
            continue;
        }
        let hit = compiled.each[index]
            .find_iter(text)
            .any(|hit| valid(text.as_bytes(), hit.start(), hit.end(), pattern.check));
        if hit {
            found.add(pattern.label);
        }
    }
    if !code {
        if !found.pii && has_bare_bsn(text) {
            found.pii = true;
        }
        if !(found.pii && found.financial) {
            fields(text, &mut found);
        }
    }
    found
}

fn valid(bytes: &[u8], start: usize, end: usize, check: Check) -> bool {
    let before = start.checked_sub(1).map(|at| bytes[at]);
    let after = bytes.get(end).copied();
    match check {
        Check::None => true,
        Check::Token => {
            before.is_none_or(|byte| !byte.is_ascii_alphabetic())
                && after.is_none_or(|byte| !is_token_byte(byte))
        }
        Check::Card => {
            let digits: Vec<u8> = bytes[start..end]
                .iter()
                .copied()
                .filter(u8::is_ascii_digit)
                .collect();
            no_digit(before) && no_digit(after) && luhn(&digits)
        }
        Check::Iban => {
            let compact: Vec<u8> = bytes[start..end]
                .iter()
                .copied()
                .filter(|byte| *byte != b' ')
                .collect();
            before.is_none_or(|byte| !byte.is_ascii_alphabetic())
                && after.is_none_or(|byte| !is_token_byte(byte))
                && (15..=34).contains(&compact.len())
                && iban_mod97(&compact)
        }
        Check::Email => {
            let at = bytes[start..end]
                .iter()
                .position(|byte| *byte == b'@')
                .map_or(start, |offset| start + offset);
            bytes[start] != b'.' && bytes[at - 1] != b'.' && !in_url(bytes, start)
        }
        Check::Digits => no_digit(before) && no_digit(after),
    }
}

/// `start` sits in a `scheme://user:password@host` token: those are URL credentials, not an
/// address.
fn in_url(bytes: &[u8], start: usize) -> bool {
    let token = bytes[..start]
        .iter()
        .rposition(u8::is_ascii_whitespace)
        .map_or(0, |space| space + 1);
    memchr::memmem::find(&bytes[token..start], b"://").is_some()
}

fn no_digit(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| !byte.is_ascii_digit())
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

fn luhn(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (index, digit) in digits.iter().rev().enumerate() {
        let mut value = u32::from(digit - b'0');
        if index % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
    }
    !digits.is_empty() && sum.is_multiple_of(10)
}

/// ISO 7064 mod-97 of an IBAN in compact form: the first four characters move to the end and
/// letters count as 10 to 35.
fn iban_mod97(compact: &[u8]) -> bool {
    let mut remainder = 0u32;
    for &byte in compact[4..].iter().chain(&compact[..4]) {
        if byte.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(byte - b'0')) % 97;
        } else if byte.is_ascii_uppercase() {
            remainder = (remainder * 100 + u32::from(byte - b'A' + 10)) % 97;
        } else {
            return false;
        }
    }
    remainder == 1
}

/// A standalone 8- or 9-digit run that passes the elfproef. A longer run, such as the ten
/// account digits in an IBAN, is not a BSN; digits glued to a letter or `_` stay that token's.
pub fn has_bare_bsn(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let end = index;
        let left_open = start == 0 || !is_ident_byte(bytes[start - 1]);
        let right_open = end == bytes.len() || !is_ident_byte(bytes[end]);
        if left_open && right_open && passes_elfproef(&bytes[start..end]) {
            return true;
        }
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Dutch citizenservicenummer check. Eight digits take a leading zero. Weights are 9, 8, 7, 6,
/// 5, 4, 3, 2, -1; the weighted sum is a multiple of 11.
fn passes_elfproef(digits: &[u8]) -> bool {
    let nine: [u8; 9] = match digits.len() {
        9 => digits.try_into().unwrap_or([0; 9]),
        8 => {
            let mut nine = [b'0'; 9];
            nine[1..].copy_from_slice(digits);
            nine
        }
        _ => return false,
    };
    let mut sum = 0i32;
    for (index, byte) in nine.iter().enumerate() {
        let digit = i32::from(byte - b'0');
        let weight = if index == 8 { -1 } else { 9 - index as i32 };
        sum += digit * weight;
    }
    sum % 11 == 0
}

/// What a field label says about its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Personal,
    Amount,
}

impl Field {
    fn label(self) -> Label {
        match self {
            Self::Personal => Label::Pii,
            Self::Amount => Label::Financial,
        }
    }
}

/// Labeled fields (`Naam: Jan`) and tables whose header names a personal or amount column.
fn fields(text: &str, found: &mut Classes) {
    table_fields(text, found);
    if found.pii && found.financial {
        return;
    }
    for line in text.lines() {
        if let Some(field) = labeled_field(line) {
            found.add(field.label());
            if found.pii && found.financial {
                return;
            }
        }
    }
}

/// `label: value` with a known label and a value.
fn labeled_field(line: &str) -> Option<Field> {
    let colon = line.find(':')?;
    let label = line[..colon].trim();
    if label.is_empty() || label.len() > 160 || label.chars().count() > 40 {
        return None;
    }
    let field = field_for_label(label)?;
    (!line[colon + 1..].trim().is_empty()).then_some(field)
}

/// The first non-empty line as a header (tab, `;` or `,`), and a data row with a value in one
/// of its named columns.
fn table_fields(text: &str, found: &mut Classes) {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = lines.next() else {
        return;
    };
    let header = header.trim();
    let Some(delimiter) = detect_delimiter(header) else {
        return;
    };
    let columns: Vec<(usize, Field)> = delimited(header, delimiter)
        .enumerate()
        .filter_map(|(index, cell)| field_for_label(cell.trim()).map(|field| (index, field)))
        .collect();
    if columns.is_empty() || delimited(header, delimiter).count() < 2 {
        return;
    }
    for line in lines {
        for (index, cell) in delimited(line, delimiter).enumerate() {
            let Some((_, field)) = columns.iter().find(|(column, _)| *column == index) else {
                continue;
            };
            if !cell.trim().is_empty() {
                found.add(field.label());
            }
        }
        if columns.iter().all(|(_, field)| found.has(field.label())) {
            return;
        }
    }
}

fn detect_delimiter(header: &str) -> Option<char> {
    let semis = header.matches(';').count();
    let commas = header.matches(',').count();
    let tabs = header.matches('\t').count();
    if tabs > 0 && tabs >= semis && tabs >= commas {
        Some('\t')
    } else if semis >= 1 && semis >= commas {
        Some(';')
    } else if commas >= 1 {
        Some(',')
    } else {
        None
    }
}

/// The cells of `line`; a delimiter inside double quotes does not split (`""` is a quote).
fn delimited(line: &str, delimiter: char) -> impl Iterator<Item = &str> {
    let mut rest = Some(line);
    std::iter::from_fn(move || {
        let current = rest?;
        let mut in_quotes = false;
        let mut chars = current.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            if ch == '"' {
                if in_quotes && chars.peek().is_some_and(|(_, next)| *next == '"') {
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            } else if ch == delimiter && !in_quotes {
                rest = Some(&current[index + ch.len_utf8()..]);
                return Some(&current[..index]);
            }
        }
        rest = None;
        Some(current)
    })
}

fn field_for_label(label: &str) -> Option<Field> {
    if label.len() > 40 {
        return None;
    }
    let key = label
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| *c != '.')
        .collect::<String>();
    let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(match key.as_str() {
        "naam" | "name" | "voornaam" | "achternaam" | "full name" => Field::Personal,
        "adres" | "address" | "straat" | "woonplaats" | "city" => Field::Personal,
        "geboortedatum" | "geboorte datum" | "date of birth" | "dob" | "geboorte" => {
            Field::Personal
        }
        "salaris" | "salary" | "inkomen" | "bedrag" | "amount" => Field::Amount,
        "e-mailadres" | "emailadres" | "e-mail" | "email" | "mail" => Field::Personal,
        "telefoonnummer" | "telefoon" | "phone" | "phonenumber" | "mobiel" => Field::Personal,
        "bsn" | "sofinummer" => Field::Personal,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{Classes, classes, has_bare_bsn, iban_mod97, luhn};

    fn found(text: &str) -> Classes {
        classes(text, false)
    }

    #[test]
    fn tokens_and_keys_are_credentials() {
        for text in [
            concat!("aws AK", "IAIOSFODNN7EXAMPLE rotated"),
            concat!("token gh", "p_0123456789abcdefghijklmnopqrstuvwxyzAB"),
            concat!("github_", "pat_11ABCDEFG0123456789_abcdefghijklmnop"),
            concat!("slack xo", "xb-123456789012-abcdefghijkl"),
            concat!("stripe sk_", "live_51H8abcdefghijklmnop"),
            concat!("google AI", "zaSyA-1234567890abcdefghijklmnopqrstu"),
            concat!("openai sk-", "proj-abcdefghijklmnopqrstuvwx"),
            concat!(
                "-----BEGIN OPENSSH PRIVATE ",
                "KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE ",
                "KEY-----"
            ),
            concat!(
                "Bearer ey",
                "JhbGciOiJIUzI1NiJ9.ey",
                "JzdWIiOiIxMjM0In0.abcdefghijk_LMNOP"
            ),
            concat!("v2AK", "IAIOSFODNN7EXAMPLE"),
        ] {
            assert!(found(text).credential, "{text}");
        }
        for text in [
            concat!("xAK", "IAIOSFODNN7EXAMPLE"),
            concat!("AK", "IAIOSFODNN7EXAMPLEX"),
            "ghp_short",
            "sk-short",
        ] {
            assert!(!found(text).credential, "{text}");
        }
    }

    #[test]
    fn iban_and_cards_need_their_checksum() {
        assert!(found("pay to NL91ABNA0417164300").financial);
        assert!(found("pay to NL91 ABNA 0417 1643 00").financial);
        assert!(!found("pay to NL92ABNA0417164300").financial);
        assert!(found("card 4111 1111 1111 1111").financial);
        assert!(found("card 4111-1111-1111-1111").financial);
        assert!(!found("card 4111 1111 1111 1112").financial);
        assert!(!found("id 14111111111111111").financial);
        assert!(iban_mod97(b"GB82WEST12345698765432"));
        assert!(luhn(b"79927398713"));
        assert!(!luhn(b"79927398710"));
    }

    #[test]
    fn personal_data() {
        assert!(found("mail me at jan.devries@email.nl please").pii);
        assert!(!found("not an address: jan@localhost").pii);
        assert!(!found(concat!("postgres://admin:", "hunter2@db.internal/app")).pii);
        assert!(found("mailto:jan@example.nl").pii);
        assert!(found("bel 06-12345678 of +31 6 12345678").pii);
        assert!(found("nummer 06 12 34 56 78").pii);
        assert!(!found("order 0612345678901").pii);
        assert!(found("klant 111222333 bevestigd").pii);
        assert!(!found("ref 123456789").pii);
        assert!(has_bare_bsn("oud nummer 12345672"));
        assert!(!has_bare_bsn("id x111222333 en 1112223330"));
        assert!(!has_bare_bsn("pay to NL91ABNA0417164300"));
    }

    #[test]
    fn labeled_and_tabular_fields() {
        let record = "Naam: Jan de Vries\nTelefoonnummer: 06-12345678\nSalaris: € 3.450";
        let record = found(record);
        assert!(record.pii && record.financial);
        let table = "Id;Naam;Salaris\n1;Jan de Vries;3450";
        let table = found(table);
        assert!(table.pii && table.financial);
        assert!(found("BSN: 123456789").pii);
        assert!(found("Sofinummer: 12345678").pii);
        assert!(found("id,E-mail\n1,\"a, b\"").pii);
        assert!(!found("Naam:").pii);
        assert!(!found("Id;Naam\n").pii);
        assert!(!found("Id;Product\n1;Fiets").pii);
    }

    #[test]
    fn code_has_no_personal_data_but_keeps_keys() {
        let rust = concat!(
            "fn main() {\n    let name = \"jan@example.com\";\n    let key = \"AK",
            "IAIOSFODNN7EXAMPLE\";\n}\n"
        );
        let code = classes(rust, true);
        assert!(code.credential);
        assert!(!code.pii);
    }
}
