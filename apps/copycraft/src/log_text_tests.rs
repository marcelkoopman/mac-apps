use super::{LogKind, detect, is_crash_log, is_log};
use crate::sensitivity::{Label, scan_labels};

/// A made-up macOS crash report: every name, id and address is fake.
pub(crate) const CRASH: &str = "-------------------------------------
Translated Report (Full Report Below)
-------------------------------------

Process:               CopyCraft [48213]
Path:                  /Applications/CopyCraft.app/Contents/MacOS/CopyCraft
Identifier:            nl.example.copycraft
Version:               26.4.1 (412)
Code Type:             ARM-64 (Native)
Parent Process:        launchd [1]
Coalition:             nl.example.copycraft [9182]
User ID:               501

Date/Time:             2026-10-28 11:02:17.4821 +0100
OS Version:            macOS 27.2 (26C61)
Report Version:        12
Anonymous UUID:        5D2C1A8E-3B7F-4C1D-9E0A-1F2B3C4D5E6F

Time Awake Since Boot: 412345 seconds

Crashed Thread:        0  Dispatch queue: com.apple.main-thread

Exception Type:        EXC_BAD_ACCESS (SIGSEGV)
Exception Codes:       KERN_INVALID_ADDRESS at 0x0000411111111111
Exception Codes:       0x0000000000000001, 0x4111111111111111

Termination Reason:    Namespace SIGNAL, Code 11 Segmentation fault: 11
Terminating Process:   exc handler [48213]

Thread 0 Crashed::  Dispatch queue: com.apple.main-thread
0   libobjc.A.dylib               \t       0x18a2b4c20 objc_release + 16
1   AppKit                        \t       0x18e7d1a2c -[NSThemeFrame dealloc] + 64
2   CopyCraft                     \t       0x1047c3e18 0x104600000 + 1849880
3   CopyCraft                     \t       0x1047c3e18 4111111111111111 + 4111111111111111
4   dyld                          \t       0x18a0f2b98 start + 6076

Thread 0 crashed with ARM Thread State (64-bit):
    x0: 0x0000000000000000   x1: 0x00000001f4111111   x2: 0x0000000000000020
    fp: 0x000000016b9fe5a0   lr: 0x000000018a2b4c20   sp: 0x000000016b9fe580
    pc: 0x000000018a2b4c20 cpsr: 0x20000000

Binary Images:
       0x104600000 -        0x104b2ffff nl.example.copycraft (26.4.1) <3c1d9e0a-1f2b-3c4d-5e6f-0a1b2c3d4e5f> /Applications/CopyCraft.app/Contents/MacOS/CopyCraft
       0x18a2a0000 -        0x18a2e3fff libobjc.A.dylib (*) <1a2b3c4d-5e6f-7081-92a3-b4c5d6e7f809> /usr/lib/libobjc.A.dylib

Device: 10.0.0.12  fe80::1  aa:bb:cc:dd:ee:ff  pid 123456782  4111 1111 1111 1111  NL91ABNA0417164300
";

/// A made-up log: timestamps, pids and a thread id.
const LOG: &str = "2026-10-28 11:02:17.482113+0100 0x1a2b3c    Default     0x0   48213  0    CopyCraft: started 123456782
2026-10-28 11:02:17.512000+0100 0x1a2b3c    Info        0x0   48213  0    CopyCraft: card 1092344577040057 built
2026-10-28 11:02:18.000001+0100 0x1a2b3d    Error       0x0   48213  0    CopyCraft: user 501 at 10.0.0.12
2026-10-28 11:02:18.100000+0100 0x1a2b3d    Default     0x0   48213  0    CopyCraft: quit
";

#[test]
fn a_crash_report_is_a_crash_log() {
    assert!(is_crash_log(CRASH));
    assert_eq!(detect(CRASH), Some(LogKind::Crash));
    assert_eq!(
        crate::format::detect(CRASH),
        crate::format::FormatKind::Text
    );
    // The `.ips` form: one line of JSON metadata, then the report as JSON.
    let ips = "{\"app_name\":\"CopyCraft\",\"bug_type\":\"309\"}\n{\n  \"pid\" : 48213,\n  \"exception\" : {\"type\":\"EXC_BAD_ACCESS\"}\n}\n";
    assert_eq!(detect(ips), Some(LogKind::Crash));
}

#[test]
fn timestamped_lines_are_a_log() {
    assert!(is_log(LOG));
    assert_eq!(detect(LOG), Some(LogKind::Log));
    let syslog = "Oct 28 11:02:17 mac CopyCraft[482]: one\nOct 28 11:02:18 mac CopyCraft[482]: two\nOct 28 11:02:19 mac CopyCraft[482]: three\n";
    assert_eq!(detect(syslog), Some(LogKind::Log));
    let bracketed = "[2026-10-28T10:02:17Z] INFO one\n[2026-10-28T10:02:18Z] WARN two\n[2026-10-28T10:02:19Z] INFO three\n";
    assert_eq!(detect(bracketed), Some(LogKind::Log));
}

#[test]
fn tables_prose_and_short_texts_are_no_log() {
    for text in [
        "Datum,Bedrag\n2026-04-16 10:00:00,3.17\n2026-04-17 10:00:00,4.20\n2026-04-18 10:00:00,5.00\n",
        "Datum\tBedrag\n2026-04-16 10:00:00\t3.17\n2026-04-17 10:00:00\t4.20\n2026-04-18 10:00:00\t5.00\n",
        "2026-10-28 11:02:17 one line only",
        "Process: write the notes\nPath: up the hill\nVersion: two\n",
        "Meeting notes\n\nWe talked about the exception type and the crashed thread.\n",
    ] {
        assert_eq!(detect(text), None, "{text}");
    }
}

#[test]
fn a_crash_log_or_log_has_no_personal_or_financial_label() {
    assert_eq!(scan_labels(CRASH), Vec::<Label>::new());
    assert_eq!(scan_labels(LOG), Vec::<Label>::new());
}

#[test]
fn a_token_in_a_crash_log_or_log_still_counts() {
    let token = concat!("gh", "p_", "aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789");
    let crash = format!("{CRASH}Environment: GITHUB_TOKEN={token}\n");
    assert_eq!(scan_labels(&crash), vec![Label::Credential]);
    let log = format!("{LOG}2026-10-28 11:02:19.000000+0100 0x1 Default 0x0 1 0 x: {token}\n");
    assert_eq!(scan_labels(&log), vec![Label::Credential]);
    let url_password = format!(
        "{LOG}2026-10-28 11:02:19.000000+0100 0x1 Default 0x0 1 0 x: {}\n",
        concat!("postgres://admin:", "hunter2@db.internal/app")
    );
    assert_eq!(scan_labels(&url_password), vec![Label::Credential]);
}

#[test]
fn the_card_title_names_the_crash_log_or_log() {
    assert_eq!(crate::commands::text_title(CRASH), "Crash log");
    assert_eq!(crate::commands::text_title(LOG), "Log");
}
