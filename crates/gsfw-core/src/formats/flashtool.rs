//! Carve the files out of a `GameSir` "Flash Tool" `.exe`.
//!
//! - The overlay starts at the end of the last PE section's raw data.
//! - The overlay is a prefix, then Qt `qCompress` data: the archive length (`u32`, big-endian)
//!   and a zlib stream. The prefix is different in each tool. Its length is 0 to 5536 bytes.
//! - The inflated archive has a 0x20-byte header and a table of 0x210-byte records. The header
//!   gives the number of records and the archive length. Each record gives the offset, the size,
//!   the CRC-16 and the name of one file.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

use super::FormatError;
use super::cipher::crc16;
use crate::bytes::{ascii_name, index, tail, u16_at, u32_at, window};

/// Length of the archive header.
pub const HEADER_LEN: usize = 0x20;
/// Length of one record of the file table.
pub const RECORD_LEN: usize = 0x210;

/// PE format (Microsoft PE/COFF specification): the DOS header offset of the PE header
/// offset, and the PE header offsets of the section count and the optional header length.
const E_LFANEW_AT: usize = 0x3C;
const SECTION_COUNT_AT: usize = 6;
const OPTIONAL_LEN_AT: usize = 20;
/// Length of the PE signature and the COFF file header, before the optional header.
const PE_HEADER_LEN: usize = 24;
/// Length of one section header, and its offsets of the raw data size and pointer.
const SECTION_LEN: usize = 40;
const RAW_SIZE_AT: usize = 16;
const RAW_POINTER_AT: usize = 20;

/// zlib header (RFC 1950, section 2.2): CMF for deflate with a 32 KiB window, the FDICT flag,
/// and the divisor of the FCHECK test.
const ZLIB_CMF: u8 = 0x78;
const ZLIB_FDICT: u8 = 0x20;
const ZLIB_CHECK: u16 = 31;
/// Length of the big-endian archive length before the zlib stream.
const DECLARED_LEN: usize = 4;

/// Archive header offsets of the record count and the archive length.
const COUNT_AT: usize = 4;
const TOTAL_AT: usize = 8;
/// Record offsets of the CRC, the offset, the size and the name.
const CRC_AT: usize = 2;
const OFFSET_AT: usize = 4;
const SIZE_AT: usize = 8;
const NAME_AT: usize = 0x10;

fn invalid(message: &str) -> FormatError {
    FormatError::Invalid(message.to_owned())
}

/// One file of the archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name as stored, for example `data\config.ini`.
    pub name: String,
    /// CRC-16/XMODEM of the file.
    pub crc: u16,
    /// Offset in the inflated archive.
    pub offset: u32,
    /// Length in bytes.
    pub size: u32,
}

/// The bytes after the last PE section: the Flash Tool's packed data.
///
/// # Errors
/// [`FormatError::Invalid`] when `exe` is not a PE file or has no data after its sections.
pub fn overlay(exe: &[u8]) -> Result<&[u8], FormatError> {
    let not_pe = || invalid("not a PE file");
    if !exe.starts_with(b"MZ") {
        return Err(not_pe());
    }
    let pe = index(u32_at(exe, E_LFANEW_AT).ok_or_else(not_pe)?);
    if !tail(exe, pe).starts_with(b"PE\0\0") {
        return Err(not_pe());
    }
    let sections = u16_at(exe, pe.saturating_add(SECTION_COUNT_AT)).ok_or_else(not_pe)?;
    let optional_len = u16_at(exe, pe.saturating_add(OPTIONAL_LEN_AT)).ok_or_else(not_pe)?;
    let table = pe
        .saturating_add(PE_HEADER_LEN)
        .saturating_add(usize::from(optional_len));
    let mut end = 0;
    for n in 0..usize::from(sections) {
        let record = table.saturating_add(n.saturating_mul(SECTION_LEN));
        let size = u32_at(exe, record.saturating_add(RAW_SIZE_AT)).ok_or_else(not_pe)?;
        let start = u32_at(exe, record.saturating_add(RAW_POINTER_AT)).ok_or_else(not_pe)?;
        end = end.max(index(start).saturating_add(index(size)));
    }
    match exe.get(end..) {
        Some(rest) if !rest.is_empty() => Ok(rest),
        _ => Err(invalid("no data after the PE sections")),
    }
}

/// True when `pair` is a zlib header (RFC 1950, section 2.2) for deflate with a 32 KiB window
/// and no preset dictionary.
const fn zlib_header(pair: [u8; 2]) -> bool {
    let [cmf, flg] = pair;
    cmf == ZLIB_CMF && flg & ZLIB_FDICT == 0 && u16::from_be_bytes(pair).is_multiple_of(ZLIB_CHECK)
}

/// The archive in the zlib stream at the start of `stream`, when it is `declared` bytes long.
///
/// # Errors
/// [`FormatError::Invalid`] when the stream is bad or its length is not `declared`.
fn inflate_at(stream: &[u8], declared: u32) -> Result<Vec<u8>, FormatError> {
    let mut archive = Vec::new();
    // Read one byte more than declared, so that a longer stream shows as a length error.
    ZlibDecoder::new(stream)
        .take(u64::from(declared).saturating_add(1))
        .read_to_end(&mut archive)
        .map_err(|err| FormatError::Invalid(format!("the overlay zlib stream: {err}")))?;
    if archive.len() != index(declared) {
        return Err(FormatError::Invalid(format!(
            "the archive is {} bytes, the overlay says {declared}",
            archive.len()
        )));
    }
    Ok(archive)
}

/// The inflated archive of an overlay: the first zlib stream that inflates to the length in
/// the 4 bytes before it.
///
/// # Errors
/// [`FormatError::Invalid`] when no zlib stream in the overlay inflates to its length. The
/// message is the failure of the last stream.
pub fn inflate(overlay: &[u8]) -> Result<Vec<u8>, FormatError> {
    let mut failure = invalid("the overlay has no zlib stream");
    for (at, head) in overlay.windows(DECLARED_LEN + 2).enumerate() {
        let [a, b, c, d, cmf, flg] = *head else {
            continue;
        };
        if !zlib_header([cmf, flg]) {
            continue;
        }
        let declared = u32::from_be_bytes([a, b, c, d]);
        match inflate_at(tail(overlay, at.saturating_add(DECLARED_LEN)), declared) {
            Ok(archive) => return Ok(archive),
            Err(err) => failure = err,
        }
    }
    Err(failure)
}

/// The file table of an inflated archive. Every entry is inside the archive and matches its
/// CRC.
///
/// # Errors
/// [`FormatError::Invalid`] when the length, a range or a CRC is wrong.
///
/// The first 4 bytes of the header are not checked. They are different in each tool, and they
/// are not the CRC-32 or the Adler-32 of the archive.
pub fn entries(archive: &[u8]) -> Result<Vec<Entry>, FormatError> {
    let truncated = || invalid("the archive table is truncated");
    let count = u32_at(archive, COUNT_AT).ok_or_else(truncated)?;
    let total = u32_at(archive, TOTAL_AT).ok_or_else(truncated)?;
    if index(total) != archive.len() {
        return Err(FormatError::Invalid(format!(
            "the archive is {} bytes, its header says {total}",
            archive.len()
        )));
    }
    let mut found = Vec::new();
    for n in 0..index(count) {
        let at = HEADER_LEN.saturating_add(n.saturating_mul(RECORD_LEN));
        let record = window(archive, at, RECORD_LEN);
        if record.len() != RECORD_LEN {
            return Err(truncated());
        }
        let entry = Entry {
            crc: u16_at(record, CRC_AT).ok_or_else(truncated)?,
            offset: u32_at(record, OFFSET_AT).ok_or_else(truncated)?,
            size: u32_at(record, SIZE_AT).ok_or_else(truncated)?,
            name: ascii_name(tail(record, NAME_AT)),
        };
        let body = window(archive, index(entry.offset), index(entry.size));
        if body.len() != index(entry.size) {
            return Err(FormatError::Invalid(format!(
                "{}: {} bytes at {:#x} is outside the archive",
                entry.name, entry.size, entry.offset
            )));
        }
        let crc = crc16(body);
        if crc != entry.crc {
            return Err(FormatError::Invalid(format!(
                "{}: CRC {crc:#06x}, the table says {:#06x}",
                entry.name, entry.crc
            )));
        }
        found.push(entry);
    }
    Ok(found)
}

/// The path of the stored name `name` under `out`. The name must be relative and stay under
/// `out`.
///
/// # Errors
/// [`FormatError::Invalid`] for an empty, absolute or `..` part.
pub fn target(out: &Path, name: &str) -> Result<PathBuf, FormatError> {
    let mut path = out.to_path_buf();
    for part in name.split(['\\', '/']) {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(FormatError::Invalid(format!(
                "{name}: the name is not a relative path"
            )));
        }
        path.push(part);
    }
    Ok(path)
}

/// Writes every file of the Flash Tool `exe` under `out`; returns `(length, path)` per file.
///
/// # Errors
/// [`FormatError::Io`] on a read or write failure; [`FormatError::Invalid`] when the overlay or
/// the archive is wrong. Nothing is written when the archive is wrong.
pub fn carve(exe: &Path, out: &Path) -> Result<Vec<(usize, PathBuf)>, FormatError> {
    let data = fs::read(exe).map_err(|e| FormatError::io(exe, e))?;
    let archive = inflate(overlay(&data)?)?;
    let files = entries(&archive)?
        .into_iter()
        .map(|entry| Ok((target(out, &entry.name)?, entry)))
        .collect::<Result<Vec<_>, FormatError>>()?;
    let mut written = Vec::with_capacity(files.len());
    for (path, entry) in files {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| FormatError::io(parent, e))?;
        }
        let body = window(&archive, index(entry.offset), index(entry.size));
        fs::write(&path, body).map_err(|e| FormatError::io(&path, e))?;
        written.push((body.len(), path));
    }
    Ok(written)
}

#[cfg(test)]
pub mod tests;
