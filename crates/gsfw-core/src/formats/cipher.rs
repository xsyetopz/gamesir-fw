//! `JieLi` ciphers and checksum: ENC keystream, SFC block cipher, CRC-16/XMODEM.

use crate::bytes::{low16, window};

/// ENC key of `.ufw` headers and entry tables, and of `_No_Key` images.
pub const ENC_DEFAULT_KEY: u16 = 0xFFFF;

/// Polynomial of the LFSR behind ENC and CRC-16/XMODEM.
const POLY: u16 = 0x1021;
/// The top bit of the LFSR, the bit that the shift pushes out.
const TOP_BIT: u16 = 0x8000;
/// Length of one SFC block.
const SFC_BLOCK: usize = 32;
/// The SFC block key uses the absolute offset shifted right by this count.
pub(crate) const SFC_OFFSET_SHIFT: u32 = 2;

/// One step of the 16-bit LFSR (poly 0x1021) behind ENC and CRC-16/XMODEM.
const fn step(value: u16) -> u16 {
    let shifted = value.wrapping_shl(1);
    if value & TOP_BIT == 0 {
        shifted
    } else {
        shifted ^ POLY
    }
}

/// `JieLi` ENC: XOR with the low byte of a 16-bit LFSR (poly 0x1021). Applying it twice with the
/// same key gives the input back.
#[must_use]
pub fn enc(data: &[u8], key: u16) -> Vec<u8> {
    let mut state = key;
    data.iter()
        .map(|&b| {
            let [lo, _] = state.to_le_bytes();
            state = step(state);
            b ^ lo
        })
        .collect()
}

/// `JieLi` SFC (`JL_Upgrade_Gip` 0x18f0): 32-byte ENC blocks, each keyed with
/// `key ^ (absolute offset >> 2)`. `base` is the offset of `data[0]`; only the low bits of the
/// offset reach the key, so the offset arithmetic wraps.
#[must_use]
pub fn sfc(data: &[u8], key: u16, base: usize) -> Vec<u8> {
    (0..data.len())
        .step_by(SFC_BLOCK)
        .flat_map(|at| {
            let block_key = key ^ low16(base.wrapping_add(at).wrapping_shr(SFC_OFFSET_SHIFT));
            enc(window(data, at, SFC_BLOCK), block_key)
        })
        .collect()
}

/// CRC-16/XMODEM: poly 0x1021, init 0, not reflected.
#[must_use]
pub fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0, |crc, &b| {
        (0..u8::BITS).fold(crc ^ u16::from(b).wrapping_shl(u8::BITS), |c, _| step(c))
    })
}

#[cfg(test)]
mod tests;
