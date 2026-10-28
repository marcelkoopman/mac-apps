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
    /// A standalone card number ([`card_number`]): 13 to 19 digits, plain or in groups,
    /// a known issuer prefix and Luhn.
    Card,
    /// Mod-97 over the compact form, 15 to 34 characters, bounded on both sides.
    Iban,
    /// The local part does not start or end with a dot.
    Email,
    /// A phone number: no digit right before or after, and not the tail of a decimal (`10.06`).
    Phone,
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
        check: Check::Phone,
        // Dutch mobile numbers, written on one line: a space or hyphen between the groups, not a
        // line break or tab (a cell ending in `06` before a row starting with a date is no phone).
        source: r"(?:\+31[\s\-&&[^\r\n\t]]?6|06)[\s\-&&[^\r\n\t]]?(?:[0-9]{8}|[0-9]{2}[\s\-&&[^\r\n\t]]?[0-9]{6}|[0-9]{2}[\s\-&&[^\r\n\t]]?[0-9]{2}[\s\-&&[^\r\n\t]]?[0-9]{2}[\s\-&&[^\r\n\t]]?[0-9]{2})",
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

/// What kind of text is scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Rust, Java or Python source: no personal or financial data (a path or a field name is not
    /// a person, a card number or IBAN is a test fixture). A real key or token still counts.
    Code,
    /// Documentation (Markdown, agent instructions): a support address or two is part of the
    /// text, not a person's data; see [`DOC_EMAILS`].
    Docs,
    /// Anything else: one e-mail address is already personal data.
    Plain,
}

/// In documentation, e-mail addresses count as personal data from this many different ones (a
/// contact list); one or two are a support address or an author line. A phone number, IBAN,
/// card, BSN or labeled field in the same text still counts on its own.
pub const DOC_EMAILS: usize = 3;

/// The classes the first layer finds in `text`, for `scope`.
pub fn classes(text: &str, scope: Scope) -> Classes {
    let code = scope == Scope::Code;
    let mut found = Classes::default();
    let compiled = compiled();
    let present = compiled.set.matches(text);
    for index in present.iter() {
        let pattern = &PATTERNS[index];
        if found.has(pattern.label) || (code && pattern.label != Label::Credential) {
            continue;
        }
        let mut hits = compiled.each[index]
            .find_iter(text)
            .filter(|hit| valid(text.as_bytes(), hit.start(), hit.end(), pattern.check));
        let hit = if scope == Scope::Docs && matches!(pattern.check, Check::Email) {
            distinct_at_least(&mut hits, DOC_EMAILS)
        } else {
            hits.next().is_some()
        };
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

/// Whether `hits` has `wanted` different addresses (case ignored).
fn distinct_at_least<'t>(hits: &mut dyn Iterator<Item = regex::Match<'t>>, wanted: usize) -> bool {
    let mut seen: Vec<&str> = Vec::with_capacity(wanted);
    for hit in hits {
        let address = hit.as_str();
        if !seen.iter().any(|known| known.eq_ignore_ascii_case(address)) {
            seen.push(address);
            if seen.len() >= wanted {
                return true;
            }
        }
    }
    false
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
        Check::Card => card_number(bytes, start, end),
        Check::Iban => iban(bytes, start, end),
        Check::Email => {
            let at = bytes[start..end]
                .iter()
                .position(|byte| *byte == b'@')
                .map_or(start, |offset| start + offset);
            bytes[start] != b'.' && bytes[at - 1] != b'.' && !in_url(bytes, start)
        }
        Check::Phone => {
            let decimal = before == Some(b'.')
                && start
                    .checked_sub(2)
                    .is_some_and(|at| bytes[at].is_ascii_digit());
            // Standalone: not a digit run inside a hex hash or another token
            // (`checksum = "…0629063962b0…"` in a Cargo.lock).
            let free = |byte: Option<u8>| byte.is_none_or(|byte| !is_ident_byte(byte));
            no_digit(before) && no_digit(after) && free(before) && free(after) && !decimal
        }
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

/// Is `bytes[start..end]` a card number on its own? 13 to 19 digits, written in one run or in
/// groups of four (Amex and Diners 4-6-5 / 4-6-4) split by one kind of space or hyphen, with a
/// known issuer prefix ([`known_issuer`]) and a valid Luhn digit. Standalone: not inside a hex
/// value or another token (`0x4111…`, `c982642fa9…`), not in a URL or a path
/// (`facebook.com/reel/1092…`), not a decimal or part of a dotted number. Pure.
pub(crate) fn card_number(bytes: &[u8], start: usize, end: usize) -> bool {
    let Some(span) = bytes.get(start..end) else {
        return false;
    };
    let before = start.checked_sub(1).map(|at| bytes[at]);
    let after = bytes.get(end).copied();
    if !card_edge(before) || !card_edge(after) {
        return false;
    }
    // `1111.` ends a sentence; `1111.5` or `1111.x` is a longer token.
    if after == Some(b'.') && bytes.get(end + 1).is_some_and(|byte| is_token_byte(*byte)) {
        return false;
    }
    if before == Some(b'.') || in_path(bytes, start) {
        return false;
    }
    if !span.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let digits: Vec<u8> = span.iter().copied().filter(u8::is_ascii_digit).collect();
    (13..=19).contains(&digits.len()) && grouped(span) && known_issuer(&digits) && luhn(&digits)
}

/// Before or after a card number: the start or end, white space or punctuation that is not
/// part of a URL, path, number or token.
fn card_edge(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| {
        !is_token_byte(byte)
            && !matches!(
                byte,
                b'/' | b'\\' | b'?' | b'&' | b'=' | b'#' | b'%' | b'+' | b'@' | b'~' | b'$'
            )
    })
}

/// `start` sits in a URL or path: its white-space-delimited token has `/` or `://` before it.
fn in_path(bytes: &[u8], start: usize) -> bool {
    let token = bytes[..start]
        .iter()
        .rposition(u8::is_ascii_whitespace)
        .map_or(0, |space| space + 1);
    bytes[token..start]
        .iter()
        .any(|byte| *byte == b'/' || *byte == b'\\')
}

/// Digits in one run, or groups split by one kind of separator: all fours (the last may be
/// shorter), or 4-6-5 / 4-6-4.
fn grouped(span: &[u8]) -> bool {
    let separator = span.iter().copied().find(|byte| !byte.is_ascii_digit());
    let Some(separator) = separator else {
        return true;
    };
    if span
        .iter()
        .any(|byte| !byte.is_ascii_digit() && *byte != separator)
    {
        return false;
    }
    let groups: Vec<usize> = span
        .split(|byte| *byte == separator)
        .map(<[u8]>::len)
        .collect();
    let fours = groups.len() >= 3
        && groups[..groups.len() - 1].iter().all(|len| *len == 4)
        && (1..=4).contains(&groups[groups.len() - 1]);
    fours || groups == [4, 6, 5] || groups == [4, 6, 4]
}

/// The issuer prefix (IIN) of a card network: Visa, Mastercard, American Express, Discover,
/// Diners Club, JCB, UnionPay, Maestro and Mir.
fn known_issuer(digits: &[u8]) -> bool {
    let prefix = |len: usize| -> u32 {
        digits
            .iter()
            .take(len)
            .fold(0, |acc, digit| acc * 10 + u32::from(digit - b'0'))
    };
    let (one, two, three, four, six) = (prefix(1), prefix(2), prefix(3), prefix(4), prefix(6));
    one == 4
        || (51..=55).contains(&two)
        || (2221..=2720).contains(&four)
        || two == 34
        || two == 37
        || four == 6011
        || (644..=649).contains(&three)
        || two == 65
        || (622126..=622925).contains(&six)
        || two == 62
        || (300..=305).contains(&three)
        || two == 36
        || two == 38
        || two == 39
        || (3528..=3589).contains(&four)
        || matches!(
            four,
            5018 | 5020 | 5038 | 5893 | 6304 | 6759 | 6761 | 6762 | 6763
        )
        || (2200..=2204).contains(&four)
}

/// Is `bytes[start..end]` an IBAN on its own? 15 to 34 characters once spaces are gone, no
/// letter right before it, no token character after it, and a valid ISO 7064 mod-97. Pure.
pub(crate) fn iban(bytes: &[u8], start: usize, end: usize) -> bool {
    let Some(span) = bytes.get(start..end) else {
        return false;
    };
    let before = start.checked_sub(1).map(|at| bytes[at]);
    let after = bytes.get(end).copied();
    let compact: Vec<u8> = span.iter().copied().filter(|byte| *byte != b' ').collect();
    before.is_none_or(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
        && after.is_none_or(|byte| !is_token_byte(byte))
        && (15..=34).contains(&compact.len())
        && compact[..2].iter().all(u8::is_ascii_uppercase)
        && compact[2..4].iter().all(u8::is_ascii_digit)
        && iban_mod97(&compact)
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
/// of its named columns. Lines above a table's header ([`crate::dataframe::table_start`]) are
/// skipped, so the real header names the columns.
fn table_fields(text: &str, found: &mut Classes) {
    let text = match crate::dataframe::table_start(text) {
        Some(start) if start.skipped > 0 => &text[start.offset..],
        _ => text,
    };
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
    use super::{Classes, Scope, classes, has_bare_bsn, iban_mod97, luhn};

    fn found(text: &str) -> Classes {
        classes(text, Scope::Plain)
    }

    fn docs(text: &str) -> Classes {
        classes(text, Scope::Docs)
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
        assert!(found("mobiel\t06 12345678").pii);
        assert!(found("+31\u{a0}6\u{a0}12345678").pii);
        // Not across a line break or tab, nor from a decimal: a value ending in `06`, then a
        // row starting with a date.
        assert!(!found("0.06\n2026-04-16,3.17").pii);
        assert!(!found("10.06\t2026-04-16\t3.17").pii);
        assert!(!found("waarde 10.06 12345678").pii);
        assert!(!found("06\n12345678").pii);
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
        let below_a_title =
            found("Personeel export\nId;Naam;Salaris\n1;Jan de Vries;3450\n2;Anja;2900");
        assert!(below_a_title.pii && below_a_title.financial);
    }

    #[test]
    fn code_has_no_personal_data_but_keeps_keys() {
        let rust = concat!(
            "fn main() {\n    let name = \"jan@example.com\";\n    let key = \"AK",
            "IAIOSFODNN7EXAMPLE\";\n}\n"
        );
        let code = classes(rust, Scope::Code);
        assert!(code.credential);
        assert!(!code.pii);
    }

    #[test]
    fn documentation_with_a_support_address_is_not_personal_data() {
        let readme = "# Tool\n\nVersion 26.0.1, build 1234.\nSupport: mac-apps-support@proton.me\n";
        assert!(!docs(readme).pii);
        // Two addresses (support and author) are still documentation, any case.
        assert!(!docs("a@example.org, A@Example.org and b@example.org").pii);
        // A contact list is not.
        assert!(docs("a@example.org\nb@example.org\nc@example.org").pii);
        // Anything else that is personal still counts in documentation.
        assert!(docs("mail x@example.org, bel 06-12345678").pii);
        assert!(docs("BSN: 123456789\nx@example.org").pii);
        // Outside documentation one address is personal data, as before.
        assert!(found(readme).pii);
    }
}
