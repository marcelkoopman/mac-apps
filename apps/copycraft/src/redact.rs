use std::sync::{Arc, OnceLock};

use redact_core::recognizers::Recognizer;
use redact_core::recognizers::pattern::PatternRecognizer;
use redact_core::types::RecognizerResult;
use redact_core::{AnalyzerEngine, EntityType};

/// Card category for a labeled or tabular field. Salary columns are financial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldClass {
    Pii,
    Financial,
}

/// Labels from the Dutch field recognizers. Leakguard does not see names or salaries.
pub fn field_classes(text: &str) -> Vec<FieldClass> {
    let scanned = Scan::new(text);
    let Ok(found) = engine().analyze(scanned.as_ref().map(Scan::text).unwrap_or(text), None) else {
        return Vec::new();
    };
    let mut classes = Vec::new();
    for hit in found.detected_entities {
        if !is_field_recognizer(&hit.recognizer_name) {
            continue;
        }
        let class = match &hit.entity_type {
            EntityType::Custom(name) if name == "AMOUNT" => FieldClass::Financial,
            EntityType::CreditCard | EntityType::Iban | EntityType::IbanCode => {
                FieldClass::Financial
            }
            _ => FieldClass::Pii,
        };
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    classes
}

fn is_field_recognizer(name: &str) -> bool {
    matches!(name, "labeled-field" | "tabular-field" | "nl-phone")
}

/// ASCII stand-in for a string that contains a multibyte character.
///
/// `PatternRecognizer::check_context` takes a match end and adds 50 bytes,
/// then slices. A dataframe grid puts `┆` (three bytes) next to a phone or
/// email, so that index lands inside the character and the process panics.
/// Spaces keep the byte length, and therefore the match indexes, of the
/// original text.
struct Scan {
    text: String,
}

impl Scan {
    fn new(text: &str) -> Option<Self> {
        if text.is_ascii() {
            return None;
        }
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch.is_ascii() {
                out.push(ch);
            } else {
                for _ in 0..ch.len_utf8() {
                    out.push(' ');
                }
            }
        }
        Some(Self { text: out })
    }

    fn text(&self) -> &str {
        &self.text
    }
}

fn engine() -> &'static AnalyzerEngine {
    static ENGINE: OnceLock<AnalyzerEngine> = OnceLock::new();
    ENGINE.get_or_init(build_engine)
}

fn build_engine() -> AnalyzerEngine {
    let mut engine = AnalyzerEngine::new();
    engine
        .recognizer_registry_mut()
        .add_recognizer(Arc::new(LabeledFieldRecognizer));
    engine
        .recognizer_registry_mut()
        .add_recognizer(Arc::new(TabularFieldRecognizer));
    engine
        .recognizer_registry_mut()
        .add_recognizer(Arc::new(dutch_phone_recognizer()));
    engine
        .recognizer_registry_mut()
        .add_recognizer(Arc::new(BsnRecognizer));
    engine
}

fn bare_bsn_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
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
        if left_open
            && right_open
            && matches!(end - start, 8 | 9)
            && passes_elfproef(&text[start..end])
        {
            spans.push((start, end));
        }
    }
    spans
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Dutch citizenservicenummer check. Eight digits take a leading zero.
/// Weights are 9, 8, 7, 6, 5, 4, 3, 2, -1. The weighted sum is a multiple of 11.
fn passes_elfproef(digits: &str) -> bool {
    let bytes = digits.as_bytes();
    let nine = if bytes.len() == 9 {
        let mut nine = [0; 9];
        nine.copy_from_slice(bytes);
        nine
    } else if bytes.len() == 8 {
        let mut nine = [b'0'; 9];
        nine[1..].copy_from_slice(bytes);
        nine
    } else {
        return false;
    };
    if nine.iter().any(|byte| !byte.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0i32;
    for (index, byte) in nine.iter().enumerate() {
        let digit = i32::from(byte - b'0');
        let weight = if index == 8 { -1 } else { 9 - index as i32 };
        sum += digit * weight;
    }
    sum % 11 == 0
}

#[derive(Debug)]
struct BsnRecognizer;

impl Recognizer for BsnRecognizer {
    fn name(&self) -> &str {
        "nl-bsn"
    }

    fn supported_entities(&self) -> &[EntityType] {
        static ENTITIES: OnceLock<Vec<EntityType>> = OnceLock::new();
        ENTITIES
            .get_or_init(|| vec![EntityType::Custom("NL_BSN".into())])
            .as_slice()
    }

    fn supports_language(&self, _language: &str) -> bool {
        true
    }

    fn analyze(&self, text: &str, _language: &str) -> anyhow::Result<Vec<RecognizerResult>> {
        Ok(bare_bsn_spans(text)
            .into_iter()
            .map(|(start, end)| {
                RecognizerResult::new(
                    EntityType::Custom("NL_BSN".into()),
                    start,
                    end,
                    0.95,
                    self.name(),
                )
                .with_text(text)
            })
            .collect())
    }
}

fn dutch_phone_recognizer() -> PatternRecognizer {
    let mut recognizer = PatternRecognizer::with_name("nl-phone");
    let _ = recognizer.add_pattern_with_context(
        EntityType::PhoneNumber,
        r"(?i)(?:\+31[\s\-]?6|06)[\s\-]?\d{8}\b|(?:\+31[\s\-]?6|06)[\s\-]?\d{2}[\s\-]?\d{6}\b|(?:\+31[\s\-]?6|06)[\s\-]?\d{2}[\s\-]?\d{2}[\s\-]?\d{2}[\s\-]?\d{2}\b",
        0.85,
        vec![
            "telefoon".into(),
            "telefoonnummer".into(),
            "mobiel".into(),
            "phone".into(),
        ],
    );
    recognizer
}

#[derive(Debug)]
struct LabeledFieldRecognizer;

impl Recognizer for LabeledFieldRecognizer {
    fn name(&self) -> &str {
        "labeled-field"
    }

    fn supported_entities(&self) -> &[EntityType] {
        &[
            EntityType::Person,
            EntityType::Location,
            EntityType::DateTime,
            EntityType::EmailAddress,
            EntityType::PhoneNumber,
        ]
    }

    fn supports_language(&self, _language: &str) -> bool {
        true
    }

    fn analyze(&self, text: &str, _language: &str) -> anyhow::Result<Vec<RecognizerResult>> {
        let mut results = Vec::new();
        let mut offset = 0usize;
        for line in text.split_inclusive('\n') {
            let content = line.trim_end_matches(['\n', '\r']);
            if let Some((label, value_start_in_line)) = labeled_value_start(content) {
                let Some(entity) = entity_for_label(label) else {
                    offset += line.len();
                    continue;
                };
                let value_start = offset + value_start_in_line;
                let value_end = offset + content.len();
                if value_end > value_start {
                    results.push(
                        RecognizerResult::new(entity, value_start, value_end, 0.95, self.name())
                            .with_text(text),
                    );
                }
            }
            offset += line.len();
        }
        Ok(results)
    }
}

#[derive(Debug)]
struct TabularFieldRecognizer;

impl Recognizer for TabularFieldRecognizer {
    fn name(&self) -> &str {
        "tabular-field"
    }

    fn supported_entities(&self) -> &[EntityType] {
        &[
            EntityType::Person,
            EntityType::Location,
            EntityType::DateTime,
            EntityType::EmailAddress,
            EntityType::PhoneNumber,
        ]
    }

    fn supports_language(&self, _language: &str) -> bool {
        true
    }

    fn analyze(&self, text: &str, _language: &str) -> anyhow::Result<Vec<RecognizerResult>> {
        let Some(table) = parse_pii_table(text) else {
            return Ok(Vec::new());
        };
        let mut results = Vec::new();
        let mut offset = 0usize;
        let mut row_index = 0usize;
        for line in text.split_inclusive('\n') {
            let content = line.trim_end_matches(['\n', '\r']);
            if content.trim().is_empty() {
                offset += line.len();
                continue;
            }
            if row_index > 0 {
                for (col, entity) in &table.pii_columns {
                    if let Some((start_in_line, end_in_line)) =
                        cell_span(content, table.delimiter, *col)
                    {
                        let value_start = offset + start_in_line;
                        let value_end = offset + end_in_line;
                        if value_end > value_start {
                            results.push(
                                RecognizerResult::new(
                                    entity.clone(),
                                    value_start,
                                    value_end,
                                    0.95,
                                    self.name(),
                                )
                                .with_text(text),
                            );
                        }
                    }
                }
            }
            row_index += 1;
            offset += line.len();
        }
        Ok(results)
    }
}

struct PiiTable {
    delimiter: char,
    pii_columns: Vec<(usize, EntityType)>,
}

fn parse_pii_table(text: &str) -> Option<PiiTable> {
    let header = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let delimiter = detect_delimiter(header)?;
    let headers = split_delimited(header, delimiter);
    if headers.len() < 2 {
        return None;
    }
    let pii_columns: Vec<(usize, EntityType)> = headers
        .iter()
        .enumerate()
        .filter_map(|(index, label)| entity_for_label(label).map(|entity| (index, entity)))
        .collect();
    if pii_columns.is_empty() {
        return None;
    }
    Some(PiiTable {
        delimiter,
        pii_columns,
    })
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

fn split_delimited(line: &str, delimiter: char) -> Vec<&str> {
    delimited_spans(line, delimiter)
        .into_iter()
        .map(|(start, end)| line[start..end].trim())
        .collect()
}

fn delimited_spans(line: &str, delimiter: char) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek().is_some_and(|(_, next)| *next == '"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if ch == delimiter && !in_quotes {
            spans.push((start, idx));
            start = idx + ch.len_utf8();
        }
    }
    spans.push((start, line.len()));
    spans
}

fn cell_span(line: &str, delimiter: char, column: usize) -> Option<(usize, usize)> {
    let (start, end) = delimited_spans(line, delimiter).into_iter().nth(column)?;
    trim_cell_span(line, start, end)
}

fn trim_cell_span(line: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let cell = &line[start..end];
    let trimmed = cell.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lead = cell.len() - cell.trim_start().len();
    let trail = cell.len() - cell.trim_end().len();
    Some((start + lead, end - trail))
}

fn labeled_value_start(line: &str) -> Option<(&str, usize)> {
    let colon = line.find(':')?;
    let label = line[..colon].trim();
    if label.is_empty() || label.chars().count() > 40 {
        return None;
    }
    entity_for_label(label)?;
    let after = &line[colon + 1..];
    let value = after.trim_start();
    if value.is_empty() {
        return None;
    }
    let value_start = colon + 1 + (after.len() - value.len());
    Some((label, value_start))
}

fn entity_for_label(label: &str) -> Option<EntityType> {
    let key = label
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| *c != '.')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(match key.as_str() {
        "naam" | "name" | "voornaam" | "achternaam" | "full name" => EntityType::Person,
        "adres" | "address" | "straat" | "woonplaats" | "city" => EntityType::Location,
        "geboortedatum" | "geboorte datum" | "date of birth" | "dob" | "geboorte" => {
            EntityType::DateTime
        }
        "salaris" | "salary" | "inkomen" | "bedrag" | "amount" => {
            EntityType::Custom("AMOUNT".into())
        }
        "e-mailadres" | "emailadres" | "e-mail" | "email" | "mail" => EntityType::EmailAddress,
        "telefoonnummer" | "telefoon" | "phone" | "phonenumber" | "mobiel" => {
            EntityType::PhoneNumber
        }
        "bsn" | "sofinummer" => EntityType::Custom("NL_BSN".into()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::FieldClass;

    #[test]
    fn dutch_record_and_tables_are_pii_and_financial() {
        let record = "\
Naam: Jan de Vries
Telefoonnummer: 06-12345678
Salaris: € 3.450";
        let classes = super::field_classes(record);
        assert!(classes.contains(&FieldClass::Pii));
        assert!(classes.contains(&FieldClass::Financial));

        let table = "\
Id;Naam;Salaris
1;Jan de Vries;3450";
        let classes = super::field_classes(table);
        assert!(classes.contains(&FieldClass::Pii));
        assert!(classes.contains(&FieldClass::Financial));
        assert!(super::field_classes("BSN: 123456789").contains(&FieldClass::Pii));
        assert!(super::field_classes("Sofinummer: 12345678").contains(&FieldClass::Pii));
    }

    #[test]
    fn box_drawing_next_to_a_phone_does_not_panic() {
        let src = "call 06-12345678┆now";
        let _ = super::field_classes(src);
    }

    #[test]
    fn dataframe_grid_of_a_phone_csv_can_be_scanned() {
        let src = "\
Id,Naam,Telefoonnummer,Salaris
1,Jan de Vries,06-12345678,3450
2,Anja Bakker,06-87654321,2900";
        let grid = crate::dataframe::try_format(src).expect("dataframe");
        assert!(grid.contains('┆'), "{grid}");
        let _ = super::field_classes(&grid);
    }
}
