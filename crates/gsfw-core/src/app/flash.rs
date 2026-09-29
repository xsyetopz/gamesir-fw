//! Flash region C of an image into the bank the pad is not running (`JL_Upgrade_Gip.dll`
//! 0x14a20, mode 1/2 with region A kept). Safety rules: `docs/architecture.md`, D5.

use core::time::Duration;
use std::path::Path;

use super::session::{DeviceError, Session, device_crc, request};
use crate::formats::crc16;
use crate::jieli::{
    FlashPlan, KILL_LEN, build, c2_result, c3_erase, c4_write, erase_steps, parse_result,
    plan_flash, write_chunks,
};

const SETTLE: Duration = Duration::from_secs(1);

/// # Errors
/// [`DeviceError::Pad`] when `data` is 4 GiB or longer.
fn len32(data: &[u8]) -> Result<u32, DeviceError> {
    u32::try_from(data.len())
        .map_err(|_| DeviceError::Pad(format!("region of {:#x} bytes passes 32 bits", data.len())))
}

/// Whether an image entry is a flash layout (`flash.bin`, `flash3.bin`).
#[must_use]
pub fn is_flash_bin(name: &str) -> bool {
    let bin = Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"));
    bin && name.starts_with("flash")
}

/// The first `flash*.bin` entry whose layout fits the device, with its name.
///
/// # Errors
/// [`DeviceError::Pad`] listing why each entry does not fit.
pub fn choose_plan(s: &Session<'_>) -> Result<(String, FlashPlan), DeviceError> {
    let mut why = Vec::new();
    for entry in &s.image.entries {
        if is_flash_bin(&entry.name) {
            match plan_flash(s.image.raw(entry), &s.head) {
                Ok(plan) => return Ok((entry.name.clone(), plan)),
                Err(err) => why.push(format!("{}: {err}", entry.name)),
            }
        }
    }
    Err(DeviceError::Pad(format!(
        "no flash*.bin entry fits the device:\n  {}",
        why.join("\n  ")
    )))
}

/// Erases `length` bytes at `addr` and checks they read back blank.
///
/// # Errors
/// [`DeviceError`] from the link.
fn erase(s: &mut Session<'_>, addr: u32, length: u32, unit: u32) -> Result<bool, DeviceError> {
    let steps = erase_steps(addr, length, unit)?;
    (s.log)(&format!("  erase {} blocks from {addr:#x}", steps.len()));
    for (at, size) in steps {
        s.link.send(&build(&c3_erase(at, size)?, s.key, None)?)?;
    }
    s.link.drain(SETTLE)?;
    let blank = crc16(&vec![0xFF; usize::try_from(length).unwrap_or(usize::MAX)]);
    let ok = device_crc(s, addr, length)? == blank;
    if !ok {
        (s.log)("  blank check failed");
    }
    Ok(ok)
}

/// Writes `data` at `addr` and checks the CRC the pad reads back.
///
/// # Errors
/// [`DeviceError`] from the link.
fn write_verify(s: &mut Session<'_>, addr: u32, data: &[u8]) -> Result<bool, DeviceError> {
    let chunks = write_chunks(addr, data)?;
    let total = chunks.len();
    for (n, (at, chunk)) in (1..).zip(chunks) {
        s.link.send(&build(&c4_write(at, chunk)?, s.key, None)?)?;
        if n % 64 == 0 || n == total {
            (s.log)(&format!("  wrote {n}/{total} chunks"));
        }
    }
    s.link.drain(SETTLE)?;
    let got = device_crc(s, addr, len32(data)?)?;
    let want = crc16(data);
    if got == want {
        (s.log)(&format!(
            "  verified {addr:#x}+{:#x} crc {got:#06x}",
            data.len()
        ));
    } else {
        (s.log)(&format!(
            "  verify failed: crc {got:#06x}, want {want:#06x}"
        ));
    }
    Ok(got == want)
}

/// 0x140c0: erase, blank check, write, verify; the whole region up to 5 times.
///
/// # Errors
/// [`DeviceError`] from the link, or after 5 failed attempts.
pub fn write_region(
    s: &mut Session<'_>,
    addr: u32,
    data: &[u8],
    unit: u32,
) -> Result<(), DeviceError> {
    let length = len32(data)?;
    for attempt in 1..=5 {
        (s.log)(&format!("  try {attempt}"));
        if erase(s, addr, length, unit)? && write_verify(s, addr, data)? {
            return Ok(());
        }
    }
    Err(DeviceError::Pad(format!(
        "region at {addr:#x} failed 5 times; the running bank is untouched"
    )))
}

/// Logs the plan and compares both regions with the device.
///
/// # Errors
/// [`DeviceError`] from the link, or when region A differs without `keep_region_a`.
fn check(
    s: &mut Session<'_>,
    name: &str,
    plan: &FlashPlan,
    keep_region_a: bool,
) -> Result<(), DeviceError> {
    let c_len = len32(&plan.region_c)?;
    let a_len = len32(&plan.region_a)?;
    let c_crc = crc16(&plan.region_c);
    (s.log)(&format!(
        "image entry {name} matches the device layout; mode {}, erase unit {:#x}",
        plan.mode, plan.unit
    ));
    (s.log)(&format!(
        "region C: {c_len:#x} bytes (crc {c_crc:#06x}) to {:#x}; then {KILL_LEN} zero bytes at \
         {:#x} (the running bank); then C2",
        plan.c_addr, plan.kill_addr
    ));
    let have_a = device_crc(s, 0, a_len)?;
    let want_a = crc16(&plan.region_a);
    (s.log)(&format!(
        "region A 0x0+{a_len:#x}: device crc {have_a:#06x}, image crc {want_a:#06x}"
    ));
    if have_a != want_a && !keep_region_a {
        return Err(DeviceError::Pad(
            "region A differs; the DLL would rewrite the boot area, which this tool does not do. \
             Keeping region A (--keep-region-a) writes region C and leaves the device's region A."
                .to_owned(),
        ));
    }
    let have_c = device_crc(s, plan.c_addr, c_len)?;
    let note = if have_c == c_crc {
        " (already this image)"
    } else {
        ""
    };
    (s.log)(&format!("target bank now: crc {have_c:#06x}{note}"));
    Ok(())
}

/// Writes region C, zeroes the running bank's dir head, and sends C2.
///
/// # Errors
/// [`DeviceError`] from the link, or a non-zero C2 status.
fn commit(s: &mut Session<'_>, plan: &FlashPlan) -> Result<(), DeviceError> {
    write_region(s, plan.c_addr, &plan.region_c, plan.unit)?;
    let zeros = [0_u8; KILL_LEN];
    s.link
        .send(&build(&c4_write(plan.kill_addr, &zeros)?, s.key, None)?)?;
    s.link.drain(SETTLE)?;
    let kill = device_crc(s, plan.kill_addr, len32(&zeros)?)?;
    (s.log)(&format!(
        "zeroed {:#x}: crc {kill:#06x} (want {:#06x})",
        plan.kill_addr,
        crc16(&zeros)
    ));
    let status = parse_result(&request(s.link, &c2_result(), s.key, "C2 reply", s.log, 5)?)?;
    (s.log)(&format!("C2 status {status}"));
    if status != 0 {
        return Err(DeviceError::Pad(format!("C2 status {status}")));
    }
    (s.log)("done; replug the pad");
    Ok(())
}

/// Checks the image against the pad, then (when `write`) writes region C, zeroes the running
/// bank's dir head, and asks for the result (C2).
///
/// # Errors
/// [`DeviceError`] when no entry fits, region A differs without `keep_region_a`, the link
/// fails, or C2 reports a non-zero status.
pub fn flash(s: &mut Session<'_>, write: bool, keep_region_a: bool) -> Result<(), DeviceError> {
    let (name, plan) = choose_plan(s)?;
    check(s, &name, &plan, keep_region_a)?;
    if !write {
        (s.log)("checks only: nothing written");
        return Ok(());
    }
    commit(s, &plan)
}
