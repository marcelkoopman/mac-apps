use super::*;
use std::io::Cursor;
use std::sync::atomic::AtomicBool;

static NO_STOP: AtomicBool = AtomicBool::new(false);

/// One entry for [`build`]: name, data, deflate, unix mode (0 = none), flags.
struct E<'a> {
    name: &'a [u8],
    data: &'a [u8],
    deflate: bool,
    mode: u32,
    flags: u16,
}

fn e<'a>(name: &'a str, data: &'a [u8]) -> E<'a> {
    E {
        name: name.as_bytes(),
        data,
        deflate: true,
        mode: 0,
        flags: 0,
    }
}

fn crc(data: &[u8]) -> u32 {
    let mut hasher = flate2::Crc::new();
    hasher.update(data);
    hasher.sum()
}

/// A zip written by hand; `zip64` writes the sizes and offsets in the zip64 extra field and
/// the zip64 end records.
fn build(entries: &[E], zip64: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut dir = Vec::new();
    for entry in entries {
        let packed = if entry.deflate {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(entry.data).unwrap();
            enc.finish().unwrap()
        } else {
            entry.data.to_vec()
        };
        let method: u16 = if entry.deflate { 8 } else { 0 };
        let offset = out.len() as u64;
        let extra = if zip64 {
            let mut extra = Vec::new();
            extra.extend_from_slice(&1u16.to_le_bytes());
            extra.extend_from_slice(&24u16.to_le_bytes());
            extra.extend_from_slice(&(entry.data.len() as u64).to_le_bytes());
            extra.extend_from_slice(&(packed.len() as u64).to_le_bytes());
            extra.extend_from_slice(&offset.to_le_bytes());
            extra
        } else {
            Vec::new()
        };
        let small = |v: u64| if zip64 { 0xFFFF_FFFFu32 } else { v as u32 };
        let mut common = Vec::new();
        common.extend_from_slice(&20u16.to_le_bytes());
        common.extend_from_slice(&entry.flags.to_le_bytes());
        common.extend_from_slice(&method.to_le_bytes());
        common.extend_from_slice(&[0; 4]);
        common.extend_from_slice(&crc(entry.data).to_le_bytes());
        common.extend_from_slice(&small(packed.len() as u64).to_le_bytes());
        common.extend_from_slice(&small(entry.data.len() as u64).to_le_bytes());
        common.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&common);
        out.extend_from_slice(&0u16.to_le_bytes()); // no extra in the local header
        out.extend_from_slice(entry.name);
        out.extend_from_slice(&packed);
        dir.extend_from_slice(b"PK\x01\x02");
        let made_by: u16 = if entry.mode != 0 { 3 << 8 | 20 } else { 20 };
        dir.extend_from_slice(&made_by.to_le_bytes());
        dir.extend_from_slice(&common);
        dir.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        dir.extend_from_slice(&[0; 2 + 2 + 2]); // comment, disk, internal attrs
        dir.extend_from_slice(&(entry.mode << 16).to_le_bytes());
        dir.extend_from_slice(&small(offset).to_le_bytes());
        dir.extend_from_slice(entry.name);
        dir.extend_from_slice(&extra);
    }
    let dir_start = out.len() as u64;
    out.extend_from_slice(&dir);
    if zip64 {
        let record_at = out.len() as u64;
        out.extend_from_slice(b"PK\x06\x06");
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&[0; 2 + 2 + 4 + 4]);
        out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
        out.extend_from_slice(&(dir.len() as u64).to_le_bytes());
        out.extend_from_slice(&dir_start.to_le_bytes());
        out.extend_from_slice(b"PK\x06\x07");
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&record_at.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
    }
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0; 4]);
    let count: u16 = if zip64 { 0xFFFF } else { entries.len() as u16 };
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    let size: u32 = if zip64 { 0xFFFF_FFFF } else { dir.len() as u32 };
    let start: u32 = if zip64 { 0xFFFF_FFFF } else { dir_start as u32 };
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn listing(bytes: &[u8]) -> Result<Listing, ZipError> {
    list(&mut Cursor::new(bytes), bytes.len() as u64)
}

fn names(listing: &Listing) -> Vec<String> {
    listing.entries.iter().map(|e| e.name.clone()).collect()
}

/// A fresh, empty folder of the test's own.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("copycraft-zip-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn lists_files_and_folders() {
    let zip = build(
        &[
            e("docs/", b""),
            e("docs/a.txt", b"hello"),
            e("docs/sub/b.txt", b"world!"),
            e("readme.md", b"# hi"),
        ],
        false,
    );
    let listing = listing(&zip).unwrap();
    assert_eq!(
        names(&listing),
        ["docs/", "docs/a.txt", "docs/sub/b.txt", "readme.md"]
    );
    let facts = listing.facts();
    assert_eq!(facts.files, 3);
    assert_eq!(facts.folders, 2); // docs, docs/sub (implied)
    assert_eq!(facts.unpacked, 5 + 6 + 4);
    assert_eq!(facts.blocker(), None);
    assert_eq!(listing.top_names(), ["docs", "readme.md"]);
    let text = listing.text(|n| format!("+{n}"));
    assert_eq!(
        text.as_str(),
        "docs/  (2)\ndocs/a.txt\ndocs/sub/  (1)\ndocs/sub/b.txt\nreadme.md"
    );
}

#[test]
fn reads_zip64_and_stored_entries() {
    let mut stored = e("a.bin", b"stored bytes");
    stored.deflate = false;
    let zip = build(&[stored, e("b/c.txt", b"deflated")], true);
    let listing = listing(&zip).unwrap();
    assert_eq!(names(&listing), ["a.bin", "b/c.txt"]);
    assert_eq!(listing.facts().unpacked, 12 + 8);
}

#[test]
fn long_listings_are_cut_with_a_count() {
    let names: Vec<String> = (0..SHOWN_LINES + 5).map(|i| format!("f{i:05}")).collect();
    let entries: Vec<E> = names.iter().map(|n| e(n, b"")).collect();
    let listing = listing(&build(&entries, false)).unwrap();
    let text = listing.text(|n| format!("… {n} more"));
    assert_eq!(text.lines().count(), SHOWN_LINES + 1);
    assert_eq!(text.lines().last(), Some("… 5 more"));
}

#[test]
fn finder_resource_forks_are_left_out() {
    let zip = build(
        &[
            e("a.txt", b"a"),
            e("__MACOSX/", b""),
            e("__MACOSX/._a.txt", b"fork"),
        ],
        false,
    );
    assert_eq!(names(&listing(&zip).unwrap()), ["a.txt"]);
}

#[test]
fn empty_corrupt_and_not_a_zip() {
    assert_eq!(listing(&build(&[], false)).err(), Some(ZipError::Empty));
    assert_eq!(
        listing(&build(&[e("__MACOSX/._x", b"")], false)).err(),
        Some(ZipError::Empty)
    );
    assert_eq!(listing(b"hello, not a zip").err(), Some(ZipError::NotZip));
    assert_eq!(listing(b"").err(), Some(ZipError::NotZip));
    let mut zip = build(&[e("a.txt", b"abc")], false);
    // Break the central directory's signature.
    let dir = zip.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    zip[dir] = b'X';
    assert_eq!(listing(&zip).err(), Some(ZipError::Corrupt));
    // Cut off: the end record points past the data.
    let zip = build(&[e("a.txt", b"abc")], false);
    let cut = [&zip[..10], &zip[zip.len() - 22..]].concat();
    assert_eq!(listing(&cut).err(), Some(ZipError::Corrupt));
}

#[test]
fn encrypted_entries_are_locked() {
    let mut locked = e("secret.txt", b"x");
    locked.flags = 1;
    let listing = listing(&build(&[locked], false)).unwrap();
    assert_eq!(names(&listing), ["secret.txt"]);
    assert_eq!(listing.facts().blocker(), Some(ZipError::Locked));
}

#[test]
fn names_are_utf8_else_cp437() {
    assert_eq!(decode_name("café/ü.txt".as_bytes(), false), "café/ü.txt");
    assert_eq!(decode_name(b"caf\x82.txt", false), "café.txt");
    assert_eq!(decode_name(b"\x80\xff", false), "Ç\u{a0}");
    assert_eq!(CP437_HIGH.chars().count(), 128);
}

#[test]
fn safe_parts_refuse_escapes() {
    let parts = |name: &str| safe_parts(name);
    assert_eq!(parts("a/b.txt"), Some(vec!["a".into(), "b.txt".into()]));
    assert_eq!(parts("./a//b/"), Some(vec!["a".into(), "b".into()]));
    assert_eq!(parts("a\\b.txt"), Some(vec!["a".into(), "b.txt".into()]));
    assert_eq!(parts("..a/b..c"), Some(vec!["..a".into(), "b..c".into()]));
    assert_eq!(parts("./"), Some(vec![]));
    for bad in [
        "../evil.txt",
        "a/../../evil",
        "a/..",
        "/etc/passwd",
        "\\evil",
        "..\\evil",
        "C:/evil",
        "c:evil",
        "a\0b",
        "",
    ] {
        assert_eq!(parts(bad), None, "{bad:?}");
    }
    assert_eq!(parts(&"x".repeat(256)), None);
    assert!(parts(&"x".repeat(255)).is_some());
}

#[test]
fn unsafe_zips_list_but_do_not_extract() {
    let slip = build(&[e("ok.txt", b"ok"), e("../evil.txt", b"evil")], false);
    let listed = listing(&slip).unwrap();
    // The names still show, as stored.
    assert!(listed.text(|_| String::new()).contains("../evil.txt"));
    assert_eq!(listed.facts().blocker(), Some(ZipError::Unsafe));

    let mut link = e("link", b"/etc/passwd");
    link.mode = 0o120_777;
    assert_eq!(
        listing(&build(&[link], false)).unwrap().facts().blocker(),
        Some(ZipError::Unsafe)
    );
    let twice = build(&[e("A.txt", b"1"), e("a.txt", b"2")], false);
    assert_eq!(
        listing(&twice).unwrap().facts().blocker(),
        Some(ZipError::Unsafe)
    );

    let dir = temp_dir("slip");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let zip = write_zip(&dir, "slip.zip", &slip);
    assert_eq!(
        extract(&zip, "slip.zip", &out, &NO_STOP),
        Err(ZipError::Unsafe)
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    assert!(!dir.join("evil.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_target_is_direct_unless_a_top_name_exists() {
    let top = vec!["docs".to_string(), "readme.md".to_string()];
    assert_eq!(plan_target(&top, |_| false), Target::Direct);
    assert_eq!(
        plan_target(&top, |name| name == "readme.md"),
        Target::Folder
    );
    assert_eq!(folder_name("Archive.zip", 1), "Archive");
    assert_eq!(folder_name("Archive.ZIP", 2), "Archive 2");
    assert_eq!(folder_name("my.photos.zip", 3), "my.photos 3");
    assert_eq!(folder_name(".zip", 1), ".zip");
    assert_eq!(folder_name("..", 1), "Archive");
    assert_eq!(folder_name("noext", 1), "noext");
}

#[test]
fn extracts_into_the_folder_and_never_overwrites() {
    let dir = temp_dir("extract");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut tool = e("bin/tool", b"#!/bin/sh\n");
    tool.mode = 0o100_755;
    let bytes = build(
        &[
            e("docs/", b""),
            e("docs/a.txt", b"hello"),
            e("docs/sub/b.txt", b"world"),
            tool,
        ],
        false,
    );
    let zip = write_zip(&dir, "Archive.zip", &bytes);

    // Nothing in the way: right into the folder.
    let first = extract(&zip, "Archive.zip", &out, &NO_STOP).unwrap();
    assert_eq!(
        first,
        Extracted {
            folder: out.clone(),
            own_folder: false
        }
    );
    assert_eq!(std::fs::read(out.join("docs/a.txt")).unwrap(), b"hello");
    assert_eq!(std::fs::read(out.join("docs/sub/b.txt")).unwrap(), b"world");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(out.join("bin/tool"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }

    // Again: "docs" is there now, so a new folder named after the zip.
    std::fs::write(out.join("docs/a.txt"), b"changed").unwrap();
    let second = extract(&zip, "Archive.zip", &out, &NO_STOP).unwrap();
    assert_eq!(
        second,
        Extracted {
            folder: out.join("Archive"),
            own_folder: true
        }
    );
    assert_eq!(std::fs::read(out.join("docs/a.txt")).unwrap(), b"changed");
    assert_eq!(
        std::fs::read(out.join("Archive/docs/a.txt")).unwrap(),
        b"hello"
    );

    // And again: "Archive 2".
    let third = extract(&zip, "Archive.zip", &out, &NO_STOP).unwrap();
    assert_eq!(third.folder, out.join("Archive 2"));
    // No hidden .partial folder stays behind.
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["Archive", "Archive 2", "bin", "docs"]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_damaged_entry_takes_back_what_was_made() {
    let dir = temp_dir("damaged");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let mut stored = e("z/last.txt", b"last entry");
    stored.deflate = false;
    let mut bytes = build(&[e("a/first.txt", b"first"), stored], false);
    // Flip a byte of the stored data: the CRC no longer matches.
    let at = bytes.windows(10).position(|w| w == b"last entry").unwrap();
    bytes[at] ^= 0xFF;
    let zip = write_zip(&dir, "bad.zip", &bytes);
    assert_eq!(
        extract(&zip, "bad.zip", &out, &NO_STOP),
        Err(ZipError::Corrupt)
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_bomb_stops_at_the_declared_size() {
    // Declares 4 bytes, unpacks to 100 000.
    let big = vec![b'a'; 100_000];
    let mut bytes = build(&[e("bomb.txt", &big)], false);
    let declared = (big.len() as u32).to_le_bytes();
    let mut replaced = 0;
    let mut at = 0;
    while let Some(pos) = bytes[at..].windows(4).position(|w| w == declared) {
        let pos = at + pos;
        bytes[pos..pos + 4].copy_from_slice(&4u32.to_le_bytes());
        replaced += 1;
        at = pos + 4;
    }
    assert_eq!(replaced, 2); // local header and directory
    let dir = temp_dir("bomb");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let zip = write_zip(&dir, "bomb.zip", &bytes);
    assert_eq!(
        extract(&zip, "bomb.zip", &out, &NO_STOP),
        Err(ZipError::Corrupt)
    );
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn locked_zips_are_not_extracted() {
    let dir = temp_dir("locked");
    let mut locked = e("secret.txt", b"x");
    locked.flags = 1;
    let zip = write_zip(&dir, "l.zip", &build(&[locked], false));
    assert_eq!(
        extract(&zip, "l.zip", &dir, &NO_STOP),
        Err(ZipError::Locked)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn zip_files_are_told_by_name_and_magic() {
    let dir = temp_dir("magic");
    assert!(is_zip_file(&write_zip(
        &dir,
        "a.zip",
        &build(&[e("x", b"")], false)
    )));
    assert!(is_zip_file(&write_zip(
        &dir,
        "empty.ZIP",
        &build(&[], false)
    )));
    assert!(!is_zip_file(&write_zip(&dir, "b.zip", b"not a zip")));
    assert!(!is_zip_file(&write_zip(
        &dir,
        "c.docx",
        &build(&[e("x", b"")], false)
    )));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn every_error_has_a_message() {
    for error in [
        ZipError::NotZip,
        ZipError::Corrupt,
        ZipError::Locked,
        ZipError::Unsupported,
        ZipError::Unsafe,
        ZipError::Empty,
        ZipError::TooLarge,
        ZipError::Read,
        ZipError::Write,
        ZipError::NoSpace,
        ZipError::Cancelled,
        ZipError::WorkFolderName,
    ] {
        assert_ne!(error.message(), "?");
    }
}

#[test]
fn control_and_bidi_characters_are_unsafe_and_shown() {
    for bad in [
        "factuur\u{202E}fdp.app",
        "a\nb.txt",
        "tab\there",
        "del\u{7F}",
        "c1\u{85}x",
        "x\u{2066}y",
        "x\u{2069}y",
        "x\u{200F}y",
        "dir\u{202A}/file",
        "line\u{2028}sep",
        "para\u{2029}sep",
        "zero\u{200B}width",
        "join\u{200C}er",
        "join\u{200D}er",
        "word\u{2060}joiner",
        "\u{FEFF}bom.txt",
        "soft\u{AD}hyphen",
        "tag\u{E0041}\u{E007F}",
        "tag\u{E0001}",
    ] {
        assert_eq!(safe_parts(bad), None, "{bad:?}");
    }
    assert!(safe_parts("gewoon é ü 漢字.txt").is_some());
    // Every end of the Cf/Zl/Zp table, and the characters just outside it.
    for &(first, last) in &FORMAT_AND_SEPARATORS {
        for c in [first, last] {
            assert!(is_deceptive(char::from_u32(c).unwrap()), "{c:X}");
        }
    }
    for c in [
        '\u{AC}',
        '\u{AE}',
        '\u{2027}',
        '\u{2030}',
        '\u{E0080}',
        '\u{FE0F}',
    ] {
        assert!(!is_deceptive(c), "{c:?}");
    }
    assert_eq!(visible("a\u{200B}b\u{E0041}"), "a\\u{200B}b\\u{E0041}");
    // Emoji: a joiner between two, U+FE0F or a skin tone straight after one.
    for good in [
        "\u{1F468}\u{200D}\u{1F4BB}.txt",
        "\u{1F44D}\u{1F3FD}",
        "\u{2764}\u{FE0F}",
        "\u{1F469}\u{1F3FD}\u{200D}\u{1F4BB}",
        "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
        "\u{1F3F4}\u{200D}\u{2620}\u{FE0F}",
        "1\u{FE0F}\u{20E3} notities.txt",
        "#\u{FE0F}\u{20E3}",
        "*\u{20E3}",
        "map 0\u{FE0F}\u{20E3}/9\u{20E3}.txt",
        "dir \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}/file \u{1F44D}\u{1F3FF}.txt",
    ] {
        assert!(safe_parts(good).is_some(), "{good:?}");
        assert_eq!(visible(good), good);
    }
    for bad in [
        "a\u{200D}b",
        "\u{200D}\u{1F4BB}",
        "\u{1F468}\u{200D}",
        "\u{1F468}\u{200D}/\u{1F4BB}",
        "dir/\u{200D}\u{1F4BB}",
        "\u{1F468}\u{200D}\u{200D}\u{1F4BB}",
        "a\u{200D}\u{1F4BB}",
        "\u{1F468}\u{200D}b",
        "\u{1F468}\u{FE0F}\u{1F3FD}\u{200D}\u{1F4BB}",
        "a\u{FE0F}",
        "\u{FE0F}.txt",
        "\u{1F44D}\u{FE0F}\u{FE0F}",
        "x\u{1F3FD}",
        "\u{1F44D}\u{1F3FD}\u{1F3FD}",
        "\u{1F3FD}",
        "1\u{FE0F}",
        "1\u{FE0F}x\u{20E3}",
        "a\u{FE0F}\u{20E3}",
        "1\u{FE0F}\u{FE0F}\u{20E3}",
        "\u{FE0F}\u{20E3}",
        "a\u{3164}.txt",
        "\u{3164}",
        "x\u{115F}y",
        "x\u{1160}y",
        "x\u{FFA0}y",
        "tag flag \u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
    ] {
        assert_eq!(safe_parts(bad), None, "{bad:?}");
    }
    assert_eq!(visible("a\u{200D}b\u{FE0F}"), "a\\u{200D}b\\u{FE0F}");
    // The emoji table's ends are emoji, the characters around them are not.
    for &(first, last) in &EXTENDED_PICTOGRAPHIC {
        for c in [first, last] {
            assert!(is_emoji(char::from_u32(c).unwrap()), "{c:X}");
        }
    }
    for c in [
        'a',
        '1',
        '#',
        '\u{A8}',
        '\u{AA}',
        '\u{1F3FB}',
        '\u{1F3FF}',
        '\u{FE0F}',
        '\u{200D}',
    ] {
        assert!(!is_emoji(c), "{c:?}");
    }
    let zip = build(
        &[
            e("ok.txt", b"ok"),
            e("factuur\u{202E}fdp.app", b"x"),
            e("two\nlines", b"y"),
        ],
        false,
    );
    let listed = listing(&zip).unwrap();
    assert_eq!(listed.facts().blocker(), Some(ZipError::Unsafe));
    let text = listed.text(|_| String::new());
    assert!(
        text.contains("factuur\\u{202E}fdp.app"),
        "{:?}",
        text.as_str()
    );
    assert!(text.contains("two\\u{000A}lines"));
    assert!(!text.chars().any(|c| c == '\u{202E}'));
    assert_eq!(text.lines().count(), 3);
}

#[test]
fn names_that_differ_only_in_case_are_unsafe() {
    let zip = build(&[e("Readme.txt", b"a"), e("README.TXT", b"b")], false);
    assert_eq!(
        listing(&zip).unwrap().facts().blocker(),
        Some(ZipError::Unsafe)
    );
    assert_eq!(name_key("Map/É.TXT"), name_key("map/é.txt"));
}

/// NFC (é as one character) and NFD (e + combining accent) are one name on APFS. Foundation does
/// the normalisation, so this runs on the Mac only.
#[cfg(target_os = "macos")]
#[test]
fn nfc_and_nfd_forms_of_one_name_are_unsafe() {
    let nfc = "caf\u{E9}.txt";
    let nfd = "cafe\u{301}.txt";
    assert_eq!(name_key(nfc), name_key(nfd));
    let zip = build(&[e(nfc, b"a"), e(nfd, b"b")], false);
    assert_eq!(
        listing(&zip).unwrap().facts().blocker(),
        Some(ZipError::Unsafe)
    );
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn too_little_free_space_writes_nothing() {
    let dir = temp_dir("space");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let zip = write_zip(&dir, "s.zip", &build(&[e("a.txt", b"hello")], false));
    assert_eq!(
        extract_with(&zip, "s.zip", &out, &NO_STOP, Some(1_000)),
        Err(ZipError::NoSpace)
    );
    assert!(names_in(&out).is_empty());
    assert!(extract_with(&zip, "s.zip", &out, &NO_STOP, Some(100_000_000)).is_ok());
    let facts = ZipFacts {
        files: 2,
        unpacked: 10,
        ..ZipFacts::default()
    };
    assert!(fits(&facts, None));
    assert!(fits(&facts, Some(SPACE_MARGIN + 10 + 2 * BLOCK)));
    assert!(!fits(&facts, Some(SPACE_MARGIN + 9 + 2 * BLOCK)));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_stopped_extraction_leaves_nothing() {
    let dir = temp_dir("stop");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let zip = write_zip(&dir, "s.zip", &build(&[e("a/b.txt", b"hello")], false));
    let stop = AtomicBool::new(true);
    assert_eq!(
        extract(&zip, "s.zip", &out, &stop),
        Err(ZipError::Cancelled)
    );
    assert!(names_in(&out).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_leftover_partial_folder_of_ours_is_removed_and_others_are_left() {
    let dir = temp_dir("partial");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    // A crash halfway, in Dutch: our marked work folder with a half file.
    let ours = out.join(partial_name("s.zip", 1, crate::locale::Lang::Nl));
    assert_eq!(ours, out.join("s (onvolledig)"));
    std::fs::create_dir(&ours).unwrap();
    std::fs::write(ours.join(PARTIAL_MARK), b"").unwrap();
    std::fs::write(ours.join("a.txt"), b"ha").unwrap();
    // Someone else's folder with a work folder name of the next try: not ours, kept.
    let theirs = out.join(partial_name("s.zip", 2, crate::locale::lang()));
    std::fs::create_dir(&theirs).unwrap();
    std::fs::write(theirs.join("keep.txt"), b"keep").unwrap();

    let zip = write_zip(&dir, "s.zip", &build(&[e("a.txt", b"hello")], false));
    let done = extract(&zip, "s.zip", &out, &NO_STOP).unwrap();
    assert_eq!(done.folder, out);
    assert_eq!(
        names_in(&out),
        [
            "a.txt".into(),
            partial_name("s.zip", 2, crate::locale::lang())
        ]
    );
    assert_eq!(std::fs::read(theirs.join("keep.txt")).unwrap(), b"keep");

    // Only someone else's folder with the first name: the next name is used, theirs kept.
    std::fs::remove_dir_all(&theirs).unwrap();
    let other_name = partial_name("t.zip", 1, crate::locale::lang());
    let other = out.join(&other_name);
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("keep.txt"), b"keep").unwrap();
    let zip = write_zip(&dir, "t.zip", &build(&[e("t.txt", b"t")], false));
    extract(&zip, "t.zip", &out, &NO_STOP).unwrap();
    assert_eq!(names_in(&out), ["a.txt".into(), other_name, "t.txt".into()]);
    assert_eq!(std::fs::read(other.join("keep.txt")).unwrap(), b"keep");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn moving_into_place_never_replaces_a_name_that_appeared() {
    let dir = temp_dir("appeared");
    let out = dir.join("out");
    let partial = dir.join("p");
    std::fs::create_dir_all(partial.join("docs")).unwrap();
    std::fs::create_dir(&out).unwrap();
    std::fs::write(partial.join("docs/a.txt"), b"new").unwrap();
    std::fs::write(partial.join("b.txt"), b"new").unwrap();
    // "b.txt" appeared in the chosen folder after the plan: everything goes into "Z" instead.
    std::fs::write(out.join("b.txt"), b"theirs").unwrap();
    let top = vec!["docs".to_string(), "b.txt".to_string()];
    assert_eq!(move_each(&partial, &top, &out), None);
    // What was moved went back.
    assert_eq!(names_in(&out), ["b.txt"]);
    assert_eq!(std::fs::read(out.join("b.txt")).unwrap(), b"theirs");
    let done = move_into_place(&partial, &top, &out, "Z.zip").unwrap();
    assert_eq!(done.folder, out.join("Z"));
    assert_eq!(std::fs::read(out.join("Z/docs/a.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("b.txt")).unwrap(), b"theirs");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_zip_cannot_plant_a_work_folder_or_its_mark() {
    for bad in [
        ".copycraft-partial",
        "docs/.copycraft-partial",
        "docs/.COPYCRAFT-Partial/x",
        ".docs.partial/a.txt",
        ".Docs.PARTIAL",
    ] {
        assert_eq!(safe_parts(bad), None, "{bad:?}");
    }
    // Deeper down, or without a name before it, such names are only names.
    for fine in [
        "docs (onvolledig)/a.txt",
        "a/.docs.partial/x",
        "a/docs (incomplete)/x",
        " (incomplete)",
        "docs.partial",
    ] {
        assert!(safe_parts(fine).is_some(), "{fine:?}");
    }
}

#[test]
fn only_this_zips_own_work_folder_name_is_refused() {
    let named = |names: &[&str], zip: &str| {
        let entries: Vec<_> = names.iter().map(|name| e(name, b"x")).collect();
        listing(&build(&entries, false)).unwrap().facts_named(zip)
    };
    for (name, zip) in [
        ("docs (onvolledig)/a.txt", "docs.zip"),
        ("docs (incomplete)/a.txt", "docs.zip"),
        ("Docs (Incomplete)/a.txt", "docs.ZIP"),
        ("docs 2 (onvolledig)/a.txt", "docs.zip"),
        ("docs (onvolledig)", "docs"),
    ] {
        let facts = named(&[name], zip);
        assert_eq!(facts.blocker(), Some(ZipError::WorkFolderName), "{name:?}");
        assert!(
            facts
                .label()
                .contains(crate::locale::t("zip_meta_work_folder_name"))
        );
    }
    for (name, zip) in [
        ("Rapport (onvolledig)/a.txt", "docs.zip"),
        ("Rapport (incomplete).txt", "docs.zip"),
        ("docs (onvolledig)/a.txt", "docs2.zip"),
        ("a/docs (onvolledig)/x", "docs.zip"),
        ("docs/a.txt", "docs.zip"),
        ("docs 0 (incomplete)/a.txt", "docs.zip"),
    ] {
        assert_eq!(named(&[name], zip).blocker(), None, "{name:?}");
    }
    assert_ne!(
        ZipError::WorkFolderName.message(),
        ZipError::Unsafe.message()
    );
    // Extracting refuses it too, writing nothing.
    let dir = temp_dir("own-work-folder");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    let zip = write_zip(
        &dir,
        "docs.zip",
        &build(&[e("docs (incomplete)/a.txt", b"x")], false),
    );
    assert_eq!(
        extract(&zip, "docs.zip", &out, &NO_STOP),
        Err(ZipError::WorkFolderName)
    );
    assert!(names_in(&out).is_empty());
    let zip = write_zip(
        &dir,
        "other.zip",
        &build(&[e("Rapport (onvolledig)/a.txt", b"x")], false),
    );
    assert!(extract(&zip, "other.zip", &out, &NO_STOP).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_leftover_with_files_of_someone_else_keeps_them() {
    let dir = temp_dir("leftover-user");
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    // A crash halfway, then someone put a file of their own in the work folder.
    let ours = out.join(partial_name("u.zip", 1, crate::locale::lang()));
    std::fs::create_dir_all(ours.join("d")).unwrap();
    std::fs::write(ours.join(PARTIAL_MARK), b"").unwrap();
    std::fs::write(ours.join("d/a.txt"), b"ha").unwrap();
    std::fs::write(ours.join("d/mine.txt"), b"mine").unwrap();

    let zip = write_zip(&dir, "u.zip", &build(&[e("d/a.txt", b"hello")], false));
    let done = extract(&zip, "u.zip", &out, &NO_STOP).unwrap();
    assert_eq!(done.folder, out);
    assert_eq!(std::fs::read(out.join("d/a.txt")).unwrap(), b"hello");
    // Only the zip's own file went from the leftover; theirs and the folder stay.
    assert_eq!(names_in(&ours.join("d")), ["mine.txt"]);
    assert_eq!(std::fs::read(ours.join("d/mine.txt")).unwrap(), b"mine");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn removing_what_was_extracted_leaves_other_files() {
    let dir = temp_dir("remove-extracted");
    let work = dir.join("w");
    std::fs::create_dir_all(work.join("a/b")).unwrap();
    std::fs::write(work.join(PARTIAL_MARK), b"").unwrap();
    std::fs::write(work.join("a/b/c.txt"), b"x").unwrap();
    std::fs::write(work.join("top.txt"), b"x").unwrap();
    let zip = build(&[e("a/b/c.txt", b"x"), e("top.txt", b"x")], false);
    let listed = listing(&zip).unwrap();
    assert!(remove_extracted(&work, &listed));
    assert!(!work.exists());

    std::fs::create_dir_all(work.join("a/b")).unwrap();
    std::fs::write(work.join("a/b/c.txt"), b"x").unwrap();
    std::fs::write(work.join("a/other.txt"), b"keep").unwrap();
    assert!(!remove_extracted(&work, &listed));
    assert_eq!(names_in(&work), ["a"]);
    assert_eq!(names_in(&work.join("a")), ["other.txt"]);
    // Not ours without the mark: nothing is touched.
    std::fs::write(work.join("top.txt"), b"x").unwrap();
    assert!(!remove_leftover(&work, &listed));
    assert!(work.join("top.txt").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
