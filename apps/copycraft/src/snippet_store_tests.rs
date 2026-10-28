use super::*;

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "copycraft-snippets-test-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn stored(name: &str, text: &str) -> Stored {
    Stored {
        name: Zeroizing::new(name.to_string()),
        text: Zeroizing::new(text.to_string()),
    }
}

fn token() -> String {
    format!("ghp_{}", "1234567890abcdefghijklmnopqrstuvwxyzAB")
}

#[test]
fn snippets_survive_a_quit_and_a_new_start() {
    let dir = temp("survive");
    let snippets = vec![
        stored("Groet", "Hoi {{naam}},\n\"tot {{datum}}\" \\ ✓"),
        stored("Bon", "Order {{order}} op {{tijd}}"),
    ];
    save(&dir, &snippets).expect("save");
    let path = dir.join(FILE);
    let mode = std::fs::metadata(&path).expect("file").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    // No temporary file is left behind.
    let names: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from(FILE)]);
    // "Quit and start again": read back from disk only.
    assert_eq!(load(&dir), snippets);
    // Saving again replaces the file; no snippet left removes it.
    save(&dir, &snippets[1..]).expect("save");
    assert_eq!(load(&dir), snippets[1..].to_vec());
    save(&dir, &[]).expect("save");
    assert!(!path.exists());
    assert!(load(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_snippet_with_a_secret_is_never_written() {
    let dir = temp("secret");
    let secret_text = stored("Deploy", &format!("export GITHUB_TOKEN={}", token()));
    let secret_name = stored(&token(), "Gewone tekst");
    assert!(holds_secret(&secret_text.name, &secret_text.text));
    assert!(holds_secret(&secret_name.name, &secret_name.text));
    save(
        &dir,
        &[secret_text.clone(), stored("Groet", "Hoi"), secret_name],
    )
    .expect("save");
    let written = std::fs::read_to_string(dir.join(FILE)).expect("file");
    assert!(!written.contains("ghp_"));
    assert!(!written.contains("Deploy"));
    assert_eq!(load(&dir), vec![stored("Groet", "Hoi")]);
    // Only secrets: nothing is written at all.
    std::fs::remove_file(dir.join(FILE)).expect("remove");
    save(&dir, &[secret_text]).expect("save");
    assert!(!dir.join(FILE).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_secret_in_the_file_is_dropped_when_read() {
    let file = format!(
        "{{\"version\":1,\"snippets\":[{{\"name\":\"Sleutel\",\"text\":\"token {}\"}},{{\"name\":\"Groet\",\"text\":\"Hoi\"}}]}}",
        token()
    );
    assert_eq!(decode(file.as_bytes()), vec![stored("Groet", "Hoi")]);
}

#[test]
fn a_corrupt_or_unreadable_file_is_an_empty_list() {
    for bytes in [
        &b""[..],
        b"not json",
        b"{\"version\":1,\"snippets\":[{\"name\":\"a\"",
        b"{\"version\":2,\"snippets\":[{\"name\":\"a\",\"text\":\"b\"}]}",
        b"{\"version\":1,\"snippets\":{}}",
        b"[1,2,3]",
        &[0xff, 0xfe, 0x00][..],
    ] {
        assert!(decode(bytes).is_empty());
    }
    // Wrong fields in one item drop that item only.
    assert_eq!(
        decode(b"{\"version\":1,\"snippets\":[{\"name\":7,\"text\":\"b\"},{\"name\":\" \",\"text\":\"b\"},{\"name\":\"a\",\"text\":\"b\"}]}"),
        vec![stored("a", "b")]
    );
    let dir = temp("corrupt");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(FILE), b"{\"version\":1,\"snip").expect("write");
    assert!(load(&dir).is_empty());
    // A folder where the file should be.
    std::fs::remove_file(dir.join(FILE)).expect("remove");
    std::fs::create_dir_all(dir.join(FILE)).expect("dir");
    assert!(load(&dir).is_empty());
    assert!(load(&dir.join("missing")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn at_most_the_pin_list_is_read() {
    let many: Vec<Stored> = (0..30)
        .map(|n| stored(&format!("s{n}"), &format!("tekst {n}")))
        .collect();
    let encoded = encode(&many);
    assert_eq!(decode(encoded.as_bytes()).len(), crate::clipboard::PIN_MAX);
}

#[test]
fn pinned_snippets_come_back_pinned_after_a_restart_and_clearing_history_keeps_them() {
    use crate::clipboard::ClipboardHistory;
    let dir = temp("restart");
    let mut history = ClipboardHistory::default();
    history.record("Hoi {{naam}}".into());
    let id = history.id_at(0).expect("id");
    assert!(history.pin(id));
    assert!(history.name_snippet(id, "Groet"));
    history.record("gewone kopie".into());
    history.record("vastgezet, geen fragment".into());
    assert!(history.pin(history.id_at(0).expect("id")));
    // Clear History (unpinned only) keeps the snippet.
    history.clear_unpinned();
    assert_eq!(history.snippets().len(), 1);
    let stored: Vec<Stored> = history
        .snippets()
        .into_iter()
        .map(|(name, text)| Stored { name, text })
        .collect();
    save(&dir, &stored).expect("save");
    drop(history);

    // A new start: only the snippet comes back, pinned and named, after what is there.
    let mut fresh = ClipboardHistory::default();
    fresh.record("nieuwste kopie".into());
    for snippet in load(&dir) {
        assert!(fresh.restore_snippet(&snippet.name, &snippet.text));
    }
    assert_eq!(fresh.len(), 2);
    assert_eq!(fresh.get(0), Some("nieuwste kopie"));
    assert_eq!(fresh.get(1), Some("Hoi {{naam}}"));
    assert!(fresh.is_pinned(1));
    assert_eq!(fresh.snippet_name(1), Some("Groet"));
    // Restoring the same again does not make a second entry.
    assert!(fresh.restore_snippet("Groet", "Hoi {{naam}}"));
    assert_eq!(fresh.len(), 2);
    // Unpinning takes it out of what is saved.
    let id = fresh.id_at(1).expect("id");
    assert!(fresh.unpin(id));
    assert!(fresh.snippets().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
