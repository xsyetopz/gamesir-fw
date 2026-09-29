//! Command bodies and reply parsers. Each builder returns the body; [`super::build`] frames it.
//! Layouts: the RVA in each doc comment.

use super::ProtocolError;
use super::flash_head::{FlashHead, HEAD_LEN, parse_flash_head};
use super::packet::{BODY_AT, SDK_ID};
use crate::bytes::{hex, i32_at, put, u16_at, u32_at, window};
use crate::formats::crc16;

/// C0: handshake.
pub const C0_HANDSHAKE: u8 = 0xC0;
/// C1: query the flash head.
pub const C1_QUERY: u8 = 0xC1;
/// C2: finish the upgrade.
pub const C2_RESULT: u8 = 0xC2;
/// C3: erase.
pub const C3_ERASE: u8 = 0xC3;
/// C4: write.
pub const C4_WRITE: u8 = 0xC4;
/// C5: read a CRC.
pub const C5_CRC: u8 = 0xC5;
/// CA: reset.
pub const CA_RESET: u8 = 0xCA;
/// CB: range (meaning not read).
pub const CB_RANGE: u8 = 0xCB;
/// Reply type (`u32` at 0x10) that 0x4520 wants.
pub const REPLY_HANDSHAKE: u32 = 1;
/// Reply type (`u32` at 0x10) that 0x4da0 wants.
pub const REPLY_STATUS: u32 = 2;
/// The status of a reply that reports success.
pub const STATUS_OK: i32 = 0;
/// 0x14a20: "The firmware KEY does not macth the device KEY".
pub const STATUS_KEY_MISMATCH: i32 = -3;
/// Length of each random that C0 exchanges.
pub const RAND_LEN: usize = 16;
/// The largest data length of one C4 write (0x12c10).
pub const MAX_WRITE: usize = 0x100;
/// The smallest erase unit.
pub const PAGE: u32 = 0x100;
/// A 4 KiB erase sector.
pub const SECTOR: u32 = 0x1000;
/// A 64 KiB erase block.
pub const BLOCK: u32 = 0x10000;

/// Device erase unit and its C3 code (0x3740).
const ERASE_UNITS: [(u32, u8); 3] = [(PAGE, 1), (SECTOR, 2), (BLOCK, 3)];

/// Body offset of the command fields, after the command byte and three zero bytes.
const FIELDS_AT: usize = 4;
/// Length of a body with fixed fields.
const FIXED_BODY_LEN: usize = 0x18;
/// C4 body offsets of the data CRC, the data length and the data.
const C4_CRC_AT: usize = 0x0A;
const C4_LEN_AT: usize = 0x0C;
const C4_DATA_AT: usize = 0x10;

/// Reply offsets (in the packet) of the reply type and the status.
const REPLY_TYPE_AT: usize = BODY_AT;
const STATUS_AT: usize = BODY_AT + FIELDS_AT;
/// Reply offset of the first field after the status: the C0 SDK id, the C1 flash head and the
/// C5 CRC.
const REPLY_DATA_AT: usize = STATUS_AT + 4;
/// C0 reply offset of the pad's random.
const PAD_RAND_AT: usize = 0x1C;
/// Status reply offsets of [`Status::word`] and [`Status::extra`].
const WORD_AT: usize = 0x1C;
const EXTRA_AT: usize = 0x28;
/// Length of [`Status::extra`].
const EXTRA_LEN: usize = 16;
/// C1 reply offsets of the mode and EOFFSET bytes, after the flash head.
const MODE_AT: usize = REPLY_DATA_AT + HEAD_LEN + 2;
const EOFFSET_AT: usize = MODE_AT + 1;

const fn bad(message: String) -> ProtocolError {
    ProtocolError::BadPacket(message)
}

fn short() -> ProtocolError {
    bad("reply too short".to_owned())
}

/// Bodies with a fixed 0x18 length: cmd at 0x10, fields from 0x14, zeros to 0x28.
fn fixed(cmd: u8, fields: &[u8]) -> Vec<u8> {
    let mut body = vec![0; FIXED_BODY_LEN];
    put(&mut body, 0, &[cmd]);
    put(&mut body, FIELDS_AT, fields);
    body
}

/// 0x27c0: SDK id at 0x14, 16 host random bytes at 0x18. Scramble with [`super::TOOL_ID`].
#[must_use]
pub fn c0_handshake(host_rand: &[u8; RAND_LEN]) -> Vec<u8> {
    fixed(C0_HANDSHAKE, &[&SDK_ID[..], host_rand].concat())
}

/// 0x4520: the C0 reply has `u32` 1 at 0x10 and the SDK id at 0x18; the key mixes both
/// randoms.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when `reply_plain` is not a handshake reply.
pub fn session_key(host_rand: &[u8; RAND_LEN], reply_plain: &[u8]) -> Result<u16, ProtocolError> {
    let kind = u32_at(reply_plain, REPLY_TYPE_AT).ok_or_else(short)?;
    let id = window(reply_plain, REPLY_DATA_AT, SDK_ID.len());
    if kind != REPLY_HANDSHAKE || id != SDK_ID {
        return Err(bad(format!(
            "not a handshake reply (type {kind}, id {})",
            hex(id, "")
        )));
    }
    Ok(crc16(host_rand) ^ crc16(window(reply_plain, PAD_RAND_AT, RAND_LEN)))
}

/// 0x31f0: `u16` chip key at 0x14 (from the image's `isd_config` key).
#[must_use]
pub fn c1_query(chipkey: u16) -> Vec<u8> {
    fixed(C1_QUERY, &chipkey.to_le_bytes())
}

/// 0x3ad0.
#[must_use]
pub fn c2_result() -> Vec<u8> {
    fixed(C2_RESULT, &[])
}

/// 0x3740: `u32` address at 0x14, unit code at 0x18 (1 = 0x100, 2 = 0x1000, 3 = 0x10000).
///
/// # Errors
/// [`ProtocolError::Invalid`] when `unit` is not one of the three erase units.
pub fn c3_erase(addr: u32, unit: u32) -> Result<Vec<u8>, ProtocolError> {
    let (_, code) = ERASE_UNITS
        .iter()
        .find(|&&(size, _)| size == unit)
        .ok_or_else(|| ProtocolError::Invalid(format!("erase unit {unit:#x} not supported")))?;
    Ok(fixed(
        C3_ERASE,
        &[&addr.to_le_bytes()[..], &[*code]].concat(),
    ))
}

/// 0x3540: `u32` address at 0x14, `crc16(data)` at 0x1a, `u32` length at 0x1c, data at 0x20;
/// the body is `len + 0x18` long, so 8 zero bytes follow the data.
///
/// # Errors
/// [`ProtocolError::Invalid`] unless `data` is 1..=256 bytes.
pub fn c4_write(addr: u32, data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let len = u32::try_from(data.len())
        .ok()
        .filter(|_| (1..=MAX_WRITE).contains(&data.len()))
        .ok_or_else(|| ProtocolError::Invalid("write chunk must be 1..256 bytes".to_owned()))?;
    let mut body = vec![0; data.len().saturating_add(FIXED_BODY_LEN)];
    put(&mut body, 0, &[C4_WRITE]);
    put(&mut body, FIELDS_AT, &addr.to_le_bytes());
    put(&mut body, C4_CRC_AT, &crc16(data).to_le_bytes());
    put(&mut body, C4_LEN_AT, &len.to_le_bytes());
    put(&mut body, C4_DATA_AT, data);
    Ok(body)
}

/// 0x2e80: `u32` address at 0x14, `u32` length at 0x18.
#[must_use]
pub fn c5_crc(addr: u32, length: u32) -> Vec<u8> {
    fixed(C5_CRC, &[addr.to_le_bytes(), length.to_le_bytes()].concat())
}

/// 0x2b40.
#[must_use]
pub fn ca_reset() -> Vec<u8> {
    fixed(CA_RESET, &[])
}

/// 0x3e10: two `u32` at 0x14 and 0x18 (meaning not read).
#[must_use]
pub fn cb_range(a: u32, b: u32) -> Vec<u8> {
    fixed(CB_RANGE, &[a.to_le_bytes(), b.to_le_bytes()].concat())
}

/// 0x4da0: `u32` 2 at 0x10, `i32` status at 0x14; on status 0, a `u32` at 0x1c and 16 bytes at
/// 0x28.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// Status code (0 ok, [`STATUS_KEY_MISMATCH`]).
    pub code: i32,
    /// The `u32` at 0x1c.
    pub word: u32,
    /// The 16 bytes at 0x28.
    pub extra: [u8; EXTRA_LEN],
}

/// Reads a status reply.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when the reply type is not [`REPLY_STATUS`].
pub fn parse_status(plain: &[u8]) -> Result<Status, ProtocolError> {
    let kind = u32_at(plain, REPLY_TYPE_AT).ok_or_else(short)?;
    let status = i32_at(plain, STATUS_AT).ok_or_else(short)?;
    if kind != REPLY_STATUS {
        return Err(bad(format!("reply type {kind}, want {REPLY_STATUS}")));
    }
    Ok(Status {
        code: status,
        word: u32_at(plain, WORD_AT).ok_or_else(short)?,
        extra: window(plain, EXTRA_AT, EXTRA_LEN)
            .try_into()
            .map_err(|_| short())?,
    })
}

/// C5 reply (all.c ~11080): `(status, crc)`, `i32` at 0x14 and `u16` at 0x18. The DLL does not
/// check the reply type; the G7 SE sends type 4.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when the reply is too short.
pub fn parse_crc(plain: &[u8]) -> Result<(i32, u16), ProtocolError> {
    Ok((
        i32_at(plain, STATUS_AT).ok_or_else(short)?,
        u16_at(plain, REPLY_DATA_AT).ok_or_else(short)?,
    ))
}

/// C1 reply: status ([`STATUS_KEY_MISMATCH`] = chip key mismatch); the flash head is present on
/// status 0.
///
/// # Errors
/// As [`parse_status`] and [`parse_flash_head`].
pub fn parse_c1(plain: &[u8]) -> Result<(Status, Option<FlashHead>), ProtocolError> {
    let status = parse_status(plain)?;
    if status.code != STATUS_OK {
        return Ok((status, None));
    }
    let mode = *plain.get(MODE_AT).ok_or_else(short)?;
    let eoffset = *plain.get(EOFFSET_AT).ok_or_else(short)?;
    let head = parse_flash_head(window(plain, REPLY_DATA_AT, HEAD_LEN), mode, eoffset)?;
    Ok((status, Some(head)))
}

/// C2 reply status, `i32` at 0x14 (0x13610); the reply type is not checked, as for C5.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when the reply is too short.
pub fn parse_result(plain: &[u8]) -> Result<i32, ProtocolError> {
    i32_at(plain, STATUS_AT).ok_or_else(short)
}

#[cfg(test)]
mod tests;
