//! Device flash header and directory (0x12690 decode): the C1 reply and the start of
//! `flash.bin` share this layout.

use super::ProtocolError;
use super::plan::eoffset_shift;
use crate::bytes::{u16_at, u32_at, window};
use crate::formats::{crc16, enc};

/// ENC key of the device flash header and its directory entries (0x12690 decode).
pub const FLASH_HEAD_KEY: u16 = 0xFFFF;

/// Bytes the header must hold: [`FlashHead::size`] at 8 and the erase unit byte at 13.
const HEADER_MIN: usize = 14;

/// One flash directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// The NUL-terminated ASCII name at +16.
    pub name: String,
    /// The `u32` at +8; 0x12690 takes it for `app_dir_head` and `app_dir_head2`.
    pub addr: u32,
    /// The descrambled 32-byte entry.
    pub raw: Vec<u8>,
}

/// Device flash header from the C1 reply (0x12690): a 32-byte header at 0x18, then seven
/// 32-byte directory entries, all ENC-scrambled with [`FLASH_HEAD_KEY`]. `flash.bin` starts
/// the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashHead {
    /// The descrambled header (32 bytes, fewer if the input was short).
    pub header: Vec<u8>,
    /// Whether the `u16` at 0 is the CRC of the rest of the header.
    pub header_ok: bool,
    /// The `u32` at header +8, 0x7ff00 in the G7 SE 6.64 `flash.bin`.
    pub size: u32,
    /// The directory entries with a printable name.
    pub entries: Vec<DirEntry>,
    /// Reply byte 0x11a.
    pub mode: u8,
    /// Reply byte 0x11b: 1 moves the app dir by 0x1000, 0x10 by 0x10000.
    pub eoffset: u8,
}

impl FlashHead {
    /// The address of the entry called `name`; the last one wins when names repeat.
    #[must_use]
    pub fn addr(&self, name: &str) -> Option<u32> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.name == name)
            .map(|e| e.addr)
    }

    /// The erase unit byte (header byte 13); the device unit is this `<< 8`.
    pub(super) fn unit_byte(&self) -> u8 {
        self.header.get(13).copied().unwrap_or_default()
    }

    /// `app_dir_head` and `app_dir_head2`, moved by the EOFFSET shift (none for an unknown
    /// EOFFSET). `None` where the entry is missing or the moved address passes 32 bits.
    #[must_use]
    pub fn app_dirs(&self) -> (Option<u32>, Option<u32>) {
        let shift = eoffset_shift(self.eoffset).unwrap_or(0);
        let moved = |name| self.addr(name)?.checked_add(shift);
        (moved("app_dir_head"), moved("app_dir_head2"))
    }
}

/// Decodes a flash head. `head` is 0x100 bytes: the reply's 0x18..0x118, or the first 0x100
/// bytes of `flash.bin`.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when `head` is too short for the header fields.
pub fn parse_flash_head(head: &[u8], mode: u8, eoffset: u8) -> Result<FlashHead, ProtocolError> {
    let header = enc(window(head, 0, 32), FLASH_HEAD_KEY);
    if header.len() < HEADER_MIN {
        return Err(ProtocolError::BadPacket(format!(
            "flash head {} bytes, need {HEADER_MIN}",
            head.len()
        )));
    }
    let entries = (32..0x100)
        .step_by(32)
        .filter_map(|at| {
            let raw = enc(window(head, at, 32), FLASH_HEAD_KEY);
            let name = window(&raw, 16, 16).split(|&b| b == 0).next()?;
            if name.is_empty() || name.iter().all(|&b| b == 0xFF) || !name.is_ascii() {
                return None;
            }
            Some(DirEntry {
                name: name.iter().copied().map(char::from).collect(),
                addr: u32_at(&raw, 8)?,
                raw,
            })
        })
        .collect();
    Ok(FlashHead {
        header_ok: u16_at(&header, 0) == Some(crc16(window(&header, 2, 30))),
        size: u32_at(&header, 8).unwrap_or_default(),
        header,
        entries,
        mode,
        eoffset,
    })
}

#[cfg(test)]
pub(super) mod tests;
