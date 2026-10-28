use super::decode;
use crate::decode::clock::Env;
use crate::decode::clock::tests::env;
use crate::locale::Lang;

/// The first line: the schedule in words.
fn words(text: &str, lang: Lang) -> String {
    let out = decode(text, &env(lang)).unwrap_or_else(|| panic!("{text}"));
    out.lines().next().unwrap_or_default().to_string()
}

#[test]
fn schedules_in_words() {
    for (text, nl, en) in [
        (
            "0 8 * * 1",
            "Elke maandag om 08:00",
            "Every Monday at 08:00",
        ),
        (
            "0 8 * * MON",
            "Elke maandag om 08:00",
            "Every Monday at 08:00",
        ),
        (
            "30 9 * * 1-5",
            "Maandag t/m vrijdag om 09:30",
            "Monday through Friday at 09:30",
        ),
        (
            "0 9 * * 1,3,5",
            "Op maandag, woensdag en vrijdag om 09:00",
            "On Monday, Wednesday and Friday at 09:00",
        ),
        (
            "0 9,12,18 * * *",
            "Elke dag om 09:00, 12:00 en 18:00",
            "Every day at 09:00, 12:00 and 18:00",
        ),
        ("*/5 * * * *", "Elke 5 minuten", "Every 5 minutes"),
        ("* * * * *", "Elke minuut", "Every minute"),
        (
            "15 * * * *",
            "Elk uur op minuut 15",
            "Every hour at minute 15",
        ),
        (
            "0 */2 * * *",
            "Elke 2 uur op minuut 0",
            "Every 2 hours at minute 0",
        ),
        (
            "*/10 9-17 * * 1-5",
            "Elke 10 minuten, tijdens uur 09 t/m 17, maandag t/m vrijdag",
            "Every 10 minutes, during hour 09 through 17, Monday through Friday",
        ),
        (
            "@monthly",
            "Op dag 1 van de maand om 00:00",
            "On day 1 of the month at 00:00",
        ),
        ("@daily", "Elke dag om 00:00", "Every day at 00:00"),
        ("@hourly", "Elk uur op minuut 0", "Every hour at minute 0"),
        ("@weekly", "Elke zondag om 00:00", "Every Sunday at 00:00"),
        ("0 0 * * 7", "Elke zondag om 00:00", "Every Sunday at 00:00"),
        (
            "0 0 1,15 * 1",
            "Op dag 1 en 15 van de maand of elke maandag om 00:00",
            "On days 1 and 15 of the month or every Monday at 00:00",
        ),
        (
            "0 12 * 1,7 *",
            "Elke dag in januari en juli om 12:00",
            "Every day in January and July at 12:00",
        ),
        (
            "0 12 1 3-5 *",
            "Op dag 1 van de maand van maart t/m mei om 12:00",
            "On day 1 of the month from March through May at 12:00",
        ),
        ("0 0 8 * * ?", "Elke dag om 08:00", "Every day at 08:00"),
        (
            "30 0 8 * * MON",
            "Elke maandag om 08:00:30",
            "Every Monday at 08:00:30",
        ),
        (
            "*/15 * * * * *",
            "Elke 15 seconden, elke minuut",
            "Every 15 seconds, every minute",
        ),
        ("@reboot", "Bij het opstarten", "At startup"),
    ] {
        assert_eq!(words(text, Lang::Nl), nl, "{text}");
        assert_eq!(words(text, Lang::En), en, "{text}");
    }
}

#[test]
fn the_next_five_times_in_local_time() {
    // The test clock: Tuesday 2026-10-06 08:00 UTC, 10:00 local (UTC+02:00).
    let out = decode("0 8 * * 1", &env(Lang::En)).expect("cron");
    assert_eq!(
        out,
        "Every Monday at 08:00\n\nNext times (local time):\n\
         Mon 12/10/2026 08:00:00  (UTC+02:00)\n\
         Mon 19/10/2026 08:00:00  (UTC+02:00)\n\
         Mon 26/10/2026 08:00:00  (UTC+02:00)\n\
         Mon 02/11/2026 08:00:00  (UTC+02:00)\n\
         Mon 09/11/2026 08:00:00  (UTC+02:00)"
    );
    let out = decode("*/5 * * * *", &env(Lang::Nl)).expect("cron");
    assert!(out.contains("Volgende keren (lokale tijd):\ndi 06/10/2026 10:05:00  (UTC+02:00)\ndi 06/10/2026 10:10:00"), "{out}");
    let out = decode("0 0 30 2 *", &env(Lang::En)).expect("cron");
    assert!(
        out.ends_with("No next time in the coming eight years"),
        "{out}"
    );
    let out = decode("0 0 29 2 *", &env(Lang::En)).expect("cron");
    assert!(
        out.contains("Tue 29/02/2028 00:00:00  (UTC+02:00)\nSun 29/02/2032"),
        "{out}"
    );
}

#[test]
fn a_skipped_summer_hour_is_passed_over() {
    // Summer time from 2026-03-29 01:00 UTC: 02:00–03:00 local does not exist that night.
    fn amsterdam(unix: i64) -> i64 {
        if unix >= 1_774_746_000 { 7200 } else { 3600 }
    }
    let clock = Env {
        now_ms: 1_774_699_200_000, // 2026-03-28 12:00 UTC
        lang: Lang::En,
        month_first: false,
        offset: amsterdam,
        zone: |_, _| None,
    };
    let out = decode("30 2 * * *", &clock).expect("cron");
    assert!(!out.contains("29/03/2026"), "{out}");
    assert!(
        out.contains("Mon 30/03/2026 02:30:00  (UTC+02:00)"),
        "{out}"
    );
    let out = decode("30 1 * * *", &clock).expect("cron");
    assert!(
        out.contains("Sun 29/03/2026 01:30:00  (UTC+01:00)"),
        "{out}"
    );
}

#[test]
fn other_text_is_no_schedule() {
    for text in [
        "1 2 3 4 5",
        "* * * *",
        "* * * * * * *",
        "60 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "* * * 13 *",
        "* * * * 8",
        "5-1 * * * *",
        "*/0 * * * *",
        "hello * world * x",
        "@never",
        "0 8 * * 1\n0 9 * * 2",
        "a * b * c",
        "? * * * *",
    ] {
        assert_eq!(decode(text, &env(Lang::En)), None, "{text}");
    }
    assert_eq!(
        crate::decode::detect("0 8 * * 1"),
        Some(crate::decode::DecodeKind::Cron)
    );
    assert_eq!(crate::decode::detect("1 2 3 4 5"), None);
}
