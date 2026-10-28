//! Snippets: a pinned text with a name (⋯ › Save as Snippet…). Its `{{name}}` placeholders ask
//! for a value in a fill-in panel (Tab goes to the next field, Return makes the copy); `{{datum}}`
//! / `{{date}}`, `{{tijd}}` / `{{time}}`, `{{uuid}}` and `{{klembord}}` / `{{clipboard}}` fill in
//! by themselves. The result is a new copy, checked like any copy CopyCraft makes. The snippet
//! lives with its pin, in memory. Its text, its name and what is typed are never logged. Pure.

/// The most fields one panel asks for; a placeholder after them stays as it is.
pub const MAX_FIELDS: usize = 8;
/// The longest placeholder name.
const MAX_NAME: usize = 40;

/// A placeholder that fills in by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auto {
    Date,
    Time,
    Uuid,
    Clipboard,
}

impl Auto {
    fn of(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "datum" | "date" => Some(Self::Date),
            "tijd" | "time" => Some(Self::Time),
            "uuid" => Some(Self::Uuid),
            "klembord" | "clipboard" => Some(Self::Clipboard),
            _ => None,
        }
    }
}

/// One piece of a snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part<'a> {
    Text(&'a str),
    /// `{{name}}`: asked for; the name as written, trimmed.
    Field(&'a str),
    Auto(Auto),
}

fn name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.')
}

/// The placeholder at the start of `rest` (which starts with `{{`): its name and length.
fn placeholder(rest: &str) -> Option<(&str, usize)> {
    let inner = &rest[2..];
    let end = inner.find("}}")?;
    let name = inner[..end].trim();
    let ok =
        !name.is_empty() && name.chars().count() <= MAX_NAME && inner[..end].chars().all(name_char);
    ok.then_some((name, end + 4))
}

fn parts(text: &str) -> Vec<Part<'_>> {
    let mut parts = Vec::new();
    let mut plain = 0;
    let mut at = 0;
    while let Some(found) = text[at..].find("{{") {
        let start = at + found;
        match placeholder(&text[start..]) {
            Some((name, len)) => {
                if plain < start {
                    parts.push(Part::Text(&text[plain..start]));
                }
                parts.push(match Auto::of(name) {
                    Some(auto) => Part::Auto(auto),
                    None => Part::Field(name),
                });
                at = start + len;
                plain = at;
            }
            None => at = start + 1,
        }
    }
    if plain < text.len() {
        parts.push(Part::Text(&text[plain..]));
    }
    parts
}

/// The names the panel asks for, in the order they first come, each once (case ignored), at
/// most [`MAX_FIELDS`].
pub fn fields(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in parts(text) {
        if let Part::Field(name) = part
            && names.len() < MAX_FIELDS
            && !names
                .iter()
                .any(|known| known.to_lowercase() == name.to_lowercase())
        {
            names.push(name.to_string());
        }
    }
    names
}

/// The values of the placeholders that fill in by themselves, worked out once per fill.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Automatic {
    pub date: String,
    pub time: String,
    pub uuid: String,
    pub clipboard: zeroize::Zeroizing<String>,
}

impl Automatic {
    /// For the local time `moment`, a v4 UUID from `random` and the clipboard's text: the date
    /// as `28-10-2026` in Dutch and `2026-10-28` in English, the time as `14:05`.
    pub fn at(
        moment: crate::generate::Moment,
        random: [u8; 16],
        clipboard: &str,
        dutch: bool,
    ) -> Self {
        let local = moment
            .unix_ms
            .div_euclid(1000)
            .saturating_add(moment.offset);
        let (year, month, day) = crate::decode::clock::civil_from_days(local.div_euclid(86_400));
        let rest = local.rem_euclid(86_400);
        Self {
            date: if dutch {
                format!("{day:02}-{month:02}-{year:04}")
            } else {
                format!("{year:04}-{month:02}-{day:02}")
            },
            time: format!("{:02}:{:02}", rest / 3600, rest % 3600 / 60),
            uuid: crate::generate::uuid_v4(random),
            clipboard: zeroize::Zeroizing::new(clipboard.to_string()),
        }
    }
}

/// The snippet with its placeholders filled: `answers` in the order of [`fields`] (a missing
/// answer is empty), the others from `auto`. A field over [`MAX_FIELDS`] stays as written.
pub fn fill(text: &str, answers: &[String], auto: &Automatic) -> String {
    let names = fields(text);
    let mut out = String::with_capacity(text.len());
    for part in parts(text) {
        match part {
            Part::Text(plain) => out.push_str(plain),
            Part::Auto(Auto::Date) => out.push_str(&auto.date),
            Part::Auto(Auto::Time) => out.push_str(&auto.time),
            Part::Auto(Auto::Uuid) => out.push_str(&auto.uuid),
            Part::Auto(Auto::Clipboard) => out.push_str(&auto.clipboard),
            Part::Field(name) => {
                match names
                    .iter()
                    .position(|known| known.to_lowercase() == name.to_lowercase())
                {
                    Some(at) => out.push_str(answers.get(at).map_or("", String::as_str)),
                    None => {
                        out.push_str("{{");
                        out.push_str(name);
                        out.push_str("}}");
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "snippet_tests.rs"]
mod tests;
