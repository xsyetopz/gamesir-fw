//! The 512-byte JSUD packet: framing and ENC scrambling (0x24a0, 0x1ab0), reply checks (0x4180).

use std::time::{SystemTime, UNIX_EPOCH};

use super::ProtocolError;
use crate::bytes::{hex, put, u16_at, u32_at, window};
use crate::formats::{crc16, enc};

/// Length of every packet.
pub const PACKET_LEN: usize = 0x200;
/// Packet magic (0x24a0).
pub const MAGIC: [u8; 4] = *b"JSUD";
/// Tool id (.data 0x1d050); C0 (0x27c0) scrambles with it.
pub const TOOL_ID: u16 = 0x1011;
/// SDK id (.data 0x1d054).
pub const SDK_ID: [u8; 4] = *b"HJX1";

/// A fresh packet tag. 0x24a0 stamps it from `GetSystemTimeAsFileTime`; any `u32` works, the
/// reply echoes it. This one is the Unix time in milliseconds, truncated.
#[must_use]
pub fn new_tag() -> u32 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let [a, b, c, d, ..] = millis.to_le_bytes();
    u32::from_le_bytes([a, b, c, d])
}

/// Frames `body` (command byte first, lands at 0x10) and ENC-scrambles it with `key`.
/// `tag` defaults to [`new_tag`].
///
/// # Errors
/// [`ProtocolError::Invalid`] when the body does not fit a packet.
pub fn build(body: &[u8], key: u16, tag: Option<u32>) -> Result<Vec<u8>, ProtocolError> {
    let len = u16::try_from(body.len())
        .ok()
        .filter(|&n| usize::from(n) <= PACKET_LEN - 0x10)
        .ok_or_else(|| {
            ProtocolError::Invalid(format!("body {} bytes does not fit a packet", body.len()))
        })?;
    let mut pkt = vec![0; PACKET_LEN];
    put(&mut pkt, 0x10, body);
    put(&mut pkt, 6, &1_u16.to_le_bytes());
    put(&mut pkt, 8, &crc16(body).to_le_bytes());
    put(&mut pkt, 10, &len.to_le_bytes());
    put(&mut pkt, 12, &tag.unwrap_or_else(new_tag).to_le_bytes());
    put(&mut pkt, 0, &MAGIC);
    let head_crc = crc16(window(&pkt, 6, 10));
    put(&mut pkt, 4, &head_crc.to_le_bytes());
    Ok(enc(&pkt, key))
}

fn bad(message: impl Into<String>) -> ProtocolError {
    ProtocolError::BadPacket(message.into())
}

/// Descrambles a reply and runs the 0x4180 checks: magic, header CRC, version 1, body CRC.
///
/// # Errors
/// [`ProtocolError::BadPacket`] naming the first check that fails.
pub fn unscramble(pkt: &[u8], key: u16) -> Result<Vec<u8>, ProtocolError> {
    if pkt.len() < PACKET_LEN {
        return Err(bad(format!("{} bytes, need {PACKET_LEN}", pkt.len())));
    }
    let plain = enc(window(pkt, 0, PACKET_LEN), key);
    let magic = window(&plain, 0, 4);
    if magic != MAGIC {
        return Err(bad(format!(
            "magic {} (wrong session key?)",
            hex(magic, "")
        )));
    }
    let header_ok = u16_at(&plain, 4) == Some(crc16(window(&plain, 6, 10)));
    if !header_ok || u16_at(&plain, 6) != Some(1) {
        return Err(bad("header CRC or version"));
    }
    let blen = u16_at(&plain, 10).map_or(usize::MAX, usize::from);
    let body_ok =
        blen <= PACKET_LEN - 0x10 && u16_at(&plain, 8) == Some(crc16(window(&plain, 0x10, blen)));
    if !body_ok {
        return Err(bad("body CRC"));
    }
    Ok(plain)
}

/// The ENC key under which `pkt` passes every [`unscramble`] check. The G7 SE (6.64) answered a
/// C0 scrambled with [`TOOL_ID`] using another key, so reply keys are found from the packet
/// instead of assumed.
#[must_use]
pub fn find_key(pkt: &[u8]) -> Option<u16> {
    (0..=u16::MAX)
        .filter(|&k| enc(window(pkt, 0, 4), k) == MAGIC)
        .find(|&k| unscramble(pkt, k).is_ok())
}

/// The tag (`u32` at 12) of a descrambled packet.
///
/// # Errors
/// [`ProtocolError::BadPacket`] when `plain` is shorter than 16 bytes.
pub fn tag_of(plain: &[u8]) -> Result<u32, ProtocolError> {
    u32_at(plain, 12).ok_or_else(|| bad("packet too short for a tag"))
}

#[cfg(test)]
mod tests;
