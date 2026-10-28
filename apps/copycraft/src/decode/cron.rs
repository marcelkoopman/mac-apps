//! Cron: a schedule of five fields (`minute hour day month weekday`), six with seconds first
//! (Spring, Quartz: `?` for "any"), or `@daily`, `@hourly`, `@weekly`, `@monthly`, `@yearly`,
//! `@reboot`. Explained in words ("Every Monday at 08:00") with the next five times it fires in
//! local time. Vixie cron's rule: with both a day of the month and a weekday, either one fires.
//! String work and the decoders' civil-date arithmetic only; a schedule is never logged.

use super::clock::{self, Env};
use crate::locale::Lang;

/// Five times, or none at `@reboot`; no search past eight years (`0 0 30 2 *` never fires).
const NEXT: usize = 5;
const SEARCH_DAYS: i64 = 8 * 366;

/// One field: the values it allows (bit `n` for value `n`) and whether it was `*` or `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Field {
    bits: u64,
    any: bool,
    min: u32,
    max: u32,
}

impl Field {
    fn has(&self, value: u32) -> bool {
        self.bits & (1 << value) != 0
    }

    fn values(&self) -> Vec<u32> {
        (self.min..=self.max).filter(|v| self.has(*v)).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Schedule {
    second: Option<Field>,
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

enum Spec {
    Reboot,
    At(Schedule),
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// The decoded view, or `None` when `text` is not a cron schedule.
pub fn decode(text: &str, env: &Env) -> Option<String> {
    match parse(text)? {
        Spec::Reboot => Some(env.pick("Bij het opstarten", "At startup").to_string()),
        Spec::At(schedule) => Some(view(&schedule, env)),
    }
}

fn parse(text: &str) -> Option<Spec> {
    let text = text.trim();
    if text.contains('\n') || text.len() > 200 {
        return None;
    }
    let expanded = match text.to_ascii_lowercase().as_str() {
        "@reboot" => return Some(Spec::Reboot),
        "@yearly" | "@annually" => "0 0 1 1 *".to_string(),
        "@monthly" => "0 0 1 * *".to_string(),
        "@weekly" => "0 0 * * 0".to_string(),
        "@daily" | "@midnight" => "0 0 * * *".to_string(),
        "@hourly" => "0 * * * *".to_string(),
        _ if text.starts_with('@') => return None,
        _ => {
            // Plain numbers are not a schedule: one field needs `*`, `?`, `/` or a name.
            if !text
                .bytes()
                .any(|b| matches!(b, b'*' | b'?' | b'/') || b.is_ascii_alphabetic())
            {
                return None;
            }
            text.to_string()
        }
    };
    let parts: Vec<&str> = expanded.split_whitespace().collect();
    let (second, rest) = match parts.len() {
        5 => (None, &parts[..]),
        6 => (Some(field(parts[0], 0, 59, &[], false)?), &parts[1..]),
        _ => return None,
    };
    let mut weekday = field(rest[4], 0, 7, &DAYS, true)?;
    // 7 is Sunday too.
    if weekday.has(7) {
        weekday.bits = (weekday.bits & !(1 << 7)) | 1;
    }
    weekday.max = 6;
    Some(Spec::At(Schedule {
        second,
        minute: field(rest[0], 0, 59, &[], false)?,
        hour: field(rest[1], 0, 23, &[], false)?,
        day: field(rest[2], 1, 31, &[], true)?,
        month: field(rest[3], 1, 12, &MONTHS, false)?,
        weekday,
    }))
}

/// `*`, `?` (where `question` allows it), `n`, `a-b`, any of them `/step`, comma-separated;
/// `names` are the values from `min` (months from 1, weekdays from 0).
fn field(text: &str, min: u32, max: u32, names: &[&str], question: bool) -> Option<Field> {
    let mut bits = 0u64;
    let mut any = false;
    for item in text.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((range, step)) => (range, Some(step.parse::<u32>().ok().filter(|s| *s > 0)?)),
            None => (item, None),
        };
        let (from, to) = if range == "*" || (question && range == "?") {
            if step.is_none() && !text.contains(',') {
                any = true;
            }
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (value(a, min, names)?, value(b, min, names)?)
        } else {
            let from = value(range, min, names)?;
            (from, if step.is_some() { max } else { from })
        };
        if from < min || to > max || from > to {
            return None;
        }
        let mut at = from;
        while at <= to {
            bits |= 1 << at;
            at += step.unwrap_or(1);
        }
    }
    Some(Field {
        bits,
        any,
        min,
        max,
    })
}

fn value(text: &str, min: u32, names: &[&str]) -> Option<u32> {
    if text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty() && text.len() <= 2 {
        return text.parse().ok();
    }
    let lower = text.to_ascii_lowercase();
    names
        .iter()
        .position(|name| *name == lower)
        .map(|at| at as u32 + min)
}

const WEEKDAYS_NL: [&str; 7] = [
    "zondag",
    "maandag",
    "dinsdag",
    "woensdag",
    "donderdag",
    "vrijdag",
    "zaterdag",
];
const WEEKDAYS_EN: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS_NL: [&str; 12] = [
    "januari",
    "februari",
    "maart",
    "april",
    "mei",
    "juni",
    "juli",
    "augustus",
    "september",
    "oktober",
    "november",
    "december",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn weekday_name(day: u32, lang: Lang) -> &'static str {
    match lang {
        Lang::Nl => WEEKDAYS_NL[day as usize % 7],
        Lang::En => WEEKDAYS_EN[day as usize % 7],
    }
}

fn month_name(month: u32, lang: Lang) -> &'static str {
    let at = (month as usize).saturating_sub(1) % 12;
    match lang {
        Lang::Nl => MONTHS_NL[at],
        Lang::En => MONTHS_EN[at],
    }
}

/// A field's values as runs: single values and ranges of three or more.
enum Run {
    One(u32),
    Span(u32, u32),
}

fn runs(values: &[u32]) -> Vec<Run> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < values.len() {
        let mut end = at;
        while end + 1 < values.len() && values[end + 1] == values[end] + 1 {
            end += 1;
        }
        if end - at >= 2 {
            out.push(Run::Span(values[at], values[end]));
            at = end + 1;
        } else {
            out.push(Run::One(values[at]));
            at += 1;
        }
    }
    out
}

/// `a, b en c` / `a, b and c`.
fn join(items: &[String], env: &Env) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} {} {last}", rest.join(", "), env.pick("en", "and")),
    }
}

/// `1–5`, `1, 3 en 5`, with `name` for each value.
fn list(field: &Field, env: &Env, name: &dyn Fn(u32) -> String) -> String {
    let through = env.pick("t/m", "through");
    let items: Vec<String> = runs(&field.values())
        .iter()
        .map(|run| match run {
            Run::One(v) => name(*v),
            Run::Span(a, b) => format!("{} {through} {}", name(*a), name(*b)),
        })
        .collect();
    join(&items, env)
}

/// The step of a field that is `*/n` (every n from its minimum to the end), if it is one.
fn step_of(field: &Field) -> Option<u32> {
    let values = field.values();
    let step = values.get(1)?.checked_sub(values[0])?;
    let regular = values[0] == field.min
        && step > 1
        && values.windows(2).all(|pair| pair[1] - pair[0] == step)
        && values.last().is_some_and(|last| last + step > field.max);
    regular.then_some(step)
}

fn single(field: &Field) -> Option<u32> {
    let values = field.values();
    (values.len() == 1).then(|| values[0])
}

fn clock_time(hour: u32, minute: u32, second: Option<u32>) -> String {
    match second {
        Some(s) if s > 0 => format!("{hour:02}:{minute:02}:{s:02}"),
        _ => format!("{hour:02}:{minute:02}"),
    }
}

/// When in the day: "om 08:00" / "at 08:00" (`true`: it reads after the day part), or a
/// phrase such as "elke 5 minuten".
fn time_part(s: &Schedule, env: &Env) -> (String, bool) {
    let second = match s.second {
        None => Some(0),
        Some(field) => single(&field),
    };
    if let (Some(minute), Some(sec)) = (single(&s.minute), second) {
        let hours = s.hour.values();
        if !s.hour.any && hours.len() <= 6 && step_of(&s.hour).is_none() {
            let times: Vec<String> = hours
                .iter()
                .map(|hour| clock_time(*hour, minute, Some(sec)))
                .collect();
            return (
                format!("{} {}", env.pick("om", "at"), join(&times, env)),
                true,
            );
        }
    }
    let mut parts = Vec::new();
    if let Some(field) = s.second.filter(|field| single(field) != Some(0)) {
        parts.push(unit_phrase(
            &field,
            env,
            ("seconde", "seconden"),
            ("second", "seconds"),
        ));
    }
    let minute = unit_phrase(&s.minute, env, ("minuut", "minuten"), ("minute", "minutes"));
    let hour = (!s.hour.any).then(|| match step_of(&s.hour) {
        Some(step) => format!(
            "{} {step} {}",
            env.pick("elke", "every"),
            env.pick("uur", "hours")
        ),
        None => format!(
            "{} {}",
            env.pick("tijdens uur", "during hour"),
            list(&s.hour, env, &|h| format!("{h:02}"))
        ),
    });
    if single(&s.minute).is_some() {
        // "elk uur op minuut 15", "elke 2 uur op minuut 0".
        let hours = hour.unwrap_or_else(|| env.pick("elk uur", "every hour").to_string());
        parts.push(format!("{hours} {minute}"));
    } else {
        parts.push(minute);
        parts.extend(hour);
    }
    (parts.join(", "), false)
}

/// `elke minuut`, `elke 5 minuten`, `op minuut 0 en 30` (and the same for seconds).
fn unit_phrase(field: &Field, env: &Env, nl: (&str, &str), en: (&str, &str)) -> String {
    let (one, many) = match env.lang {
        Lang::Nl => nl,
        Lang::En => en,
    };
    if field.any {
        return format!("{} {one}", env.pick("elke", "every"));
    }
    if let Some(step) = step_of(field) {
        return format!("{} {step} {many}", env.pick("elke", "every"));
    }
    let values = field.values();
    let unit = if values.len() == 1 { one } else { many };
    format!(
        "{} {unit} {}",
        env.pick("op", "at"),
        list(field, env, &|v| v.to_string())
    )
}

/// Which days: "elke dag", "elke maandag", "maandag t/m vrijdag", "op dag 1 van de maand",
/// then the months.
fn day_part(s: &Schedule, env: &Env) -> String {
    let lang = env.lang;
    let weekdays = || -> String {
        if let Some(day) = single(&s.weekday) {
            return format!("{} {}", env.pick("elke", "every"), weekday_name(day, lang));
        }
        let named = list(&s.weekday, env, &|d| weekday_name(d, lang).to_string());
        match runs(&s.weekday.values()).as_slice() {
            [Run::Span(..)] => named,
            _ => format!("{} {named}", env.pick("op", "on")),
        }
    };
    let month_days = || -> String {
        let days = list(&s.day, env, &|d| d.to_string());
        let unit = if single(&s.day).is_some() {
            env.pick("dag", "day")
        } else {
            env.pick("dag", "days")
        };
        format!(
            "{} {unit} {days} {}",
            env.pick("op", "on"),
            env.pick("van de maand", "of the month")
        )
    };
    let mut out = match (s.day.any, s.weekday.any) {
        (true, true) => env.pick("elke dag", "every day").to_string(),
        (true, false) => weekdays(),
        (false, true) => month_days(),
        (false, false) => format!("{} {} {}", month_days(), env.pick("of", "or"), weekdays()),
    };
    if !s.month.any {
        let months = list(&s.month, env, &|m| month_name(m, lang).to_string());
        let lead = match runs(&s.month.values()).as_slice() {
            [Run::Span(..)] => env.pick("van", "from"),
            _ => env.pick("in", "in"),
        };
        out.push_str(&format!(" {lead} {months}"));
    }
    out
}

fn sentence(s: &Schedule, env: &Env) -> String {
    let (time, after_day) = time_part(s, env);
    let every_day = s.day.any && s.weekday.any && s.month.any;
    let text = if after_day {
        format!("{} {time}", day_part(s, env))
    } else if every_day {
        time
    } else {
        format!("{time}, {}", day_part(s, env))
    };
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn weekday_of(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

/// The next times `s` fires after `now` (Unix seconds), as (Unix seconds, local wall seconds).
/// A local time that does not exist (the hour skipped for summer time) is passed over.
fn next_times(s: &Schedule, now: i64, env: &Env) -> Vec<(i64, i64)> {
    let local_now = now + (env.offset)(now);
    let first_day = local_now.div_euclid(86_400);
    let seconds = s.second.map_or(vec![0], |field| field.values());
    let (minutes, hours) = (s.minute.values(), s.hour.values());
    let mut out = Vec::new();
    for days in first_day..first_day + SEARCH_DAYS {
        let (_, month, day) = clock::civil_from_days(days);
        if !s.month.has(month) {
            continue;
        }
        let by_day = s.day.has(day);
        let by_weekday = s.weekday.has(weekday_of(days));
        let fires = match (s.day.any, s.weekday.any) {
            (false, false) => by_day || by_weekday,
            _ => by_day && by_weekday,
        };
        if !fires {
            continue;
        }
        for hour in &hours {
            for minute in &minutes {
                for second in &seconds {
                    let wall = days * 86_400
                        + i64::from(*hour) * 3600
                        + i64::from(*minute) * 60
                        + i64::from(*second);
                    if wall <= local_now {
                        continue;
                    }
                    let unix = wall - (env.offset)(wall - (env.offset)(wall));
                    if unix + (env.offset)(unix) != wall || unix <= now {
                        continue;
                    }
                    out.push((unix, wall));
                    if out.len() == NEXT {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn view(s: &Schedule, env: &Env) -> String {
    let now = env.now_ms.div_euclid(1000);
    let lang = env.lang;
    let times = next_times(s, now, env);
    let mut out = sentence(s, env);
    out.push_str("\n\n");
    if times.is_empty() {
        out.push_str(env.pick(
            "Geen volgende keer in de komende acht jaar",
            "No next time in the coming eight years",
        ));
    } else {
        out.push_str(env.pick("Volgende keren (lokale tijd):", "Next times (local time):"));
        for (unix, wall) in &times {
            let weekday = weekday_name(weekday_of(wall.div_euclid(86_400)), lang);
            let letters = if lang == Lang::Nl { 2 } else { 3 };
            let short: String = weekday.chars().take(letters).collect();
            out.push_str(&format!(
                "\n{short} {}  ({})",
                clock::date_time(*wall, None, env.month_first),
                clock::offset_label(wall - unix)
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "cron_tests.rs"]
mod tests;
