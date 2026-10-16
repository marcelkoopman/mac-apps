//! Word documents (.docx): the text of `word/document.xml`, read from the zip in memory.
//!
//! A docx is a zip. This module reads its central directory, inflates the one entry it needs
//! and pulls the text out with a small tolerant scanner (no XML crate). Nothing is extracted to
//! disk, nothing is logged, and every buffer that holds document content is zeroizing and of
//! fixed size: it is allocated once at the declared (and checked) size and never grows, so no
//! reallocation leaves an un-wiped copy behind. Not covered: the inflate state of `flate2` (its
//! 32 KiB window holds the last bytes of the document and is freed without wiping) and
//! whatever the allocator or the OS did with the pages. Limits
//! guard against zip bombs: the packed file, the unpacked `document.xml`, the number of entries
//! and the extracted text are all capped, and a declared size that the data does not match is
//! refused. Only the entry name `word/document.xml` is ever looked at, so a crafted entry path
//! (`../x`) has nowhere to go.

use std::io::Read;

use zeroize::Zeroizing;

/// Packed docx files larger than this stay off the card (the same limit as a PDF).
pub const MAX_DOCX_BYTES: u64 = 32_000_000;
/// `word/document.xml` unpacked, at most.
pub const MAX_XML_BYTES: usize = 32_000_000;
/// Entries in the zip, at most (a real document has a few dozen).
pub const MAX_ENTRIES: usize = 10_000;
/// Extracted text, at most (the card's text limit).
pub const MAX_TEXT_BYTES: usize = 8_000_000;

const DOCUMENT_XML: &[u8] = b"word/document.xml";

/// Why a docx gave no text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocxError {
    /// Not a zip with `word/document.xml`: not a Word document.
    NotDocx,
    /// Password protected (zip encryption, or an encrypted Office container).
    Locked,
    /// Damaged, or a zip feature this reader does not do (zip64, other compression methods).
    Unreadable,
    /// Unpacks larger than the limits allow.
    Bomb,
    /// The text is larger than the card takes.
    TooLarge,
    /// A document without any text.
    NoText,
}

/// The file starts like a zip (a local file header).
pub fn has_zip_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// The file starts like an encrypted Office document (an OLE container, not a zip).
pub fn has_ole_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([raw[0], raw[1]]) as usize)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize)
}

/// The bytes of the file at `path`, in one zeroizing buffer of exactly the file's size, which is
/// at most `max` (else `None`). The buffer never grows: a file that changes size while it is read
/// is refused rather than reallocated (`fs::read` would leave the old, un-wiped buffer behind).
pub fn read_file(path: &std::path::Path, max: u64) -> Option<Zeroizing<Vec<u8>>> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size > max {
        return None;
    }
    let mut buf = Zeroizing::new(vec![0u8; usize::try_from(size).ok()?]);
    file.read_exact(&mut buf).ok()?;
    let mut probe = Zeroizing::new([0u8; 1]);
    (file.read(&mut *probe).ok()? == 0).then_some(buf)
}

/// Where `word/document.xml` sits in the zip.
struct Entry {
    method: usize,
    packed: usize,
    unpacked: usize,
    local: usize,
}

/// Walk the central directory for `word/document.xml`.
fn find_document(zip: &[u8]) -> Result<Entry, DocxError> {
    // End of central directory: the last 22 bytes, or before a comment of up to 64 KiB.
    let from = zip.len().saturating_sub(22 + 65_535);
    let tail = zip.get(from..).ok_or(DocxError::NotDocx)?;
    let eocd = tail
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .map(|at| from + at)
        .ok_or(DocxError::NotDocx)?;
    let total = u16_at(zip, eocd + 10).ok_or(DocxError::Unreadable)?;
    let dir_size = u32_at(zip, eocd + 12).ok_or(DocxError::Unreadable)?;
    let dir_start = u32_at(zip, eocd + 16).ok_or(DocxError::Unreadable)?;
    if total == 0xFFFF || dir_size == 0xFFFF_FFFF || dir_start == 0xFFFF_FFFF {
        return Err(DocxError::Unreadable); // zip64
    }
    if total > MAX_ENTRIES {
        return Err(DocxError::Bomb);
    }
    let dir_end = dir_start
        .checked_add(dir_size)
        .ok_or(DocxError::Unreadable)?;
    if dir_end > eocd {
        return Err(DocxError::Unreadable);
    }
    let mut at = dir_start;
    for _ in 0..total {
        if zip.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(DocxError::Unreadable);
        }
        let flags = u16_at(zip, at + 8).ok_or(DocxError::Unreadable)?;
        let method = u16_at(zip, at + 10).ok_or(DocxError::Unreadable)?;
        let packed = u32_at(zip, at + 20).ok_or(DocxError::Unreadable)?;
        let unpacked = u32_at(zip, at + 24).ok_or(DocxError::Unreadable)?;
        let name_len = u16_at(zip, at + 28).ok_or(DocxError::Unreadable)?;
        let extra_len = u16_at(zip, at + 30).ok_or(DocxError::Unreadable)?;
        let comment_len = u16_at(zip, at + 32).ok_or(DocxError::Unreadable)?;
        let local = u32_at(zip, at + 42).ok_or(DocxError::Unreadable)?;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or(DocxError::Unreadable)?;
        if name == DOCUMENT_XML {
            if flags & 1 != 0 || flags & 0x40 != 0 {
                return Err(DocxError::Locked);
            }
            if packed == 0xFFFF_FFFF || unpacked == 0xFFFF_FFFF {
                return Err(DocxError::Unreadable);
            }
            return Ok(Entry {
                method,
                packed,
                unpacked,
                local,
            });
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    Err(DocxError::NotDocx)
}

/// The inflated `word/document.xml`, in one exact-size zeroizing buffer (allocated once, never grown).
fn read_document(zip: &[u8]) -> Result<Zeroizing<Vec<u8>>, DocxError> {
    let entry = find_document(zip)?;
    if entry.unpacked > MAX_XML_BYTES {
        return Err(DocxError::Bomb);
    }
    if zip.get(entry.local..entry.local + 4) != Some(b"PK\x03\x04") {
        return Err(DocxError::Unreadable);
    }
    let name_len = u16_at(zip, entry.local + 26).ok_or(DocxError::Unreadable)?;
    let extra_len = u16_at(zip, entry.local + 28).ok_or(DocxError::Unreadable)?;
    let start = entry.local + 30 + name_len + extra_len;
    let data = zip
        .get(
            start
                ..start
                    .checked_add(entry.packed)
                    .ok_or(DocxError::Unreadable)?,
        )
        .ok_or(DocxError::Unreadable)?;
    // One buffer of the declared size (checked above against MAX_XML_BYTES), filled in place:
    // nothing is ever appended, so it cannot reallocate.
    let mut out = Zeroizing::new(vec![0u8; entry.unpacked]);
    match entry.method {
        0 => {
            if entry.packed != entry.unpacked {
                return Err(DocxError::Unreadable);
            }
            out.copy_from_slice(data);
        }
        8 => {
            let mut inflate = flate2::read::DeflateDecoder::new(data);
            inflate
                .read_exact(&mut out)
                .map_err(|_| DocxError::Unreadable)?; // shorter than declared, or corrupt
            // One more byte tells a stream that outgrows its header (a bomb).
            let mut probe = Zeroizing::new([0u8; 1]);
            match inflate.read(&mut *probe) {
                Ok(0) => {}
                Ok(_) => return Err(DocxError::Bomb),
                Err(_) => return Err(DocxError::Unreadable),
            }
        }
        _ => return Err(DocxError::Unreadable),
    }
    Ok(out)
}

/// The text of a docx file's bytes.
pub fn extract(zip: &[u8]) -> Result<Zeroizing<String>, DocxError> {
    if !has_zip_magic(zip) {
        return Err(if has_ole_magic(zip) {
            DocxError::Locked
        } else {
            DocxError::NotDocx
        });
    }
    let xml = read_document(zip)?;
    let xml = std::str::from_utf8(&xml).map_err(|_| DocxError::Unreadable)?;
    text_of(xml)
}

/// Whether the zip holds a Word document (a `word/document.xml`), without reading it.
pub fn is_docx(zip: &[u8]) -> bool {
    has_zip_magic(zip) && !matches!(find_document(zip), Err(DocxError::NotDocx))
}

/// The text of a `document.xml`: paragraphs on their own lines, a tab for `w:tab`, a line
/// break for `w:br`, table cells separated by a tab and rows by a newline. The scanner is
/// tolerant: it never fails on odd markup, it only skips what it does not understand.
pub fn text_of(xml: &str) -> Result<Zeroizing<String>, DocxError> {
    // The output never outgrows this capacity: every push is checked first (see `put`), so the
    // buffer is never reallocated and no un-wiped copy is left behind. The text is never longer
    // than the markup it comes from, so the limit is only reached by the 8 MB cap.
    let limit = text_limit(xml.len());
    let mut out = Zeroizing::new(String::with_capacity(limit));
    let bytes = xml.as_bytes();
    let mut at = 0;
    // Depth of cells (tables nest), of `w:tabs` (tab stop definitions, not tabs) and of
    // `mc:Fallback` (a second copy of what its sibling already holds).
    let mut cells = 0usize;
    let mut tab_stops = 0usize;
    let mut fallback = 0usize;
    while let Some(lt) = memchr::memchr(b'<', &bytes[at..]) {
        let tag_start = at + lt;
        let rest = &bytes[tag_start..];
        // Comments, CDATA outside text, processing instructions, doctype.
        if rest.starts_with(b"<!--") {
            at = find(bytes, tag_start + 4, b"-->").map_or(bytes.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            at = find(bytes, tag_start + 9, b"]]>").map_or(bytes.len(), |end| end + 3);
            continue;
        }
        if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
            at = find(bytes, tag_start + 2, b">").map_or(bytes.len(), |end| end + 1);
            continue;
        }
        let Some(end) = tag_end(bytes, tag_start + 1) else {
            break;
        };
        let inside = &xml[tag_start + 1..end];
        at = end + 1;
        let closing = inside.starts_with('/');
        let self_closing = inside.ends_with('/');
        let name = inside
            .trim_start_matches('/')
            .split(|c: char| c.is_ascii_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        if name == "mc:Fallback" {
            if closing {
                fallback = fallback.saturating_sub(1);
            } else if !self_closing {
                fallback += 1;
            }
            continue;
        }
        if fallback > 0 {
            continue;
        }
        match (name, closing) {
            ("w:t", false) if !self_closing => {
                let stop = find(bytes, at, b"</").unwrap_or(bytes.len());
                push_text(&mut out, limit, &xml[at..stop])?;
                at = stop;
            }
            ("w:tab" | "w:ptab", false) if tab_stops == 0 => put(&mut out, limit, "\t")?,
            ("w:br" | "w:cr", false) => put(&mut out, limit, "\n")?,
            ("w:noBreakHyphen", false) => put(&mut out, limit, "-")?,
            ("w:tabs", false) if !self_closing => tab_stops += 1,
            ("w:tabs", true) => tab_stops = tab_stops.saturating_sub(1),
            ("w:tc", false) if !self_closing => cells += 1,
            ("w:p", true) => {
                if cells > 0 {
                    // Paragraphs of one cell stay on its line.
                    if !out.ends_with(['\t', '\n', ' ']) && !out.is_empty() {
                        put(&mut out, limit, " ")?;
                    }
                } else {
                    put(&mut out, limit, "\n")?;
                }
            }
            ("w:tc", true) => {
                cells = cells.saturating_sub(1);
                let kept = out.trim_end_matches(' ').len();
                out.truncate(kept);
                put(&mut out, limit, "\t")?;
            }
            ("w:tr", true) => {
                if out.ends_with('\t') {
                    out.pop();
                }
                put(&mut out, limit, "\n")?;
            }
            _ => {}
        }
    }
    let kept = out.trim_end().len();
    out.truncate(kept);
    if out.trim().is_empty() {
        return Err(DocxError::NoText);
    }
    Ok(out)
}

/// The capacity (and hard limit) of the text buffer for `xml_len` bytes of markup.
fn text_limit(xml_len: usize) -> usize {
    xml_len.saturating_add(16).min(MAX_TEXT_BYTES)
}

/// Append `text` unless it would not fit in the capacity the buffer started with.
fn put(out: &mut String, limit: usize, text: &str) -> Result<(), DocxError> {
    if out.len() + text.len() > limit {
        return Err(DocxError::TooLarge);
    }
    out.push_str(text);
    Ok(())
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    memchr::memmem::find(bytes.get(from..)?, needle).map(|at| from + at)
}

/// The `>` that ends the tag whose name starts at `from`, skipping quoted attribute values.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (at, &byte) in bytes.get(from..)?.iter().enumerate() {
        match byte {
            b'"' | b'\'' if quote == 0 => quote = byte,
            _ if byte == quote => quote = 0,
            b'>' if quote == 0 => return Some(from + at),
            _ => {}
        }
    }
    None
}

/// The text of a `w:t`: CDATA sections as they are, the rest with its entities decoded.
fn push_text(out: &mut String, limit: usize, text: &str) -> Result<(), DocxError> {
    let mut rest = text;
    while let Some(open) = rest.find("<![CDATA[") {
        push_decoded(out, limit, &rest[..open])?;
        rest = &rest[open + 9..];
        match rest.find("]]>") {
            Some(close) => {
                put(out, limit, &rest[..close])?;
                rest = &rest[close + 3..];
            }
            None => {
                return put(out, limit, rest);
            }
        }
    }
    push_decoded(out, limit, rest)
}

/// Text with its XML entities decoded: the five named ones and numeric references. Anything
/// else (an unknown `&name;`, a stray `&`) stays as written.
fn push_decoded(out: &mut String, limit: usize, text: &str) -> Result<(), DocxError> {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        put(out, limit, &rest[..amp])?;
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let entity = &rest[1..semi];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity.strip_prefix('#').and_then(|num| {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }),
            };
            ch.map(|ch| (ch, semi + 1))
        });
        match decoded {
            Some((ch, used)) => {
                put(out, limit, ch.encode_utf8(&mut [0; 4]))?;
                rest = &rest[used..];
            }
            None => {
                put(out, limit, "&")?;
                rest = &rest[1..];
            }
        }
    }
    put(out, limit, rest)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    const CRC: fn(&[u8]) -> u32 = |data| {
        let mut hasher = flate2::Crc::new();
        hasher.update(data);
        hasher.sum()
    };

    /// A zip with `entries` (name, data, deflate?), written by hand.
    fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut dir = Vec::new();
        for (name, data, deflate) in entries {
            let packed = if *deflate {
                let mut enc =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                enc.write_all(data).unwrap();
                enc.finish().unwrap()
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            let mut common = Vec::new();
            common.extend_from_slice(&20u16.to_le_bytes());
            common.extend_from_slice(&0u16.to_le_bytes()); // flags
            common.extend_from_slice(&method.to_le_bytes());
            common.extend_from_slice(&[0; 4]); // time, date
            common.extend_from_slice(&CRC(data).to_le_bytes());
            common.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            common.extend_from_slice(&(data.len() as u32).to_le_bytes());
            common.extend_from_slice(&(name.len() as u16).to_le_bytes());
            common.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&common);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            dir.extend_from_slice(b"PK\x01\x02");
            dir.extend_from_slice(&20u16.to_le_bytes()); // made by
            dir.extend_from_slice(&common);
            dir.extend_from_slice(&[0; 2 + 2 + 2 + 4]); // comment, disk, internal attrs, external attrs
            dir.extend_from_slice(&offset.to_le_bytes());
            dir.extend_from_slice(name.as_bytes());
        }
        let dir_start = out.len() as u32;
        out.extend_from_slice(&dir);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
        out.extend_from_slice(&dir_start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn doc(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?><w:document xmlns:w=\"x\"><w:body>{body}</w:body></w:document>"
        )
    }

    pub(crate) fn docx(body: &str, deflate: bool) -> Vec<u8> {
        zip(&[
            ("[Content_Types].xml", b"<Types/>", false),
            ("word/document.xml", doc(body).as_bytes(), deflate),
        ])
    }

    #[test]
    fn paragraphs_tabs_and_breaks() {
        let body = "<w:p><w:r><w:t>Hello</w:t><w:tab/><w:t xml:space=\"preserve\"> world</w:t></w:r></w:p>\
                    <w:p><w:r><w:t>one</w:t><w:br/><w:t>two</w:t></w:r></w:p><w:p/>";
        for deflate in [false, true] {
            let text = extract(&docx(body, deflate)).unwrap();
            assert_eq!(text.as_str(), "Hello\t world\none\ntwo");
        }
    }

    #[test]
    fn tab_stops_are_not_tabs() {
        let body = "<w:p><w:pPr><w:tabs><w:tab w:val=\"left\" w:pos=\"720\"/></w:tabs></w:pPr>\
                    <w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r></w:p>";
        assert_eq!(text_of(&doc(body)).unwrap().as_str(), "a\tb");
    }

    #[test]
    fn tables_use_tabs_and_newlines() {
        let body = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>\
                    <w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p><w:p><w:r><w:t>b2</w:t></w:r></w:p></w:tc></w:tr>\
                    <w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl>\
                    <w:p><w:r><w:t>after</w:t></w:r></w:p>";
        assert_eq!(text_of(&doc(body)).unwrap().as_str(), "a\tb b2\nc\t\nafter");
    }

    #[test]
    fn entities_are_decoded() {
        let body = "<w:p><w:r><w:t>a &amp; b &lt;c&gt; &quot;d&quot; &#233; &#x1F600; &unknown; &amp</w:t></w:r></w:p>";
        assert_eq!(
            text_of(&doc(body)).unwrap().as_str(),
            "a & b <c> \"d\" é 😀 &unknown; &amp"
        );
    }

    #[test]
    fn deleted_text_and_fallback_copies_are_left_out() {
        let body = "<w:p><w:del><w:r><w:delText>gone</w:delText></w:r></w:del><w:r><w:t>kept</w:t></w:r>\
                    <mc:AlternateContent><mc:Choice><w:r><w:t>box</w:t></w:r></mc:Choice>\
                    <mc:Fallback><w:r><w:t>box</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p>";
        assert_eq!(text_of(&doc(body)).unwrap().as_str(), "keptbox");
    }

    #[test]
    fn tolerates_odd_markup() {
        let body = "<!-- <w:t>no</w:t> --><w:p><w:r><w:t>a<![CDATA[x]]></w:t></w:r></w:p><w:p><w:r><w:t a=\">\">b</w:t></w:r></w:p><w:p><w:r><w:t>cut";
        let text = text_of(body).unwrap();
        assert!(text.starts_with("ax\nb\n"), "{text:?}");
    }

    #[test]
    fn empty_document_has_no_text() {
        assert_eq!(
            extract(&docx("<w:p/>", true)).unwrap_err(),
            DocxError::NoText
        );
    }

    #[test]
    fn missing_document_xml_is_not_a_docx() {
        let other = zip(&[("hello.txt", b"hi", false)]);
        assert_eq!(extract(&other).unwrap_err(), DocxError::NotDocx);
        assert!(!is_docx(&other));
        assert!(is_docx(&docx("<w:p/>", false)));
    }

    #[test]
    fn garbage_and_truncated_files() {
        assert_eq!(extract(b"hello").unwrap_err(), DocxError::NotDocx);
        assert_eq!(extract(b"PK\x03\x04junk").unwrap_err(), DocxError::NotDocx);
        let good = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", true);
        for cut in [10, good.len() / 2, good.len() - 5] {
            assert!(extract(&good[..cut]).is_err());
        }
        // A corrupt deflate stream.
        let mut bad = docx("<w:p><w:r><w:t>hello hello hello</w:t></w:r></w:p>", true);
        let at = bad.windows(17).position(|w| w == DOCUMENT_XML).unwrap() + 17;
        bad[at..at + 6].fill(0xFF);
        assert!(extract(&bad).is_err());
    }

    #[test]
    fn encrypted_is_locked() {
        // Zip encryption: general purpose flag bit 0 in the central directory entry.
        let mut zipped = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", false);
        let dir = zipped.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        // The directory holds two entries; flip the flag of the one for document.xml (the last).
        zipped[dir + 8] |= 1;
        assert_eq!(extract(&zipped).unwrap_err(), DocxError::Locked);
        // An encrypted Office container is an OLE file, not a zip.
        let mut ole = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        ole.extend_from_slice(&[0; 64]);
        assert_eq!(extract(&ole).unwrap_err(), DocxError::Locked);
    }

    #[test]
    fn bomb_limits() {
        // Declared size over the limit.
        let mut zipped = docx("<w:p/>", false);
        let dir = zipped.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        zipped[dir + 24..dir + 28].copy_from_slice(&(MAX_XML_BYTES as u32 + 1).to_le_bytes());
        assert_eq!(extract(&zipped).unwrap_err(), DocxError::Bomb);
        // A stream that outgrows its declared size.
        let mut lying = docx(
            &"<w:p><w:r><w:t>aaaaaaaa</w:t></w:r></w:p>".repeat(100),
            true,
        );
        let dir = lying.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        lying[dir + 24..dir + 28].copy_from_slice(&50u32.to_le_bytes());
        assert_eq!(extract(&lying).unwrap_err(), DocxError::Bomb);
        // Too many entries.
        let mut many = docx("<w:p/>", false);
        let eocd = many.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
        many[eocd + 10..eocd + 12].copy_from_slice(&((MAX_ENTRIES as u16) + 1).to_le_bytes());
        assert_eq!(extract(&many).unwrap_err(), DocxError::Bomb);
        // Text over the card's limit.
        let big = "<w:p><w:r><w:t>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</w:t></w:r></w:p>"
            .repeat(MAX_TEXT_BYTES / 60);
        assert_eq!(text_of(&doc(&big)).unwrap_err(), DocxError::TooLarge);
    }

    #[test]
    fn text_buffer_never_grows() {
        // The output is allocated once at its limit and stays within it.
        let body =
            "<w:p><w:r><w:t>a &amp; b &#x1F600; <![CDATA[x]]></w:t><w:tab/></w:r></w:p>".repeat(50);
        let xml = doc(&body);
        let text = text_of(&xml).unwrap();
        assert_eq!(text.capacity(), text_limit(xml.len()));
        assert!(text.len() <= text.capacity());
        // Pushing past the limit is refused, and does not touch the capacity.
        let mut out = String::with_capacity(4);
        let cap = out.capacity();
        assert!(put(&mut out, 4, "abcd").is_ok());
        assert_eq!(put(&mut out, 4, "e"), Err(DocxError::TooLarge));
        assert_eq!(out.capacity(), cap);
        assert_eq!(
            push_decoded(&mut String::new(), 3, "ab&amp;c"),
            Err(DocxError::TooLarge)
        );
    }

    #[test]
    fn inflated_buffer_has_exactly_the_declared_size() {
        let body = "<w:p><w:r><w:t>exact</w:t></w:r></w:p>".repeat(20);
        for deflate in [false, true] {
            let zipped = docx(&body, deflate);
            let xml = read_document(&zipped).unwrap();
            assert_eq!(xml.len(), doc(&body).len());
            assert_eq!(xml.capacity(), xml.len());
        }
        // Declared larger than the stream holds: refused, not padded.
        let mut short = docx(&body, true);
        let dir = short.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
        let real = u32::from_le_bytes(short[dir + 24..dir + 28].try_into().unwrap());
        short[dir + 24..dir + 28].copy_from_slice(&(real + 10).to_le_bytes());
        assert_eq!(extract(&short).unwrap_err(), DocxError::Unreadable);
    }

    #[test]
    fn read_file_is_exact_size_and_limited() {
        let path =
            std::env::temp_dir().join(format!("copycraft-docx-test-{}.bin", std::process::id()));
        std::fs::write(&path, b"hello world").unwrap();
        let bytes = read_file(&path, 100).unwrap();
        assert_eq!(&bytes[..], b"hello world");
        assert_eq!(bytes.capacity(), 11);
        assert!(read_file(&path, 10).is_none());
        std::fs::remove_file(&path).unwrap();
        assert!(read_file(&path, 100).is_none());
    }
}
