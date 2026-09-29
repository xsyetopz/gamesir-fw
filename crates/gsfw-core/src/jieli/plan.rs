//! Flash planning: 0x14a20 main sequence, 0x140c0 region write, 0x12900 erase, 0x12c10 write.

use super::ProtocolError;
use super::flash_head::{FlashHead, parse_flash_head};
use crate::bytes::{index, tail, window};

/// EOFFSET (reply byte 0x11b) and how far it moves the device dir addresses (0x14330).
pub const EOFFSET_SHIFT: [(u8, u32); 2] = [(1, 0x1000), (0x10, 0x10000)];
/// 32 zero bytes (`DAT_1800195c0`) written over the other bank's dir head.
pub const KILL_LEN: usize = 0x20;
/// Bytes per C4 write (0x12c10).
const WRITE_CHUNK: usize = 0x100;
/// The two directory entries a plan places.
const DIR_NAMES: [&str; 2] = ["app_dir_head", "app_dir_head2"];

/// The dir address shift for `eoffset`, if the DLL knows it.
#[must_use]
pub const fn eoffset_shift(eoffset: u8) -> Option<u32> {
    match eoffset {
        1 => Some(EOFFSET_SHIFT[0].1),
        0x10 => Some(EOFFSET_SHIFT[1].1),
        _ => None,
    }
}

/// Mode 1 or 2 with flag 0 and region A already matching: region C goes into the bank the pad
/// is not running, then the running bank's first 32 bytes are zeroed, then C2. Region A (boot
/// loader and `isd_config`) is only checked, never written here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashPlan {
    /// Device mode (1 or 2).
    pub mode: u8,
    /// Device erase unit, header byte 13 `<< 8`.
    pub unit: u32,
    /// What the pad should hold at 0: `0xFF * shift + flash[:dh]` (EOFFSET prepend).
    pub region_a: Vec<u8>,
    /// `flash[dh:]`.
    pub region_c: Vec<u8>,
    /// Where region C goes.
    pub c_addr: u32,
    /// Where the [`KILL_LEN`] zero bytes go.
    pub kill_addr: u32,
}

const fn invalid(message: String) -> ProtocolError {
    ProtocolError::Invalid(message)
}

/// The erase unit for header byte `byte` (`byte << 8`).
const fn unit_of(byte: u8) -> u32 {
    u32::from_le_bytes([0, byte, 0, 0])
}

/// Python `repr(str)` for the ASCII directory names.
fn py_repr(name: &str) -> String {
    let quote = if name.contains('\'') && !name.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in name.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_ascii_control() => {
                let escaped = format!("\\x{:02x}", u32::from(c));
                out.push_str(&escaped);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python `repr({e.name: e.addr for e in head.entries})`: first-seen order, last value wins.
fn dir_repr(head: &FlashHead) -> String {
    let mut names: Vec<&str> = Vec::new();
    for entry in &head.entries {
        if !names.contains(&entry.name.as_str()) {
            names.push(&entry.name);
        }
    }
    let items: Vec<String> = names
        .iter()
        .map(|&n| format!("{}: {}", py_repr(n), head.addr(n).unwrap_or_default()))
        .collect();
    format!("{{{}}}", items.join(", "))
}

/// `addr` as a device address.
///
/// # Errors
///
/// [`ProtocolError::Invalid`] when it passes 32 bits.
fn addr32(addr: u64) -> Result<u32, ProtocolError> {
    u32::try_from(addr).map_err(|_| invalid(format!("address {addr:#x} passes 32 bits")))
}

/// Checks that `flash` (a `flash*.bin` entry, raw) has the device's layout and places region C.
///
/// # Errors
/// [`ProtocolError::Invalid`] when a head CRC fails, the erase unit or directory differs from
/// the device's, the device EOFFSET or mode is not supported, region C does not fit, or an
/// address passes 32 bits. [`ProtocolError::BadPacket`] when `flash` is too short for a head.
pub fn plan_flash(flash: &[u8], dev: &FlashHead) -> Result<FlashPlan, ProtocolError> {
    let img = parse_flash_head(window(flash, 0, 0x100), 0, 0)?;
    if !img.header_ok || !dev.header_ok {
        return Err(invalid("flash head CRC fails".to_owned()));
    }
    if img.unit_byte() != dev.unit_byte() {
        return Err(invalid(format!(
            "erase unit {:#x}, device {:#x}",
            unit_of(img.unit_byte()),
            unit_of(dev.unit_byte())
        )));
    }
    let [Some(dh), Some(dh2)] = DIR_NAMES.map(|n| dev.addr(n).filter(|&a| img.addr(n) == Some(a)))
    else {
        return Err(invalid(format!(
            "directory {} does not match the device {}",
            dir_repr(&img),
            dir_repr(dev)
        )));
    };
    let shift = eoffset_shift(dev.eoffset)
        .ok_or_else(|| invalid(format!("device EOFFSET {:#x} not supported", dev.eoffset)))?;
    let (dirhead, dirhead2) = (
        u64::from(dh).saturating_add(u64::from(shift)),
        u64::from(dh2).saturating_add(u64::from(shift)),
    );
    let region_c = tail(flash, index(dh));
    let (c_addr, kill_addr, limit) = match dev.mode {
        1 => (dirhead2, dirhead, u64::from(dev.size)),
        2 => (dirhead, dirhead2, dirhead2),
        mode => {
            return Err(invalid(format!(
                "device mode {mode}: the DLL writes nothing"
            )));
        }
    };
    let c_len = u64::try_from(region_c.len()).unwrap_or(u64::MAX);
    if c_addr.saturating_add(c_len) > limit {
        return Err(invalid(format!(
            "region C {c_len:#x} at {c_addr:#x} passes {limit:#x}"
        )));
    }
    let mut region_a = vec![0xFF; index(shift)];
    region_a.extend_from_slice(window(flash, 0, index(dh)));
    Ok(FlashPlan {
        mode: dev.mode,
        unit: unit_of(dev.unit_byte()),
        region_a,
        region_c: region_c.to_vec(),
        c_addr: addr32(c_addr)?,
        kill_addr: addr32(kill_addr)?,
    })
}

/// `(address, size)` of each C3 (0x12900): 64K where 64K-aligned with 64K left, else 4K where
/// 4K-aligned with 4K left, else the device unit.
///
/// # Errors
/// [`ProtocolError::Invalid`] when `unit` is 0 (the DLL loop would never end) or an address
/// passes 32 bits.
pub fn erase_steps(addr: u32, length: u32, unit: u32) -> Result<Vec<(u32, u32)>, ProtocolError> {
    if unit == 0 {
        return Err(invalid("erase unit 0".to_owned()));
    }
    let mut steps = Vec::new();
    let mut off: u32 = 0;
    while off < length {
        let at = addr
            .checked_add(off)
            .ok_or_else(|| invalid(format!("address {addr:#x} + {off:#x} passes 32 bits")))?;
        let left = length.saturating_sub(off);
        let size = if at.trailing_zeros() >= 16 && left >= 0x10000 {
            0x10000
        } else if at.trailing_zeros() >= 12 && left >= 0x1000 {
            0x1000
        } else {
            unit
        };
        steps.push((at, size));
        // Past u32::MAX is past any u32 length, so the loop is done.
        let Some(next) = off.checked_add(size) else {
            break;
        };
        off = next;
    }
    Ok(steps)
}

/// C4 chunks of 0x100 bytes (0x12c10), the first chunk last so the bank's dir head lands after
/// the rest of the region.
///
/// # Errors
/// [`ProtocolError::Invalid`] when a chunk address passes 32 bits.
pub fn write_chunks(addr: u32, data: &[u8]) -> Result<Vec<(u32, &[u8])>, ProtocolError> {
    let mut chunks = data
        .chunks(WRITE_CHUNK)
        .zip((0..).step_by(WRITE_CHUNK))
        .map(|(chunk, off)| {
            u32::try_from(off)
                .ok()
                .and_then(|o| addr.checked_add(o))
                .map(|at| (at, chunk))
                .ok_or_else(|| invalid(format!("address {addr:#x} + {off:#x} passes 32 bits")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !chunks.is_empty() {
        chunks.rotate_left(1);
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests;
