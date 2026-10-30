//! Zip archives dropped on the card: the names inside, and extracting them into a folder.
//!
//! Read by hand like [`crate::docx`] (no zip crate, no subprocess): the central directory is
//! read from the end of the file, entries are inflated with `flate2` (method 8) or copied
//! (method 0). Zip64 is read. Nothing is logged and nothing is cached: the listing lives on
//! the card only, and only the folder the user chose is written to.
//!
//! Safety:
//! - Every entry name must be a plain relative path: no leading `/`, no `..`, no drive
//!   (`C:`), no NUL or other control character, no bidirectional control (U+202E …). A
//!   backslash counts as a separator (old Windows zips). One unsafe entry makes the whole zip
//!   refuse to extract ([`ZipError::Unsafe`]); the names still show.
//! - A name cannot look like CopyCraft's own work folder (`docs (incomplete)`, `.docs.partial`
//!   at the top) or hold its mark file ([`safe_parts`]). A leftover work folder of a crash goes
//!   to the Trash whole ([`trash_leftover`]), never deleted; a failed or stopped extraction
//!   removes only the zip's own names from its own work folder ([`remove_extracted`]): a
//!   folder with someone's files is never removed.
//! - Symbolic links are refused the same way, and so are two entries with the same path (also
//!   when they differ only in case or Unicode form: the Mac's disk usually does not tell them
//!   apart, see [`name_key`]).
//! - Quarantine: every file CopyCraft makes gets `com.apple.quarantine` from the App Sandbox
//!   itself (a sandboxed app's files are always quarantined; no `LSFileQuarantineEnabled`
//!   needed), so an extracted app or script is checked by Gatekeeper on first open, like one
//!   Archive Utility extracts. The zip's own quarantine attribute is therefore not copied (that
//!   would need `getxattr`/`setxattr`, i.e. new unsafe). An unsandboxed `cargo run` build does
//!   not quarantine. Executable entries keep their `x` bit (0o755) for that reason only.
//! - `__MACOSX/` (Finder's resource forks) is left out of the listing and not extracted.
//! - Sizes are checked: an entry that unpacks to more or other bytes than its header says
//!   (CRC-32) fails the extraction.
//!
//! Never overwrites ([`plan_target`]): when none of the zip's top-level names exists in the
//! chosen folder the zip is extracted right into it; otherwise into a new folder named after
//! the zip ("Archive", "Archive 2", …). The files are first extracted into a work folder in
//! the chosen folder, named so it shows what it is ("Archive (onvolledig)" / "Archive
//! (incomplete)", never hidden: a crash cannot leave gigabytes nobody sees), and moved into
//! place at the end ([`move_into_place`]: each name is claimed with `create_dir` /
//! `create_new` before the rename, so nothing is replaced), so a quit or crash leaves no half
//! files under their real names. A failed or stopped extraction removes the work folder; Quit,
//! Clear History, Wipe and the screen lock stop a running extraction first. One left by a crash
//! goes to the Trash on the next try (only a folder with CopyCraft's mark inside; where there is
//! no Trash it stays and the next name is used). Before anything is
//! written, the disk must have room for the unpacked size ([`fits`]).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::Zeroizing;

/// Entries in a zip, at most.
pub const MAX_ENTRIES: usize = 100_000;
/// The central directory (the list of names), at most.
pub const MAX_DIRECTORY_BYTES: u64 = 64_000_000;
/// All entries together unpacked, at most (a zip bomb stops here, before anything is written).
pub const MAX_UNPACKED_BYTES: u64 = 16_000_000_000;
/// Lines of the listing on the card; the rest is "… and N more".
pub const SHOWN_LINES: usize = 1_000;
/// Folder names tried for the zip's own folder ("Archive", "Archive 2", … "Archive 999").
const MAX_FOLDER_TRIES: u32 = 999;

/// Why a zip cannot be listed or extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipError {
    /// No end of central directory: not a zip.
    NotZip,
    /// Damaged: the directory or an entry does not add up.
    Corrupt,
    /// Password protected (encrypted entries).
    Locked,
    /// A compression method this reader does not do (only stored and deflate).
    Unsupported,
    /// An entry would land outside the chosen folder, is a link, or is there twice.
    Unsafe,
    /// No entries (besides `__MACOSX/`).
    Empty,
    /// More entries, or more unpacked bytes, than the limits allow.
    TooLarge,
    /// The zip could not be read (gone, no access).
    Read,
    /// Writing to the chosen folder failed (no access, disk full, a name appeared meanwhile).
    Write,
    /// Not enough free space on the chosen folder's disk ([`fits`]); nothing was written.
    NoSpace,
    /// Stopped by the user (⌘. or the chip again); nothing is left behind.
    Cancelled,
    /// A top-level name is that of this zip's own work folder ([`Listing::plants_work_folder`]).
    WorkFolderName,
}

impl ZipError {
    /// The message for this error (NL/EN).
    pub fn message(self) -> &'static str {
        use crate::locale::t;
        match self {
            Self::NotZip | Self::Corrupt => t("zip_corrupt"),
            Self::Locked => t("zip_locked"),
            Self::Unsupported => t("zip_unsupported"),
            Self::Unsafe => t("zip_unsafe"),
            Self::Empty => t("zip_empty"),
            Self::TooLarge => t("zip_too_large"),
            Self::Read => t("file_unreadable"),
            Self::Write => t("zip_write_failed"),
            Self::NoSpace => t("zip_no_space"),
            Self::Cancelled => t("zip_cancelled"),
            Self::WorkFolderName => t("zip_work_folder_name"),
        }
    }
}

/// One entry of the central directory.
#[derive(Clone)]
pub struct Entry {
    /// The name as stored (decoded: UTF-8, else CP437), `/`-separated.
    pub name: String,
    /// Its path parts, when the name is safe ([`safe_parts`]).
    parts: Option<Vec<String>>,
    pub dir: bool,
    pub size: u64,
    packed: u64,
    method: u16,
    crc: u32,
    local: u64,
    /// Unix mode bits (a zip made on a Mac or Linux).
    mode: Option<u32>,
    encrypted: bool,
}

impl Entry {
    fn is_symlink(&self) -> bool {
        self.mode.is_some_and(|mode| mode & 0o170_000 == 0o120_000)
    }

    fn executable(&self) -> bool {
        self.mode.is_some_and(|mode| mode & 0o111 != 0)
    }
}

/// What the card knows about a zip: counts only (no names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZipFacts {
    pub files: usize,
    pub folders: usize,
    pub unpacked: u64,
    /// Password protected: the names show, extracting is refused.
    pub locked: bool,
    /// An entry is unsafe ([`ZipError::Unsafe`]): the names show, extracting is refused.
    pub unsafe_names: bool,
    /// A compression method this reader does not do.
    pub unsupported: bool,
    /// A top-level name is this zip's own work folder ([`ZipError::WorkFolderName`]).
    pub work_folder_name: bool,
}

impl ZipFacts {
    /// What stops extracting, if anything.
    pub fn blocker(&self) -> Option<ZipError> {
        if self.locked {
            Some(ZipError::Locked)
        } else if self.unsafe_names {
            Some(ZipError::Unsafe)
        } else if self.work_folder_name {
            Some(ZipError::WorkFolderName)
        } else if self.unsupported {
            Some(ZipError::Unsupported)
        } else {
            None
        }
    }

    /// The card's footer: "3 bestanden  ·  2 mappen  ·  1.20 MB uitgepakt", and what stops
    /// extracting, if anything.
    pub fn label(&self) -> String {
        use crate::locale::{t, tf};
        let mut parts = vec![if self.files == 1 {
            t("zip_one_file").to_string()
        } else {
            tf("zip_n_files", &[&self.files])
        }];
        if self.folders > 0 {
            parts.push(if self.folders == 1 {
                t("zip_one_folder").to_string()
            } else {
                tf("zip_n_folders", &[&self.folders])
            });
        }
        let size =
            crate::commands::format_bytes(usize::try_from(self.unpacked).unwrap_or(usize::MAX));
        parts.push(tf("zip_unpacked", &[&size]));
        match self.blocker() {
            Some(ZipError::Locked) => parts.push(t("zip_meta_locked").to_string()),
            Some(ZipError::Unsafe) => parts.push(t("zip_meta_unsafe").to_string()),
            Some(ZipError::WorkFolderName) => {
                parts.push(t("zip_meta_work_folder_name").to_string());
            }
            Some(_) => parts.push(t("zip_meta_unsupported").to_string()),
            None => {}
        }
        parts.join("  ·  ")
    }
}

/// The entries of a zip, `__MACOSX/` left out.
pub struct Listing {
    pub entries: Vec<Entry>,
}

impl Listing {
    pub fn facts(&self) -> ZipFacts {
        let mut facts = ZipFacts::default();
        let mut folders: HashSet<Vec<String>> = HashSet::new();
        let mut paths: HashSet<String> = HashSet::new();
        for entry in &self.entries {
            facts.locked |= entry.encrypted;
            facts.unsupported |= !entry.dir && !matches!(entry.method, 0 | 8);
            let Some(parts) = entry.parts.as_ref() else {
                facts.unsafe_names = true;
                continue;
            };
            if entry.is_symlink() {
                facts.unsafe_names = true;
            }
            // The same path twice, also in another case or Unicode form (é as one character
            // or as e + accent), would overwrite the first one: APFS treats them as one name.
            let key = name_key(&parts.join("/"));
            if !paths.insert(key) && !entry.dir {
                facts.unsafe_names = true;
            }
            if entry.dir {
                folders.insert(parts.clone());
            } else {
                facts.files += 1;
                facts.unpacked = facts.unpacked.saturating_add(entry.size);
            }
            // Folders implied by a path ("a/b/c.txt" has "a" and "a/b").
            for end in 1..parts.len() {
                folders.insert(parts[..end].to_vec());
            }
        }
        facts.folders = folders.len();
        facts
    }

    /// The listing as the card's text: every file and folder by its path, sorted, a folder
    /// with a trailing `/` and the number of entries right in it ("docs/  (3)"). At most
    /// [`SHOWN_LINES`] lines, then `more(n)`. Unsafe names show as stored, control and
    /// bidirectional characters written out ([`visible`]).
    pub fn text(&self, more: impl Fn(usize) -> String) -> Zeroizing<String> {
        // path → (is folder, entries right in it)
        let mut lines: std::collections::BTreeMap<String, (bool, usize)> = Default::default();
        let mut children: HashMap<String, HashSet<String>> = HashMap::new();
        for entry in &self.entries {
            let Some(parts) = entry.parts.as_ref().filter(|parts| !parts.is_empty()) else {
                lines.insert(visible(&entry.name), (entry.dir, 0));
                continue;
            };
            for end in 1..=parts.len() {
                let path = parts[..end].join("/");
                let folder = end < parts.len() || entry.dir;
                lines.entry(path.clone()).or_insert((folder, 0)).0 |= folder;
                if end > 1 {
                    children
                        .entry(parts[..end - 1].join("/"))
                        .or_default()
                        .insert(path);
                }
            }
        }
        for (path, (folder, count)) in lines.iter_mut() {
            if *folder {
                *count = children.get(path).map_or(0, HashSet::len);
            }
        }
        let total = lines.len();
        let mut out = String::new();
        for (path, (folder, count)) in lines.iter().take(SHOWN_LINES) {
            out.push_str(path);
            if *folder {
                out.push('/');
                out.push_str(&format!("  ({count})"));
            }
            out.push('\n');
        }
        if total > SHOWN_LINES {
            out.push_str(&more(total - SHOWN_LINES));
            out.push('\n');
        }
        let text = Zeroizing::new(out.trim_end().to_string());
        // The working copy goes too.
        drop(Zeroizing::new(out));
        text
    }

    /// The first part of every safe entry, without repeats, in order.
    /// [`Listing::facts`] for the zip called `zip_name`, with
    /// [`ZipFacts::work_folder_name`] filled in.
    pub fn facts_named(&self, zip_name: &str) -> ZipFacts {
        ZipFacts {
            work_folder_name: self.plants_work_folder(zip_name),
            ..self.facts()
        }
    }

    /// Whether a top-level name is one of the work folders extracting the zip called
    /// `zip_name` makes ([`partial_name`]: "docs (onvolledig)", "docs 2 (incomplete)", …, either
    /// language). Such a folder would pass for a leftover of ours; any other "… (onvolledig)"
    /// ("Rapport (onvolledig)" in docs.zip) is just a name. One comparison per name and
    /// language ([`is_work_folder_stem`]), however many names the zip has.
    pub fn plants_work_folder(&self, zip_name: &str) -> bool {
        use crate::locale::Lang;
        let suffixes: Vec<String> = [Lang::Nl, Lang::En]
            .into_iter()
            .map(|lang| name_key(&crate::locale::tf_in(lang, "zip_incomplete", &[&""])))
            .collect();
        let first = name_key(&folder_name(zip_name, 1));
        self.top_names().iter().any(|top| {
            let key = name_key(top);
            suffixes.iter().any(|suffix| {
                key.strip_suffix(suffix.as_str())
                    .is_some_and(|stem| is_work_folder_stem(stem, &first, zip_name))
            })
        })
    }

    pub fn top_names(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for parts in self.entries.iter().filter_map(|e| e.parts.as_ref()) {
            if let Some(first) = parts.first()
                && seen.insert(name_key(first))
            {
                names.push(first.clone());
            }
        }
        names
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([raw[0], raw[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let raw = bytes.get(at..at.checked_add(8)?)?;
    let mut le = [0u8; 8];
    le.copy_from_slice(raw);
    Some(u64::from_le_bytes(le))
}

/// Read `len` bytes at `at` into one zeroizing buffer of that size.
fn read_at<R: Read + Seek>(
    file: &mut R,
    at: u64,
    len: u64,
) -> Result<Zeroizing<Vec<u8>>, ZipError> {
    let len = usize::try_from(len).map_err(|_| ZipError::TooLarge)?;
    let mut buf = Zeroizing::new(vec![0u8; len]);
    file.seek(SeekFrom::Start(at)).map_err(|_| ZipError::Read)?;
    file.read_exact(&mut buf).map_err(|_| ZipError::Corrupt)?;
    Ok(buf)
}

/// Where the central directory is: (entries, start, size).
fn find_directory<R: Read + Seek>(file: &mut R, len: u64) -> Result<(u64, u64, u64), ZipError> {
    // End of central directory: the last 22 bytes, or before a comment of up to 64 KiB, with
    // room for the zip64 locator (20 bytes) in front of it.
    let tail_len = len.min(22 + 65_535 + 20);
    let tail_at = len - tail_len;
    let tail = read_at(file, tail_at, tail_len)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&at| {
            tail.get(at..at + 4) == Some(b"PK\x05\x06")
                && u16_at(&tail, at + 20)
                    .is_some_and(|comment| at + 22 + comment as usize <= tail.len())
        })
        .ok_or(ZipError::NotZip)?;
    let total = u16_at(&tail, eocd + 10).ok_or(ZipError::Corrupt)?;
    let size = u32_at(&tail, eocd + 12).ok_or(ZipError::Corrupt)?;
    let start = u32_at(&tail, eocd + 16).ok_or(ZipError::Corrupt)?;
    let eocd_at = tail_at + eocd as u64;
    if total != 0xFFFF && size != 0xFFFF_FFFF && start != 0xFFFF_FFFF {
        if u64::from(start) + u64::from(size) > eocd_at {
            return Err(ZipError::Corrupt);
        }
        return Ok((u64::from(total), u64::from(start), u64::from(size)));
    }
    // Zip64: the locator sits right before the end record and points at the zip64 end record.
    let locator = eocd.checked_sub(20).ok_or(ZipError::Corrupt)?;
    if tail.get(locator..locator + 4) != Some(b"PK\x06\x07") {
        return Err(ZipError::Corrupt);
    }
    let record_at = u64_at(&tail, locator + 8).ok_or(ZipError::Corrupt)?;
    if record_at.checked_add(56).is_none_or(|end| end > eocd_at) {
        return Err(ZipError::Corrupt);
    }
    let record = read_at(file, record_at, 56)?;
    if record.get(..4) != Some(b"PK\x06\x06") {
        return Err(ZipError::Corrupt);
    }
    let total = u64_at(&record, 32).ok_or(ZipError::Corrupt)?;
    let size = u64_at(&record, 40).ok_or(ZipError::Corrupt)?;
    let start = u64_at(&record, 48).ok_or(ZipError::Corrupt)?;
    if start.checked_add(size).is_none_or(|end| end > record_at) {
        return Err(ZipError::Corrupt);
    }
    Ok((total, start, size))
}

/// The entries of the zip read from `file` (`len` bytes long).
pub fn list<R: Read + Seek>(file: &mut R, len: u64) -> Result<Listing, ZipError> {
    let (total, start, size) = find_directory(file, len)?;
    if total > MAX_ENTRIES as u64 {
        return Err(ZipError::TooLarge);
    }
    if size > MAX_DIRECTORY_BYTES {
        return Err(ZipError::TooLarge);
    }
    let dir = read_at(file, start, size)?;
    let mut entries = Vec::with_capacity(total as usize);
    let mut at = 0usize;
    let mut unpacked: u64 = 0;
    for _ in 0..total {
        if dir.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(ZipError::Corrupt);
        }
        let field = |offset: usize| u16_at(&dir, at + offset).ok_or(ZipError::Corrupt);
        let field32 = |offset: usize| u32_at(&dir, at + offset).ok_or(ZipError::Corrupt);
        let made_by = field(4)?;
        let flags = field(8)?;
        let method = field(10)?;
        let crc = field32(16)?;
        let mut packed = u64::from(field32(20)?);
        let mut size = u64::from(field32(24)?);
        let name_len = field(28)? as usize;
        let extra_len = field(30)? as usize;
        let comment_len = field(32)? as usize;
        let external = field32(38)?;
        let mut local = u64::from(field32(42)?);
        let name_at = at + 46;
        let raw_name = dir
            .get(name_at..name_at + name_len)
            .ok_or(ZipError::Corrupt)?;
        let extra = dir
            .get(name_at + name_len..name_at + name_len + extra_len)
            .ok_or(ZipError::Corrupt)?;
        // Zip64 extra field: the 64-bit values of the fields that say 0xFFFFFFFF, in order.
        if size == 0xFFFF_FFFF || packed == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
            let mut values = zip64_extra(extra).ok_or(ZipError::Corrupt)?.into_iter();
            for value in [&mut size, &mut packed, &mut local] {
                if *value == 0xFFFF_FFFF {
                    *value = values.next().ok_or(ZipError::Corrupt)?;
                }
            }
        }
        let name = decode_name(raw_name, flags & 0x800 != 0);
        let host = made_by >> 8;
        let mode = (host == 3 || host == 19)
            .then_some(external >> 16)
            .filter(|m| *m != 0);
        let dir_entry = name.ends_with('/')
            || name.ends_with('\\')
            || (host == 0 && external & 0x10 != 0)
            || mode.is_some_and(|m| m & 0o170_000 == 0o040_000);
        let parts = safe_parts(&name);
        at = name_at + name_len + extra_len + comment_len;
        if parts
            .as_ref()
            .and_then(|parts| parts.first())
            .is_some_and(|first| first == "__MACOSX")
        {
            continue;
        }
        // "./" and the like: nothing to make.
        if parts.as_ref().is_some_and(Vec::is_empty) {
            continue;
        }
        if !dir_entry {
            unpacked = unpacked.saturating_add(size);
        }
        entries.push(Entry {
            name,
            parts,
            dir: dir_entry,
            size: if dir_entry { 0 } else { size },
            packed,
            method,
            crc,
            local,
            mode,
            encrypted: flags & 1 != 0 || flags & 0x40 != 0 || method == 99,
        });
    }
    if unpacked > MAX_UNPACKED_BYTES {
        return Err(ZipError::TooLarge);
    }
    if entries.is_empty() {
        return Err(ZipError::Empty);
    }
    Ok(Listing { entries })
}

/// The values of the zip64 extended information field (id 1) in `extra`.
fn zip64_extra(extra: &[u8]) -> Option<Vec<u64>> {
    let mut at = 0;
    while at + 4 <= extra.len() {
        let id = u16_at(extra, at)?;
        let len = u16_at(extra, at + 2)? as usize;
        let data = extra.get(at + 4..at + 4 + len)?;
        if id == 1 {
            return Some(
                data.as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| u64::from_le_bytes(*c))
                    .collect(),
            );
        }
        at += 4 + len;
    }
    None
}

/// CP437, the old DOS code page, for bytes 0x80–0xFF: what a zip without the UTF-8 flag
/// means, unless its name happens to be valid UTF-8 (Finder and most tools write UTF-8
/// without setting the flag).
const CP437_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

fn decode_name(raw: &[u8], utf8: bool) -> String {
    match std::str::from_utf8(raw) {
        Ok(name) => name.to_string(),
        Err(_) if utf8 => String::from_utf8_lossy(raw).into_owned(),
        Err(_) => raw
            .iter()
            .map(|&b| {
                if b < 0x80 {
                    b as char
                } else {
                    CP437_HIGH.chars().nth(usize::from(b - 0x80)).unwrap_or('?')
                }
            })
            .collect(),
    }
}

/// A character that can make a name look like another one, or break the list on the card:
/// a control character (Cc: C0 such as a newline, DEL, C1), an invisible format character
/// (Cf: bidirectional controls such as U+202E, `factuur\u{202E}fdp.app` shows as
/// `factuurppa.pdf`; zero-width spaces and joiners, U+FEFF, the tag characters), or a line or
/// paragraph separator (Zl, Zp: U+2028, U+2029), or a letter that shows as nothing
/// ([`BLANK_LETTERS`]). In a name use [`deceptive_at`], which lets the joiner through between
/// two emoji.
pub fn is_deceptive(c: char) -> bool {
    let code = u32::from(c);
    c.is_control()
        || BLANK_LETTERS.contains(&c)
        || FORMAT_AND_SEPARATORS
            .iter()
            .any(|&(first, last)| (first..=last).contains(&code))
}

/// The Hangul fillers: letters (Lo) by their category, but drawn as blank space, so
/// `a\u{3164}.txt` passes for `a .txt` or a name looks empty. Hangul text never needs them in
/// a file name (a filler only pads an incomplete syllable).
const BLANK_LETTERS: [char; 4] = ['\u{115F}', '\u{1160}', '\u{3164}', '\u{FFA0}'];

/// The Unicode general categories Cf, Zl and Zp, as ranges (from Unicode 15.1
/// `extracted/DerivedGeneralCategory.txt`; the whole tag block U+E0000–E007F, unassigned
/// code points in it included). No crate in the tree offers the categories.
const FORMAT_AND_SEPARATORS: [(u32, u32); 21] = [
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x2028, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Joins two emoji into one (👨‍💻); a format character ([`is_deceptive`]) anywhere else.
const ZWJ: char = '\u{200D}';
/// Asks for the emoji look of the character before it (❤️).
const EMOJI_PRESENTATION: char = '\u{FE0F}';

/// Draws a key around the character before it: `1\u{FE0F}\u{20E3}` is 1️⃣.
const KEYCAP: char = '\u{20E3}';

/// The characters a keycap emoji is made of: the digits, `#` and `*` (Unicode
/// `emoji/emoji-sequences.txt`, `Emoji_Keycap_Sequence`).
fn is_keycap_base(c: char) -> bool {
    c.is_ascii_digit() || c == '#' || c == '*'
}

/// A skin tone (🏻 to 🏿, Emoji_Modifier), colouring the emoji before it (👍🏽).
fn is_skin_tone(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

/// An emoji as Unicode defines it for joining: Extended_Pictographic ([`EXTENDED_PICTOGRAPHIC`]).
fn is_emoji(c: char) -> bool {
    let code = u32::from(c);
    EXTENDED_PICTOGRAPHIC
        .iter()
        .any(|&(first, last)| (first..=last).contains(&code))
}

/// Whether the character at `index` of `chars` makes the name deceptive: [`is_deceptive`],
/// except for what emoji are made of. U+FE0F and a skin tone pass only straight after an emoji,
/// and U+FE0F also in a keycap (`[0-9#*]`, U+FE0F, U+20E3: 1️⃣, #️⃣; the form without U+FE0F has
/// nothing to check); U+200D (zero-width joiner) only between two emoji, the first with at most one U+FE0F or skin
/// tone of its own (👨‍💻, 👩🏽‍💻, 🏳️‍🌈). A joiner after or before a letter, a `/`, the start
/// or the end of the name, or another joiner stays deceptive (`a\u{200D}b` reads as `ab`).
pub fn deceptive_at(chars: &[char], index: usize) -> bool {
    let emoji_at = |i: Option<usize>| i.and_then(|i| chars.get(i)).is_some_and(|&c| is_emoji(c));
    let before = index.checked_sub(1);
    match chars[index] {
        ZWJ => {
            let emoji_ends_before = emoji_at(before)
                || (before
                    .and_then(|i| chars.get(i))
                    .is_some_and(|&c| c == EMOJI_PRESENTATION || is_skin_tone(c))
                    && emoji_at(index.checked_sub(2)));
            !(emoji_ends_before && emoji_at(Some(index + 1)))
        }
        EMOJI_PRESENTATION => {
            let keycap = before
                .and_then(|i| chars.get(i))
                .is_some_and(|&c| is_keycap_base(c))
                && chars.get(index + 1) == Some(&KEYCAP);
            !(keycap || emoji_at(before))
        }
        c if is_skin_tone(c) => !emoji_at(before),
        c => is_deceptive(c),
    }
}

/// Whether any character of `name` is deceptive ([`deceptive_at`]).
pub fn has_deceptive(name: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    (0..chars.len()).any(|index| deceptive_at(&chars, index))
}

/// Extended_Pictographic as ranges (from Unicode 15.1 `emoji/emoji-data.txt`, the same version
/// as [`FORMAT_AND_SEPARATORS`]; adjacent ranges merged, the reserved code points the file
/// lists included). It holds no skin tone, no U+FE0F and no letter or digit.
const EXTENDED_PICTOGRAPHIC: [(u32, u32); 78] = [
    (0x00A9, 0x00A9),
    (0x00AE, 0x00AE),
    (0x203C, 0x203C),
    (0x2049, 0x2049),
    (0x2122, 0x2122),
    (0x2139, 0x2139),
    (0x2194, 0x2199),
    (0x21A9, 0x21AA),
    (0x231A, 0x231B),
    (0x2328, 0x2328),
    (0x2388, 0x2388),
    (0x23CF, 0x23CF),
    (0x23E9, 0x23F3),
    (0x23F8, 0x23FA),
    (0x24C2, 0x24C2),
    (0x25AA, 0x25AB),
    (0x25B6, 0x25B6),
    (0x25C0, 0x25C0),
    (0x25FB, 0x25FE),
    (0x2600, 0x2605),
    (0x2607, 0x2612),
    (0x2614, 0x2685),
    (0x2690, 0x2705),
    (0x2708, 0x2712),
    (0x2714, 0x2714),
    (0x2716, 0x2716),
    (0x271D, 0x271D),
    (0x2721, 0x2721),
    (0x2728, 0x2728),
    (0x2733, 0x2734),
    (0x2744, 0x2744),
    (0x2747, 0x2747),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2763, 0x2767),
    (0x2795, 0x2797),
    (0x27A1, 0x27A1),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2934, 0x2935),
    (0x2B05, 0x2B07),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x3030, 0x3030),
    (0x303D, 0x303D),
    (0x3297, 0x3297),
    (0x3299, 0x3299),
    (0x1F000, 0x1F0FF),
    (0x1F10D, 0x1F10F),
    (0x1F12F, 0x1F12F),
    (0x1F16C, 0x1F171),
    (0x1F17E, 0x1F17F),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1AD, 0x1F1E5),
    (0x1F201, 0x1F20F),
    (0x1F21A, 0x1F21A),
    (0x1F22F, 0x1F22F),
    (0x1F232, 0x1F23A),
    (0x1F23C, 0x1F23F),
    (0x1F249, 0x1F3FA),
    (0x1F400, 0x1F53D),
    (0x1F546, 0x1F64F),
    (0x1F680, 0x1F6FF),
    (0x1F774, 0x1F77F),
    (0x1F7D5, 0x1F7FF),
    (0x1F80C, 0x1F80F),
    (0x1F848, 0x1F84F),
    (0x1F85A, 0x1F85F),
    (0x1F888, 0x1F88F),
    (0x1F8AE, 0x1F8FF),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1FAFF),
    (0x1FC00, 0x1FFFD),
];

/// `name` with every deceptive character ([`deceptive_at`]) written out (`\u{202E}`), so the list shows
/// what is really there.
pub fn visible(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let chars: Vec<char> = name.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if deceptive_at(&chars, index) {
            out.push_str(&format!("\\u{{{:04X}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// The key under which the Mac's file system sees `name` as the same name: Unicode NFC, then
/// lower case (APFS and HFS+ ignore case and normalisation by default).
pub fn name_key(name: &str) -> String {
    if name.is_ascii() {
        return name.to_ascii_lowercase();
    }
    nfc(name).to_lowercase()
}

/// `text` in Unicode NFC, through Foundation (no extra crate). Off the Mac (Linux tests) the
/// text as is: only the case is folded there.
#[cfg(target_os = "macos")]
fn nfc(text: &str) -> String {
    use mac_ui::objc2_foundation::NSString;
    mac_ui::objc2::rc::autoreleasepool(|_| {
        NSString::from_str(text)
            .precomposedStringWithCanonicalMapping()
            .to_string()
    })
}

#[cfg(not(target_os = "macos"))]
fn nfc(text: &str) -> String {
    text.to_string()
}

/// The path parts of an entry name, when it stays inside the folder it is extracted to:
/// relative (no leading `/` or `\`), no `..`, no drive (`C:`), no control or bidirectional
/// characters ([`deceptive_at`], NUL included), each part at most 255 bytes, no part named
/// like CopyCraft's work-folder mark ([`PARTIAL_MARK`]) and no top-level `.….partial` (the
/// work folders of earlier versions): a zip cannot plant what makes a folder look like a
/// leftover of ours. A top-level name of this zip's own work folder depends on the zip's name:
/// [`Listing::plants_work_folder`]. Empty and `.` parts are skipped (`a//b`, `./a`). `None` for an unsafe name.
pub fn safe_parts(name: &str) -> Option<Vec<String>> {
    if name.is_empty() || has_deceptive(name) {
        return None;
    }
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for (index, part) in name.split('/').enumerate() {
        match part {
            "" | "." => continue,
            ".." => return None,
            _ => {}
        }
        // A drive ("C:", "c:evil") as first part.
        if index == 0
            && part.as_bytes().get(1) == Some(&b':')
            && part.as_bytes()[0].is_ascii_alphabetic()
        {
            return None;
        }
        if part.len() > 255 || part.eq_ignore_ascii_case(PARTIAL_MARK) {
            return None;
        }
        if parts.is_empty() && looks_like_old_work_folder(part) {
            return None;
        }
        parts.push(part.to_string());
    }
    Some(parts)
}

/// The hidden work folder of earlier versions: `.docs.partial`.
fn looks_like_old_work_folder(name: &str) -> bool {
    let lower = name_key(name);
    lower.starts_with('.') && lower.ends_with(".partial")
}

/// Where to extract: `Direct` (right into the chosen folder) when none of `top_names`
/// exists there (`exists`), else `Folder` (a new folder named after the zip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Direct,
    Folder,
}

pub fn plan_target(top_names: &[String], exists: impl Fn(&str) -> bool) -> Target {
    if top_names.iter().any(|name| exists(name)) {
        Target::Folder
    } else {
        Target::Direct
    }
}

/// The zip's own folder name: its file name without `.zip` ("Archive"), then "Archive 2", ….
pub fn folder_name(zip_name: &str, try_number: u32) -> String {
    let trimmed = zip_name.trim();
    let stem = match trimmed.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("zip") && !stem.is_empty() => stem,
        _ => trimmed,
    };
    // Never a path: the name of a file has no '/', but be sure.
    let stem: String = stem
        .chars()
        .map(|c| if c == '/' || c == '\0' { '_' } else { c })
        .collect();
    let stem = match stem.trim() {
        "" | "." | ".." => "Archive".to_string(),
        stem => stem.to_string(),
    };
    if try_number <= 1 {
        stem
    } else {
        format!("{stem} {try_number}")
    }
}

/// What was made inside the `.partial` folder: the folders, by [`name_key`], so a folder of
/// the zip is made once and a folder that was not ours is never written into.
#[derive(Default)]
struct Made {
    dir_keys: HashSet<String>,
}

/// Where the zip went: the folder it was extracted into (the chosen one or the zip's own),
/// and whether that is a new folder named after the zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub folder: PathBuf,
    pub own_folder: bool,
}

/// Room kept free on the disk besides the unpacked bytes: 64 MB, plus a 4 KB block per file.
const SPACE_MARGIN: u64 = 64_000_000;
const BLOCK: u64 = 4_096;

/// Whether `free` bytes hold the zip's files, with [`SPACE_MARGIN`]. `None` (unknown) fits.
pub fn fits(facts: &ZipFacts, free: Option<u64>) -> bool {
    let needed = facts
        .unpacked
        .saturating_add((facts.files as u64).saturating_mul(BLOCK))
        .saturating_add(SPACE_MARGIN);
    free.is_none_or(|free| free >= needed)
}

/// The bytes free for `folder`'s disk ("important usage": what macOS can make free too),
/// through Foundation; `None` when unknown. Off the Mac: unknown.
#[cfg(target_os = "macos")]
pub fn free_space(folder: &Path) -> Option<u64> {
    use mac_ui::objc2_foundation::{NSArray, NSNumber, NSString, NSURL};
    let folder = folder.to_str()?;
    mac_ui::objc2::rc::autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(folder));
        // The value of NSURLVolumeAvailableCapacityForImportantUsageKey is its own name; the
        // string avoids reading the extern static (unsafe).
        let key = NSString::from_str("NSURLVolumeAvailableCapacityForImportantUsageKey");
        let values = url
            .resourceValuesForKeys_error(&NSArray::from_slice(&[&*key]))
            .ok()?;
        let value = values.objectForKey(&key)?;
        let number = value.downcast_ref::<NSNumber>()?;
        u64::try_from(number.longLongValue()).ok()
    })
}

#[cfg(not(target_os = "macos"))]
pub fn free_space(_folder: &Path) -> Option<u64> {
    None
}

/// Inside every work folder CopyCraft makes, so only such a folder is ever removed as a
/// leftover (a crash during an earlier extraction).
const PARTIAL_MARK: &str = ".copycraft-partial";

/// The work folder the zip is first extracted into, visible in `dest`: "docs (onvolledig)" /
/// "docs (incomplete)" in `lang`.
fn partial_name(zip_name: &str, try_number: u32, lang: crate::locale::Lang) -> String {
    crate::locale::tf_in(
        lang,
        "zip_incomplete",
        &[&folder_name(zip_name, try_number)],
    )
}

/// Where a leftover work folder of an earlier try goes: to the Trash, whole, never deleted (a
/// file someone put in it can be got back), and whether one went.
pub struct Leftovers<'a> {
    /// Moves a folder to the Trash; `true` when it went. [`system_trash`] in the app.
    trash: &'a dyn Fn(&Path) -> bool,
    /// A leftover went to the Trash during this extraction (the toast says so).
    pub trashed: bool,
}

impl Default for Leftovers<'_> {
    fn default() -> Self {
        Self {
            trash: &system_trash,
            trashed: false,
        }
    }
}

/// The Finder's Trash (`NSFileManager trashItemAtURL`, through [`mac_ui::trash`]). Elsewhere
/// (the tests on Linux) there is none: `false`, and a leftover stays where it is.
fn system_trash(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        mac_ui::trash::move_to_trash(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Move a leftover work folder of this zip to the Trash, whole, but only one CopyCraft made (it
/// holds [`PARTIAL_MARK`]). `true` when it went; otherwise it stays as it is (someone else's
/// folder, or a volume without a Trash) and the next work folder name is used.
fn trash_leftover(path: &Path, leftovers: &mut Leftovers) -> bool {
    let ours = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
        && std::fs::symlink_metadata(path.join(PARTIAL_MARK)).is_ok_and(|meta| meta.is_file());
    let gone = ours && (leftovers.trash)(path);
    leftovers.trashed |= gone;
    gone
}

/// Empty the work folder `path` of what `listing` extracts into it, then remove it when only
/// the mark is left. Only the zip's own names go (files, then folders that are empty then);
/// anything else (a file someone put there) stays, and so does the folder then. Links are
/// never followed: before each removal every folder from `path` down to the name is checked
/// to be a real folder ([`real_dirs_to`]), so a folder swapped for a link meanwhile never
/// leads a removal outside the work folder.
fn remove_extracted(path: &Path, listing: &Listing) -> bool {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in &listing.entries {
        let Some(parts) = entry.parts.as_deref() else {
            continue;
        };
        let mut at = path.to_path_buf();
        for (index, part) in parts.iter().enumerate() {
            at.push(part);
            if index + 1 < parts.len() || entry.dir {
                dirs.push(at.clone());
            }
        }
        if !entry.dir
            && real_dirs_to(path, &at)
            && std::fs::symlink_metadata(&at).is_ok_and(|meta| meta.is_file())
        {
            let _ = std::fs::remove_file(&at);
        }
    }
    // Deepest first; remove_dir takes only an empty folder.
    dirs.sort_by(|a, b| {
        b.components()
            .count()
            .cmp(&a.components().count())
            .then_with(|| a.cmp(b))
    });
    dirs.dedup();
    for dir in &dirs {
        if real_dirs_to(path, dir) {
            let _ = std::fs::remove_dir(dir);
        }
    }
    let only_mark = std::fs::read_dir(path).is_ok_and(|mut names| {
        names.all(|name| name.is_ok_and(|name| name.file_name() == PARTIAL_MARK))
    });
    if !only_mark {
        return false;
    }
    let mark = path.join(PARTIAL_MARK);
    if real_dirs_to(path, &mark) && std::fs::symlink_metadata(&mark).is_ok() {
        let _ = std::fs::remove_file(&mark);
    }
    std::fs::remove_dir(path).is_ok()
}

/// Every folder from the work folder `root` down to `at`'s parent (`root` included) is a real
/// folder, not a link (`symlink_metadata`, so nothing is followed). `false` when one is not, or
/// `at` is not inside `root`.
fn real_dirs_to(root: &Path, at: &Path) -> bool {
    let Ok(rest) = at.strip_prefix(root) else {
        return false;
    };
    let real = |dir: &Path| std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir());
    let mut dir = root.to_path_buf();
    if !real(&dir) {
        return false;
    }
    let mut parts: Vec<_> = rest.components().collect();
    parts.pop();
    parts.into_iter().all(|part| {
        dir.push(part);
        real(&dir)
    })
}

/// Whether `stem` (by [`name_key`]) is [`folder_name`]`(zip_name, n)` for some try `n` in
/// `1..=MAX_FOLDER_TRIES`, `first` being the key for try 1. A later try only adds " n" (ASCII,
/// no leading zero), so that number is read from the end and compared once.
fn is_work_folder_stem(stem: &str, first: &str, zip_name: &str) -> bool {
    if stem == first {
        return true;
    }
    let Some((_, digits)) = stem.rsplit_once(' ') else {
        return false;
    };
    let canonical = !digits.is_empty()
        && digits.len() <= 3
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !digits.starts_with('0');
    match digits.parse::<u32>() {
        Ok(number) if canonical && (2..=MAX_FOLDER_TRIES).contains(&number) => {
            name_key(&folder_name(zip_name, number)) == stem
        }
        _ => false,
    }
}

/// Make a new work folder in `dest`, named in the language of the moment and marked with
/// [`PARTIAL_MARK`]. A leftover of ours with the same number goes to the Trash first, in either
/// language (the language may have changed since the crash); someone else's, or one that
/// cannot go to the Trash, is left and the next number tried.
fn make_partial(
    dest: &Path,
    zip_name: &str,
    leftovers: &mut Leftovers,
) -> Result<PathBuf, ZipError> {
    use crate::locale::Lang;
    for number in 1..=MAX_FOLDER_TRIES {
        for lang in [Lang::Nl, Lang::En] {
            let old = dest.join(partial_name(zip_name, number, lang));
            if std::fs::symlink_metadata(&old).is_ok() {
                trash_leftover(&old, leftovers);
            }
        }
        let path = dest.join(partial_name(zip_name, number, crate::locale::lang()));
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ZipError::Write),
        }
        let mark = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join(PARTIAL_MARK));
        if mark.is_err() {
            let _ = std::fs::remove_dir(&path);
            return Err(ZipError::Write);
        }
        return Ok(path);
    }
    Err(ZipError::Write)
}

/// Extract the zip at `zip` (called `zip_name`) into the folder `dest`, by the rules above.
/// `stop` cancels it (checked between blocks of 64 KB); nothing is left behind then. A
/// leftover work folder of an earlier try goes to the Trash ([`Leftovers::trashed`] says so).
pub fn extract(
    zip: &Path,
    zip_name: &str,
    dest: &Path,
    stop: &AtomicBool,
    leftovers: &mut Leftovers,
) -> Result<Extracted, ZipError> {
    extract_with(zip, zip_name, dest, stop, free_space(dest), leftovers)
}

fn extract_with(
    zip: &Path,
    zip_name: &str,
    dest: &Path,
    stop: &AtomicBool,
    free: Option<u64>,
    leftovers: &mut Leftovers,
) -> Result<Extracted, ZipError> {
    let mut file = std::fs::File::open(zip).map_err(|_| ZipError::Read)?;
    let len = file.metadata().map_err(|_| ZipError::Read)?.len();
    let listing = list(&mut file, len)?;
    let facts = listing.facts_named(zip_name);
    if let Some(blocker) = facts.blocker() {
        return Err(blocker);
    }
    if !std::fs::metadata(dest).is_ok_and(|meta| meta.is_dir()) {
        return Err(ZipError::Write);
    }
    if !fits(&facts, free) {
        return Err(ZipError::NoSpace);
    }
    let partial = make_partial(dest, zip_name, leftovers)?;
    let mut made = Made::default();
    let mut reader = std::io::BufReader::new(file);
    // A panic (a bug) here is an error like any other, so the work folder is still cleaned up.
    let result = caught(|| {
        listing.entries.iter().try_for_each(|entry| {
            if stop.load(Ordering::Relaxed) {
                return Err(ZipError::Cancelled);
            }
            extract_entry(&mut reader, entry, &partial, &mut made, stop)
        })?;
        std::fs::remove_file(partial.join(PARTIAL_MARK)).map_err(|_| ZipError::Write)?;
        if stop.load(Ordering::Relaxed) {
            return Err(ZipError::Cancelled);
        }
        move_into_place(&partial, &listing.top_names(), dest, zip_name)
    });
    if result.is_err() {
        // Ours only: the work folder was made above (with its mark) and nothing outside it has
        // been touched. Only the zip's names in it go.
        remove_extracted(&partial, &listing);
    }
    result
}

/// Run `work` with any panic caught: a bug becomes [`ZipError::Write`] (the toast says it
/// failed) instead of an extraction that never ends. The panic's payload is dropped unread:
/// it is never logged or shown.
pub(crate) fn caught<T>(work: impl FnOnce() -> Result<T, ZipError>) -> Result<T, ZipError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_payload| Err(ZipError::Write))
}

/// Move what was extracted into `partial` to where it goes, never over anything: straight
/// into `dest` when none of the zip's top-level names is there, otherwise the `partial`
/// folder becomes a new folder named after the zip. Each name is claimed first with
/// `create_dir` / `create_new` (which fail when it exists) and only then renamed onto, so
/// a name that appears meanwhile is never replaced.
fn move_into_place(
    partial: &Path,
    top: &[String],
    dest: &Path,
    zip_name: &str,
) -> Result<Extracted, ZipError> {
    let direct = matches!(
        plan_target(top, |name| std::fs::symlink_metadata(dest.join(name))
            .is_ok()),
        Target::Direct
    );
    if direct && let Some(()) = move_each(partial, top, dest) {
        let _ = std::fs::remove_dir(partial);
        return Ok(Extracted {
            folder: dest.to_path_buf(),
            own_folder: false,
        });
    }
    for number in 1..=MAX_FOLDER_TRIES {
        let path = dest.join(folder_name(zip_name, number));
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ZipError::Write),
        }
        // Onto the empty folder just made: rename replaces an empty folder only, so if it
        // was filled meanwhile this fails instead of overwriting.
        return match std::fs::rename(partial, &path) {
            Ok(()) => Ok(Extracted {
                folder: path,
                own_folder: true,
            }),
            Err(_) => {
                let _ = std::fs::remove_dir(&path);
                Err(ZipError::Write)
            }
        };
    }
    Err(ZipError::Write)
}

/// Move each top-level name from `partial` into `dest`. `None` (and everything moved back)
/// when a name is taken.
fn move_each(partial: &Path, top: &[String], dest: &Path) -> Option<()> {
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for name in top {
        let from = partial.join(name);
        let to = dest.join(name);
        let is_dir = std::fs::symlink_metadata(&from).is_ok_and(|meta| meta.is_dir());
        let claimed = if is_dir {
            std::fs::create_dir(&to).is_ok()
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&to)
                .is_ok()
        };
        let ok = claimed && std::fs::rename(&from, &to).is_ok();
        if !ok {
            if claimed {
                let _ = if is_dir {
                    std::fs::remove_dir(&to)
                } else {
                    std::fs::remove_file(&to)
                };
            }
            for (from, to) in moved.into_iter().rev() {
                let _ = std::fs::rename(&to, &from);
            }
            return None;
        }
        moved.push((from, to));
    }
    Some(())
}

/// Make the folder `parts` inside `root` (and the ones above it), unless this extraction made
/// it already.
fn make_dirs(root: &Path, parts: &[String], made: &mut Made) -> Result<PathBuf, ZipError> {
    let mut path = root.to_path_buf();
    for end in 1..=parts.len() {
        path.push(&parts[end - 1]);
        let key = name_key(&parts[..end].join("/"));
        if made.dir_keys.contains(&key) {
            continue;
        }
        match std::fs::create_dir(&path) {
            Ok(()) => {
                made.dir_keys.insert(key);
            }
            // Not ours: never write into a folder that was there (or appeared) before.
            Err(_) => return Err(ZipError::Write),
        }
    }
    Ok(path)
}

fn extract_entry<R: Read + Seek>(
    zip: &mut R,
    entry: &Entry,
    root: &Path,
    made: &mut Made,
    stop: &AtomicBool,
) -> Result<(), ZipError> {
    let parts = entry.parts.as_deref().ok_or(ZipError::Unsafe)?;
    if entry.dir {
        make_dirs(root, parts, made)?;
        return Ok(());
    }
    let (file_name, folders) = parts.split_last().ok_or(ZipError::Unsafe)?;
    let mut path = make_dirs(root, folders, made)?;
    path.push(file_name);
    // The data: after the local header, whose name and extra lengths may differ from the
    // directory's.
    zip.seek(SeekFrom::Start(entry.local))
        .map_err(|_| ZipError::Corrupt)?;
    let mut header = [0u8; 30];
    zip.read_exact(&mut header).map_err(|_| ZipError::Corrupt)?;
    if &header[..4] != b"PK\x03\x04" {
        return Err(ZipError::Corrupt);
    }
    let skip =
        u64::from(u16_at(&header, 26).unwrap_or(0)) + u64::from(u16_at(&header, 28).unwrap_or(0));
    zip.seek(SeekFrom::Current(skip as i64))
        .map_err(|_| ZipError::Corrupt)?;
    let data = zip.by_ref().take(entry.packed);
    let mut source: Box<dyn Read + '_> = match entry.method {
        0 => Box::new(data),
        8 => Box::new(flate2::read::DeflateDecoder::new(data)),
        _ => return Err(ZipError::Unsupported),
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // 0o755 for an entry marked executable: safe because the sandbox quarantines the file
    // (module docs), so Gatekeeper still checks it before it runs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if entry.executable() { 0o755 } else { 0o644 });
    }
    let mut out = options.open(&path).map_err(|_| ZipError::Write)?;
    let mut buf = Zeroizing::new(vec![0u8; 64 * 1024]);
    let mut crc = flate2::Crc::new();
    let mut written: u64 = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(ZipError::Cancelled);
        }
        let n = source.read(&mut buf).map_err(|_| ZipError::Corrupt)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        // More than the header says: a bomb or a damaged entry. Stop before writing it.
        if written > entry.size {
            return Err(ZipError::Corrupt);
        }
        crc.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|_| ZipError::Write)?;
    }
    if written != entry.size || crc.sum() != entry.crc {
        return Err(ZipError::Corrupt);
    }
    out.flush().map_err(|_| ZipError::Write)?;
    Ok(())
}

/// Whether the file at `path` is a zip by its name (`.zip`) and its first bytes (a local
/// header, or the end record of an empty zip).
pub fn is_zip_file(path: &Path) -> bool {
    let named = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
    if !named {
        return false;
    }
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok_and(|()| &magic == b"PK\x03\x04" || &magic == b"PK\x05\x06")
}

#[cfg(test)]
#[path = "zip_archive_tests.rs"]
mod tests;
