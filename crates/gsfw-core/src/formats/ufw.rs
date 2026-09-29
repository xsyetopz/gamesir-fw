//! `JieLi` AC695X `.ufw` upgrade images (layout: `notes/facts.md`, "`JL_Upgrade_Gip.dll` firmware
//! loader") and download-tool `.fw` images (`notes/facts.md`, "v6.40 .fw format and
//! descriptors").
//!
//! - Header and entry table are scrambled with the ENC keystream, key 0xFFFF.
//! - Entry bodies (except flash images) use the SFC cipher with the chip key: 32-byte blocks,
//!   each ENC-keyed with `chipkey ^ (file_offset >> 2)`. `_No_Key` images use chip key 0xFFFF.
//! - The chip key is recovered from `isd_config.ini`, whose first 32 plaintext bytes are '#'.
//! - `.fw` (chosen by file suffix): same container, but the header and entry table are ENC-keyed
//!   with the chip key, entry CRCs cover the padded stored bytes, and only `isd_config.ini` is
//!   SFC-encrypted, with offsets relative to the entry.

use core::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::FormatError;
use super::cipher::{ENC_DEFAULT_KEY, SFC_OFFSET_SHIFT, crc16, enc, sfc};
use crate::bytes::{ascii_name, find, index, low16, tail, u16_at, u32_at, window};

/// Length of the container header.
pub const HEADER_LEN: usize = 0x40;
/// Length of one entry-table record.
pub const ENTRY_LEN: usize = 0x50;
/// Entry types sent as-is to 0x56d0 by the loader (decrypted on the chip).
pub const FLASH_TYPES: [u16; 6] = [0x00, 0x20, 0x21, 0x22, 0x23, 0x24];
/// Entry type of `isd_config.ini`.
pub const ISD_CONFIG_TYPE: u16 = 0x34;
/// The first 32 plaintext bytes of every `isd_config.ini`.
pub const ISD_PLAINTEXT_HEAD: [u8; 32] = [b'#'; 32];
/// Entry type of the flash image that holds `app.bin`.
const APP_FLASH_TYPE: u16 = 0x00;

/// Header offsets: the CRCs, the image size, the entry count, two unknown fields, the chip name.
const HDR_CRC_AT: usize = 0;
const LIST_CRC_AT: usize = 2;
const SIZE_AT: usize = 4;
const COUNT_AT: usize = 8;
const F0A_AT: usize = 10;
const F0C_AT: usize = 12;
const CHIP_AT: usize = 0x10;
const CHIP_LEN: usize = HEADER_LEN - CHIP_AT;
/// The header CRC covers the header from this offset.
const HDR_CRC_COVERS: usize = LIST_CRC_AT;

/// Entry-record offsets.
const KIND_AT: usize = 0;
const INDEX_AT: usize = 2;
const ENTRY_CRC_AT: usize = 4;
const OFFSET_AT: usize = 8;
const ENTRY_SIZE_AT: usize = 12;
const PADDED_AT: usize = 16;
const NAME_AT: usize = 0x40;
const NAME_LEN: usize = ENTRY_LEN - NAME_AT;

/// JLFS entry length and offsets.
const JLFS_LEN: usize = 32;
const JLFS_HDR_CRC_AT: usize = 0;
const JLFS_CRC_AT: usize = 2;
const JLFS_OFFSET_AT: usize = 4;
const JLFS_SIZE_AT: usize = 8;
const JLFS_ATTR_AT: usize = 12;
const JLFS_INDEX_AT: usize = 14;
const JLFS_NAME_AT: usize = 16;
const JLFS_NAME_LEN: usize = JLFS_LEN - JLFS_NAME_AT;
/// The JLFS entry CRC covers the entry from this offset.
const JLFS_CRC_COVERS: usize = JLFS_CRC_AT;
/// Offset of the top JLFS table in the flash image.
const TOP_TABLE_AT: usize = 0x20;

fn short(what: &str, len: usize) -> FormatError {
    FormatError::Invalid(format!("{what} of {len} bytes is too short"))
}

/// One record of the entry table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Entry type (`0` flash image, `0x34` `isd_config.ini`, ...).
    pub kind: u16,
    /// Index shown by the tools.
    pub index: u16,
    /// CRC-16 of the plaintext (`.ufw`) or of the padded stored bytes (`.fw`).
    pub crc: u16,
    /// File offset of the body.
    pub offset: u32,
    /// Body length.
    pub size: u32,
    /// Body length padded to the cipher block.
    pub padded: u32,
    /// File name, up to the first NUL.
    pub name: String,
}

impl Entry {
    /// Descrambles one table record with `key` and reads its fields.
    ///
    /// # Errors
    /// [`FormatError::Invalid`] when the record is shorter than its fixed fields.
    pub fn parse(raw: &[u8], key: u16) -> Result<Self, FormatError> {
        let plain = enc(raw, key);
        let field16 = |at| u16_at(&plain, at).ok_or_else(|| short("entry", raw.len()));
        let field32 = |at| u32_at(&plain, at).ok_or_else(|| short("entry", raw.len()));
        Ok(Self {
            kind: field16(KIND_AT)?,
            index: field16(INDEX_AT)?,
            crc: field16(ENTRY_CRC_AT)?,
            offset: field32(OFFSET_AT)?,
            size: field32(ENTRY_SIZE_AT)?,
            padded: field32(PADDED_AT)?,
            name: ascii_name(window(&plain, NAME_AT, NAME_LEN)),
        })
    }
}

/// Which container rules apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Nexus `.ufw`: header key 0xFFFF, entry bodies SFC-keyed by absolute offset.
    Ufw,
    /// Download-tool `.fw`: header key is the chip key, only `isd_config.ini` is encrypted.
    Fw,
}

/// A parsed `.ufw` or `.fw` image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ufw {
    /// The whole file.
    pub data: Vec<u8>,
    /// Container rules.
    pub format: Format,
    /// ENC key of the header and entry table.
    pub hdr_key: u16,
    /// Stored CRC of header bytes 2..0x40.
    pub hdr_crc: u16,
    /// Stored CRC of the entry table.
    pub list_crc: u16,
    /// Image size from the header.
    pub size: u32,
    /// Number of entries.
    pub count: u16,
    /// Header field at 0x0a (meaning unknown).
    pub f0a: u16,
    /// Header field at 0x0c (meaning unknown).
    pub f0c: u32,
    /// Chip name, e.g. `AC695X`.
    pub chip: String,
    /// The header CRC matches.
    pub hdr_ok: bool,
    /// The entry-table CRC matches.
    pub list_ok: bool,
    /// The entry table.
    pub entries: Vec<Entry>,
    /// The chip key, when it could be recovered.
    pub chipkey: Option<u16>,
}

impl Ufw {
    /// Parses a Nexus `.ufw` image and recovers its chip key from `isd_config.ini`.
    ///
    /// # Errors
    /// [`FormatError::Invalid`] when the header or an entry record is truncated.
    pub fn parse(data: Vec<u8>) -> Result<Self, FormatError> {
        let mut image = Self::with_key(data, ENC_DEFAULT_KEY, Format::Ufw)?;
        image.chipkey = image.find_chipkey();
        Ok(image)
    }

    /// Parses a download-tool `.fw` image; its header key is the chip key.
    ///
    /// # Errors
    /// [`FormatError::Invalid`] when no ENC key gives a valid header, or a record is truncated.
    pub fn parse_fw(data: Vec<u8>) -> Result<Self, FormatError> {
        let key = find_header_key(&data)?;
        let mut image = Self::with_key(data, key, Format::Fw)?;
        image.chipkey = Some(key);
        Ok(image)
    }

    /// Parses the header and entry table with `hdr_key`.
    ///
    /// # Errors
    ///
    /// [`FormatError::Invalid`] when the header or the table is truncated.
    fn with_key(data: Vec<u8>, hdr_key: u16, format: Format) -> Result<Self, FormatError> {
        let head = enc(window(&data, 0, HEADER_LEN), hdr_key);
        let field16 = |at| u16_at(&head, at).ok_or_else(|| short("header", head.len()));
        let field32 = |at| u32_at(&head, at).ok_or_else(|| short("header", head.len()));
        let hdr_crc = field16(HDR_CRC_AT)?;
        let (list_crc, size) = (field16(LIST_CRC_AT)?, field32(SIZE_AT)?);
        let (count, f0a, f0c) = (field16(COUNT_AT)?, field16(F0A_AT)?, field32(F0C_AT)?);
        let chip = ascii_name(window(&head, CHIP_AT, CHIP_LEN));
        let hdr_ok = crc16(tail(&head, HDR_CRC_COVERS)) == hdr_crc;
        let table = window(
            &data,
            HEADER_LEN,
            usize::from(count).saturating_mul(ENTRY_LEN),
        );
        let list_ok = crc16(table) == list_crc;
        let entries = table
            .chunks(ENTRY_LEN)
            .map(|raw| Entry::parse(raw, hdr_key))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            data,
            format,
            hdr_key,
            hdr_crc,
            list_crc,
            size,
            count,
            f0a,
            f0c,
            chip,
            hdr_ok,
            list_ok,
            entries,
            chipkey: None,
        })
    }

    /// The first entry called `name`.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// The stored bytes of `entry`, clamped to the file.
    #[must_use]
    pub fn raw(&self, entry: &Entry) -> &[u8] {
        window(&self.data, index(entry.offset), index(entry.size))
    }

    fn find_chipkey(&self) -> Option<u16> {
        let isd = self
            .entries
            .iter()
            .find(|e| e.kind == ISD_CONFIG_TYPE && index(e.size) >= ISD_PLAINTEXT_HEAD.len())?;
        let first = window(self.raw(isd), 0, ISD_PLAINTEXT_HEAD.len());
        // Block 0's key is chipkey ^ (offset >> 2).
        (0..=u16::MAX)
            .find(|&k| enc(first, k) == ISD_PLAINTEXT_HEAD)
            .map(|k| k ^ low16(index(isd.offset).wrapping_shr(SFC_OFFSET_SHIFT)))
    }

    /// The body of `entry` in plaintext. `.ufw`: stored bytes for flash images (decrypted on the
    /// chip), SFC-decrypted otherwise. `.fw`: only `isd_config.ini` is SFC-decrypted.
    #[must_use]
    pub fn plain(&self, entry: &Entry) -> Vec<u8> {
        let body = self.raw(entry);
        match (self.format, self.chipkey) {
            (Format::Fw, Some(key)) if entry.kind == ISD_CONFIG_TYPE => sfc(body, key, 0),
            (Format::Ufw, Some(key))
                if !FLASH_TYPES.contains(&entry.kind) && crc16(body) != entry.crc =>
            {
                sfc(body, key, index(entry.offset))
            }
            _ => body.to_vec(),
        }
    }

    /// The entry's CRC check: over the plaintext (`.ufw`) or the padded stored bytes (`.fw`).
    #[must_use]
    pub fn entry_ok(&self, entry: &Entry) -> bool {
        match self.format {
            Format::Ufw => crc16(&self.plain(entry)) == entry.crc,
            Format::Fw => {
                crc16(window(&self.data, index(entry.offset), index(entry.padded))) == entry.crc
            }
        }
    }
}

/// Header ENC key: header CRC passes and `image_size` equals the file size (0xFFFF tried
/// first).
///
/// # Errors
/// [`FormatError::Invalid`] when the file is shorter than 8 bytes or no key fits.
pub fn find_header_key(data: &[u8]) -> Result<u16, FormatError> {
    for key in core::iter::once(ENC_DEFAULT_KEY).chain(0..ENC_DEFAULT_KEY) {
        let size = u32_at(&enc(window(data, 0, SIZE_AT + 4), key), SIZE_AT)
            .ok_or_else(|| short("file", data.len()))?;
        if index(size) != data.len() {
            continue;
        }
        let head = enc(window(data, 0, HEADER_LEN), key);
        if u16_at(&head, HDR_CRC_AT) == Some(crc16(tail(&head, HDR_CRC_COVERS))) {
            return Ok(key);
        }
    }
    Err(FormatError::Invalid(
        "no ENC key gives a valid header".to_owned(),
    ))
}

/// Reads `path` as `.fw` when its suffix is `.fw` (any case), else as `.ufw`.
///
/// # Errors
/// [`FormatError::Io`] when the file cannot be read; parse errors as in [`Ufw::parse`].
pub fn load(path: &Path) -> Result<Ufw, FormatError> {
    let data = fs::read(path).map_err(|e| FormatError::io(path, e))?;
    if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("fw"))
    {
        Ufw::parse_fw(data)
    } else {
        Ufw::parse(data)
    }
}

/// 32-byte JLFS file entry: `hdr_crc`, `data_crc`, offset, size, attr, reserved, index,
/// `name[16]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JlfsEntry {
    /// Stored CRC of entry bytes 2..32.
    pub hdr_crc: u16,
    /// CRC of the file data.
    pub crc: u16,
    /// Offset of the file data.
    pub offset: u32,
    /// Length of the file data.
    pub size: u32,
    /// Attribute byte.
    pub attr: u8,
    /// Index.
    pub index: u16,
    /// Name, up to the first NUL.
    pub name: String,
    /// The entry CRC matches.
    pub ok: bool,
}

impl JlfsEntry {
    /// Reads a descrambled 32-byte entry; `None` when `plain` is shorter than 16 bytes.
    #[must_use]
    pub fn parse(plain: &[u8]) -> Option<Self> {
        let hdr_crc = u16_at(plain, JLFS_HDR_CRC_AT)?;
        Some(Self {
            hdr_crc,
            crc: u16_at(plain, JLFS_CRC_AT)?,
            offset: u32_at(plain, JLFS_OFFSET_AT)?,
            size: u32_at(plain, JLFS_SIZE_AT)?,
            attr: *plain.get(JLFS_ATTR_AT)?,
            index: u16_at(plain, JLFS_INDEX_AT)?,
            name: ascii_name(window(plain, JLFS_NAME_AT, JLFS_NAME_LEN)),
            ok: crc16(window(plain, JLFS_CRC_COVERS, JLFS_LEN - JLFS_CRC_COVERS)) == hdr_crc,
        })
    }
}

/// The JLFS entries from `start`, 32 bytes each, up to the first one whose CRC fails.
/// `decode(bytes, offset)` descrambles the entry found at `offset` in `image`.
pub fn jlfs_list(
    image: &[u8],
    start: usize,
    decode: impl Fn(&[u8], usize) -> Vec<u8>,
) -> Vec<JlfsEntry> {
    (start..image.len().saturating_sub(JLFS_LEN - 1))
        .step_by(JLFS_LEN)
        .map_while(|off| JlfsEntry::parse(&decode(window(image, off, JLFS_LEN), off)))
        .take_while(|e| e.ok)
        .collect()
}

/// `app.bin` from the first flash image (type 0): the top table is ENC 0xFFFF at 0x20;
/// `app_dir_head`'s table and files are SFC-keyed with the chip key, offsets relative to the
/// directory start.
///
/// # Errors
/// [`FormatError::Invalid`] when a piece is missing or the `app.bin` CRC does not match.
pub fn app_bin(image: &Ufw) -> Result<Vec<u8>, FormatError> {
    let missing = |what: &str| FormatError::Invalid(format!("no {what}"));
    let flash_entry = image
        .entries
        .iter()
        .find(|e| e.kind == APP_FLASH_TYPE)
        .ok_or_else(|| missing("flash image entry (type 0)"))?;
    let key = image.chipkey.ok_or_else(|| missing("chip key"))?;
    let flash = image.raw(flash_entry);
    let top = jlfs_list(flash, TOP_TABLE_AT, |b, _| enc(b, ENC_DEFAULT_KEY));
    let base = top
        .iter()
        .find(|e| e.name == "app_dir_head")
        .map(|e| index(e.offset))
        .ok_or_else(|| missing("app_dir_head in the flash directory"))?;
    let sub = jlfs_list(tail(flash, base), 0, |b, off| sfc(b, key, off));
    let app = sub
        .iter()
        .find(|e| e.name == "app.bin")
        .ok_or_else(|| missing("app.bin under app_dir_head"))?;
    let at = base.saturating_add(index(app.offset));
    let body = sfc(window(flash, at, index(app.size)), key, index(app.offset));
    let got = crc16(&body);
    if got != app.crc {
        return Err(FormatError::Invalid(format!(
            "app.bin CRC {got:04x} != stored {:04x}",
            app.crc
        )));
    }
    Ok(body)
}

/// One entry line of [`Info`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryInfo {
    /// The entry.
    pub entry: Entry,
    /// Its CRC check passed.
    pub ok: bool,
}

/// What `gsfw info` shows for an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    /// File name (no directory).
    pub file_name: String,
    /// Chip name.
    pub chip: String,
    /// Image size from the header.
    pub size: u32,
    /// Actual file length.
    pub file_len: usize,
    /// Number of entries in the header.
    pub count: u16,
    /// Header CRC passed.
    pub hdr_ok: bool,
    /// Entry-table CRC passed.
    pub list_ok: bool,
    /// Recovered chip key.
    pub chipkey: Option<u16>,
    /// Each entry with its check result.
    pub entries: Vec<EntryInfo>,
}

impl Info {
    /// Header, table and every entry check out, and the header size is the file size.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.hdr_ok
            && self.list_ok
            && index(self.size) == self.file_len
            && self.entries.iter().all(|e| e.ok)
    }
}

const fn ok_bad(ok: bool) -> &'static str {
    if ok { "ok" } else { "BAD" }
}

impl fmt::Display for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self
            .chipkey
            .map_or_else(|| "none found".to_owned(), |k| format!("{k:#06x}"));
        write!(
            f,
            "{}: chip {}, size {} (file {}), {} entries, header crc {}, table crc {}, chip key {key}",
            self.file_name,
            self.chip,
            self.size,
            self.file_len,
            self.count,
            ok_bad(self.hdr_ok),
            ok_bad(self.list_ok),
        )?;
        for EntryInfo { entry: e, ok } in &self.entries {
            write!(
                f,
                "\n  {:2} type {:#04x} off {:#08x} size {:#07x} crc {:04x} {}  {}",
                e.index,
                e.kind,
                e.offset,
                e.size,
                e.crc,
                ok_bad(*ok),
                e.name
            )?;
        }
        Ok(())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Loads `path` and checks its header, table and entries.
///
/// # Errors
/// As [`load`].
pub fn info(path: &Path) -> Result<Info, FormatError> {
    let image = load(path)?;
    let entries = image
        .entries
        .iter()
        .map(|e| EntryInfo {
            ok: image.entry_ok(e),
            entry: e.clone(),
        })
        .collect();
    Ok(Info {
        file_name: file_name(path),
        chip: image.chip,
        size: image.size,
        file_len: image.data.len(),
        count: image.count,
        hdr_ok: image.hdr_ok,
        list_ok: image.list_ok,
        chipkey: image.chipkey,
        entries,
    })
}

/// Python `PurePath(name).name`: the last non-empty component other than `.`.
fn base_name(name: &str) -> &str {
    name.split('/')
        .rfind(|part| !part.is_empty() && *part != ".")
        .unwrap_or_default()
}

/// Writes each entry's plaintext to `out/NN_name`; returns the written paths in table order.
///
/// # Errors
/// As [`load`], and [`FormatError::Io`] when a file cannot be written.
pub fn extract(path: &Path, out: &Path) -> Result<Vec<PathBuf>, FormatError> {
    let image = load(path)?;
    fs::create_dir_all(out).map_err(|e| FormatError::io(out, e))?;
    image
        .entries
        .iter()
        .map(|e| {
            let name = match base_name(&e.name) {
                "" => "unnamed",
                name => name,
            };
            let dest = out.join(format!("{:02}_{name}", e.index));
            fs::write(&dest, image.plain(e)).map_err(|err| FormatError::io(&dest, err))?;
            Ok(dest)
        })
        .collect()
}

/// USB identifiers found in an image by [`find_usb`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbMatches {
    /// `(pid, offset)` of each `GameSir` VID/PID pair found (first occurrence).
    pub ids: Vec<(u16, usize)>,
    /// `(file name, offset)` of each descriptor file found verbatim.
    pub descriptors: Vec<(String, usize)>,
}

impl UsbMatches {
    /// At least one VID/PID pair and one descriptor were found.
    #[must_use]
    pub const fn found(&self) -> bool {
        !self.ids.is_empty() && !self.descriptors.is_empty()
    }
}

impl fmt::Display for UsbMatches {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ids = self
            .ids
            .iter()
            .map(|(pid, at)| format!("VID 3537 PID {pid:04x} at {at:#x}"));
        let descriptors = self
            .descriptors
            .iter()
            .map(|(name, at)| format!("descriptor {name} at {at:#x}"));
        f.write_str(&ids.chain(descriptors).collect::<Vec<_>>().join("\n"))
    }
}

/// `GameSir` VID.
pub const GAMESIR_VID: u16 = 0x3537;
/// G7 SE PIDs searched by [`find_usb`], in order.
pub const G7SE_PIDS: [u16; 3] = [0x1082, 0x1022, 0x1010];

/// Searches `image` for the `GameSir` VID/PID pairs and for every file in `descdir` (sorted by
/// name) verbatim.
///
/// # Errors
/// [`FormatError::Io`] when the image, the directory or a descriptor cannot be read.
pub fn find_usb(image: &Path, descdir: &Path) -> Result<UsbMatches, FormatError> {
    let data = fs::read(image).map_err(|e| FormatError::io(image, e))?;
    let ids = G7SE_PIDS
        .iter()
        .filter_map(|&pid| {
            let [v0, v1] = GAMESIR_VID.to_le_bytes();
            let [p0, p1] = pid.to_le_bytes();
            find(&data, &[v0, v1, p0, p1]).map(|at| (pid, at))
        })
        .collect();
    let mut files = fs::read_dir(descdir)
        .map_err(|e| FormatError::io(descdir, e))?
        .map(|d| d.map(|d| d.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| FormatError::io(descdir, e))?;
    files.sort();
    let mut descriptors = Vec::new();
    for file in files.iter().filter(|f| f.is_file()) {
        let bytes = fs::read(file).map_err(|e| FormatError::io(file, e))?;
        if let Some(at) = find(&data, &bytes) {
            descriptors.push((file_name(file), at));
        }
    }
    Ok(UsbMatches { ids, descriptors })
}

#[cfg(test)]
mod tests;
