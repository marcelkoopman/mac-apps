//! Zip archives dropped on the card: the names inside, and extracting them into a folder.
//!
//! Read by hand like [`crate::docx`] (no zip crate, no subprocess): the central directory is
//! read from the end of the file, entries are inflated with `flate2` (method 8) or copied
//! (method 0). Zip64 is read. Nothing is logged and nothing is cached: the listing lives on
//! the card only, and only the folder the user chose is written to.
//!
//! Safety:
//! - Every entry name must be a plain relative path: no leading `/`, no `..`, no drive
//!   (`C:`), no NUL. A backslash counts as a separator (old Windows zips). One unsafe entry
//!   makes the whole zip refuse to extract ([`ZipError::Unsafe`]); the names still show.
//! - Symbolic links are refused the same way, and so are two entries with the same path (also
//!   when they differ only in case: the Mac's disk usually does not tell them apart).
//! - `__MACOSX/` (Finder's resource forks) is left out of the listing and not extracted.
//! - Sizes are checked: an entry that unpacks to more or other bytes than its header says
//!   (CRC-32) fails the extraction.
//!
//! Never overwrites ([`plan_target`]): when none of the zip's top-level names exists in the
//! chosen folder the zip is extracted right into it; otherwise into a new folder named after
//! the zip ("Archive", "Archive 2", …). Every folder is made with `create_dir` and every file
//! with `create_new`, so a name that appears meanwhile fails the extraction instead of being
//! replaced. A failed extraction removes what it made, and only that.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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
}

impl ZipFacts {
    /// What stops extracting, if anything.
    pub fn blocker(&self) -> Option<ZipError> {
        if self.locked {
            Some(ZipError::Locked)
        } else if self.unsafe_names {
            Some(ZipError::Unsafe)
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
            // The same path twice, also in another case, would overwrite the first one.
            let key = parts.join("/").to_lowercase();
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
    /// [`SHOWN_LINES`] lines, then `more(n)`. Unsafe names show as stored.
    pub fn text(&self, more: impl Fn(usize) -> String) -> Zeroizing<String> {
        // path → (is folder, entries right in it)
        let mut lines: std::collections::BTreeMap<String, (bool, usize)> = Default::default();
        let mut children: HashMap<String, HashSet<String>> = HashMap::new();
        for entry in &self.entries {
            let Some(parts) = entry.parts.as_ref().filter(|parts| !parts.is_empty()) else {
                lines.insert(entry.name.clone(), (entry.dir, 0));
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
    pub fn top_names(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for parts in self.entries.iter().filter_map(|e| e.parts.as_ref()) {
            if let Some(first) = parts.first()
                && seen.insert(first.to_lowercase())
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

/// The path parts of an entry name, when it stays inside the folder it is extracted to:
/// relative (no leading `/` or `\`), no `..`, no drive (`C:`), no NUL, each part at most 255
/// bytes. Empty and `.` parts are skipped (`a//b`, `./a`). `None` for an unsafe name.
pub fn safe_parts(name: &str) -> Option<Vec<String>> {
    if name.is_empty() || name.contains('\0') {
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
        if part.len() > 255 {
            return None;
        }
        parts.push(part.to_string());
    }
    Some(parts)
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

/// What was made, so a failed extraction can take it away again (and nothing else).
#[derive(Default)]
struct Made {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    /// Folders made, by their lowercased path inside the target.
    dir_keys: HashSet<String>,
}

impl Made {
    fn undo(&mut self) {
        for file in self.files.drain(..).rev() {
            let _ = std::fs::remove_file(file);
        }
        for dir in self.dirs.drain(..).rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Where the zip went: the folder it was extracted into (the chosen one or the zip's own),
/// and whether that is a new folder named after the zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub folder: PathBuf,
    pub own_folder: bool,
}

/// Extract the zip at `zip` (called `zip_name`) into the folder `dest`, by the rules above.
pub fn extract(zip: &Path, zip_name: &str, dest: &Path) -> Result<Extracted, ZipError> {
    let mut file = std::fs::File::open(zip).map_err(|_| ZipError::Read)?;
    let len = file.metadata().map_err(|_| ZipError::Read)?.len();
    let listing = list(&mut file, len)?;
    if let Some(blocker) = listing.facts().blocker() {
        return Err(blocker);
    }
    if !std::fs::metadata(dest).is_ok_and(|meta| meta.is_dir()) {
        return Err(ZipError::Write);
    }
    let top = listing.top_names();
    let mut made = Made::default();
    let (root, own_folder) = match plan_target(&top, |name| {
        std::fs::symlink_metadata(dest.join(name)).is_ok()
    }) {
        Target::Direct => (dest.to_path_buf(), false),
        Target::Folder => (own_folder(dest, zip_name, &mut made)?, true),
    };
    let mut reader = std::io::BufReader::new(file);
    let result = listing
        .entries
        .iter()
        .try_for_each(|entry| extract_entry(&mut reader, entry, &root, &mut made));
    match result {
        Ok(()) => Ok(Extracted {
            folder: root,
            own_folder,
        }),
        Err(error) => {
            made.undo();
            Err(error)
        }
    }
}

/// A new folder in `dest` named after the zip ([`folder_name`]); made, never reused.
fn own_folder(dest: &Path, zip_name: &str, made: &mut Made) -> Result<PathBuf, ZipError> {
    for number in 1..=MAX_FOLDER_TRIES {
        let path = dest.join(folder_name(zip_name, number));
        match std::fs::create_dir(&path) {
            Ok(()) => {
                made.dirs.push(path.clone());
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(ZipError::Write),
        }
    }
    Err(ZipError::Write)
}

/// Make the folder `parts` inside `root` (and the ones above it), unless this extraction made
/// it already.
fn make_dirs(root: &Path, parts: &[String], made: &mut Made) -> Result<PathBuf, ZipError> {
    let mut path = root.to_path_buf();
    for end in 1..=parts.len() {
        path.push(&parts[end - 1]);
        let key = parts[..end].join("/").to_lowercase();
        if made.dir_keys.contains(&key) {
            continue;
        }
        match std::fs::create_dir(&path) {
            Ok(()) => {
                made.dirs.push(path.clone());
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if entry.executable() { 0o755 } else { 0o644 });
    }
    let mut out = options.open(&path).map_err(|_| ZipError::Write)?;
    made.files.push(path);
    let mut buf = Zeroizing::new(vec![0u8; 64 * 1024]);
    let mut crc = flate2::Crc::new();
    let mut written: u64 = 0;
    loop {
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
