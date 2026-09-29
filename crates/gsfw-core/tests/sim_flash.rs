//! The flash use case against a simulated G7 SE after the Nexus 6.64 update (C1 reports
//! `flash3.bin`'s head, mode 2, eoffset 1), writing the 6.40 image. Needs the images under
//! `private/`; skips without them.
#![cfg(test)]

use core::time::Duration;
use std::path::{Path, PathBuf};

use gsfw_core::app::{DeviceError, GipLink, device_crc, flash, open_session};
use gsfw_core::formats::crc16;
use gsfw_core::formats::ufw::{Ufw, load};
use gsfw_core::jieli::{KILL_LEN, TOOL_ID, build, find_key, plan_flash, tag_of, unscramble};

/// The key the real pad scrambles its C0 reply with (`jlgip-c0-reply1.bin`).
const PAD_C0_KEY: u16 = 0x5A5A;
const DEV_RAND: [u8; 16] = *b"0123456789abcdef";

fn fixture(rel: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private")
        .join(rel);
    if path.exists() {
        Some(path)
    } else {
        eprintln!("skipped: {} is missing", path.display());
        None
    }
}

fn entry(image: &Ufw, name: &str) -> Vec<u8> {
    let e = image.entries.iter().find(|e| e.name == name).unwrap();
    image.raw(e).to_vec()
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..][..4].try_into().unwrap())
}

fn index(value: u32) -> usize {
    usize::try_from(value).unwrap()
}

/// The USB link between the host and the simulated pad.
#[derive(Clone, Copy, Debug)]
enum Link {
    Up,
    /// Up for `calls` more `send`, `reply` or `drain` calls; the call after them fails. With
    /// `delivered`, a failing `send` still reaches the pad (the acknowledgement is lost).
    DropAfter {
        calls: usize,
        delivered: bool,
    },
    Down,
}

/// A pad at packet level: NOR flash (writes clear bits), one C0 per power-up, and a status
/// reply to every C3 and C4 that the host never reads.
struct Pad {
    flash: Vec<u8>,
    head: Vec<u8>,
    key: Option<u16>,
    queue: Vec<Vec<u8>>,
    ignore_writes: bool,
    link: Link,
    /// Link calls made so far.
    calls: usize,
    /// The running bank's head has changed.
    killed: bool,
}

impl Pad {
    fn new(head: &[u8]) -> Self {
        Self {
            flash: vec![0x55; 0x8_0000],
            head: head[..0x100].to_vec(),
            key: None,
            queue: Vec::new(),
            ignore_writes: false,
            link: Link::Up,
            calls: 0,
            killed: false,
        }
    }

    /// Counts one link call. Returns whether the pad gets the data, and whether the call
    /// succeeds.
    fn tick(&mut self) -> (bool, bool) {
        self.calls = self.calls.saturating_add(1);
        match self.link {
            Link::Up => (true, true),
            Link::DropAfter {
                calls: 0,
                delivered,
            } => {
                self.link = Link::Down;
                (delivered, false)
            }
            Link::DropAfter { calls, delivered } => {
                self.link = Link::DropAfter {
                    calls: calls.saturating_sub(1),
                    delivered,
                };
                (true, true)
            }
            Link::Down => (false, false),
        }
    }

    fn answer(&mut self, fields: &[u8], key: u16, tag: u32) {
        self.queue.push(build(fields, key, Some(tag)).unwrap());
    }
}

impl GipLink for Pad {
    fn send(&mut self, pkt: &[u8]) -> Result<(), DeviceError> {
        let (delivered, up) = self.tick();
        if delivered {
            self.receive(pkt)?;
        }
        if up {
            Ok(())
        } else {
            Err(DeviceError::Pad("link dropped".to_owned()))
        }
    }

    fn reply(&mut self, _waits: u32) -> Result<Vec<u8>, DeviceError> {
        if !self.tick().1 {
            return Err(DeviceError::Pad("link dropped".to_owned()));
        }
        if self.queue.is_empty() {
            return Err(DeviceError::Pad("no reply packet".to_owned()));
        }
        Ok(self.queue.remove(0))
    }

    fn drain(&mut self, _time: Duration) -> Result<(), DeviceError> {
        if !self.tick().1 {
            return Err(DeviceError::Pad("link dropped".to_owned()));
        }
        self.queue.clear();
        Ok(())
    }
}

impl Pad {
    /// Q1 (`docs/architecture.md`): checks a C3 or C4 of `len` bytes at `addr` before the pad
    /// acts on it. A link drop stops every later packet, so the flash after any drop is a
    /// flash this check has passed: the pad keeps one bootable bank.
    ///
    /// - Only region C and the running bank's head are written.
    /// - The head changes only when region C is complete.
    /// - Region C does not change after the head.
    fn guard(&mut self, addr: usize, len: usize) {
        let end = addr.saturating_add(len);
        let in_c = C_ADDR <= addr && end <= C_ADDR + C_LEN;
        let in_head = KILL_ADDR <= addr && end <= KILL_ADDR + KILL_LEN;
        assert!(
            in_c || in_head,
            "write at {addr:#x}+{len:#x}: outside region C and the head"
        );
        if in_c {
            assert!(
                !self.killed,
                "region C changed after the running bank's head"
            );
        } else {
            let region_c = &self.flash[C_ADDR..][..C_LEN];
            assert_eq!(
                crc16(region_c),
                C_CRC,
                "the running bank's head changed before region C was complete"
            );
            self.killed = true;
        }
    }

    /// Acts on one packet from the host.
    fn receive(&mut self, pkt: &[u8]) -> Result<(), DeviceError> {
        let key = self.key.unwrap_or(TOOL_ID);
        let plain = unscramble(pkt, key)?;
        let tag = tag_of(&plain)?;
        let body = &plain[0x10..];
        let (addr, arg) = (u32_at(body, 4), u32_at(body, 8));
        match body[0] {
            0xC0 => {
                let host = &body[8..0x18];
                let session = crc16(host) ^ crc16(&DEV_RAND);
                let fields = [&1_u32.to_le_bytes()[..], &[0; 4], b"HJX1", &DEV_RAND].concat();
                self.answer(&fields, PAD_C0_KEY, tag);
                self.key = Some(session);
            }
            0xC1 => {
                let mut fields = vec![0; 0x10C];
                fields[..4].copy_from_slice(&2_u32.to_le_bytes());
                fields[8..0x108].copy_from_slice(&self.head);
                fields[0x10A] = 2;
                fields[0x10B] = 1;
                self.answer(&fields, key, tag);
            }
            0xC3 => {
                let unit = match arg & 0xFF {
                    1 => 0x100_usize,
                    2 => 0x1000,
                    _ => 0x1_0000,
                };
                self.guard(index(addr), unit);
                self.flash[index(addr)..][..unit].fill(0xFF);
                self.answer(&[4, 0, 0, 0, 0, 0, 0, 0], key, tag);
            }
            0xC4 => {
                let len = index(u32_at(body, 0xC));
                self.guard(index(addr), len);
                if !self.ignore_writes {
                    for (cell, byte) in self.flash[index(addr)..]
                        .iter_mut()
                        .zip(&body[0x10..][..len])
                    {
                        *cell &= byte;
                    }
                }
                self.answer(&[4, 0, 0, 0, 0, 0, 0, 0], key, tag);
            }
            0xC5 => {
                let crc = crc16(&self.flash[index(addr)..][..index(arg)]);
                let fields = [&4_u32.to_le_bytes()[..], &[0; 4], &crc.to_le_bytes()].concat();
                self.answer(&fields, key, tag);
            }
            0xC2 => self.answer(&[4, 0, 0, 0, 0, 0, 0, 0], key, tag),
            other => panic!("unexpected command {other:#x}"),
        }
        Ok(())
    }
}

fn images() -> Option<(Ufw, Vec<u8>)> {
    let image = load(&fixture("flash-tool/bundle/JS_SL3101_V640_Key.fw")?).unwrap();
    let nexus = load(&fixture(
        "firmware/Core/FirmwarePackages/G7SE/JS_SL3101_V664_Key.ufw",
    )?)
    .unwrap();
    Some((image, entry(&nexus, "flash3.bin")))
}

#[test]
fn flash_640_over_664() {
    let Some((image, head)) = images() else {
        return;
    };
    let mut pad = Pad::new(&head);
    let mut lines = Vec::new();
    let mut log = |line: &str| lines.push(line.to_owned());
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    flash(&mut s, true, true).unwrap();
    drop(s);
    let plan = {
        let bin = entry(&image, "flash.bin");
        let dev = gsfw_core::jieli::parse_flash_head(&head[..0x100], 2, 1).unwrap();
        plan_flash(&bin, &dev).unwrap()
    };
    assert_eq!(
        (plan.c_addr, plan.kill_addr),
        (0x3000, 0x3_F000),
        "bank 1, kill bank 2"
    );
    let c = index(plan.c_addr);
    assert_eq!(
        pad.flash[c..][..plan.region_c.len()],
        plan.region_c,
        "region C written"
    );
    let k = index(plan.kill_addr);
    assert_eq!(
        pad.flash[k..][..KILL_LEN],
        [0; KILL_LEN],
        "running dir head zeroed"
    );
    assert!(
        pad.flash[..c].iter().all(|&b| b == 0x55),
        "region A untouched"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains(&format!("{PAD_C0_KEY:#06x}"))),
        "pad's C0 key logged"
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("done; replug the pad"),
        "{lines:#?}"
    );
}

#[test]
fn region_a_mismatch_stops_before_writing() {
    let Some((image, head)) = images() else {
        return;
    };
    let mut pad = Pad::new(&head);
    let mut log = |_: &str| {};
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    let err = flash(&mut s, true, false).unwrap_err();
    drop(s);
    assert!(err.to_string().contains("region A differs"), "{err}");
    assert!(pad.flash.iter().all(|&b| b == 0x55), "nothing written");
}

#[test]
fn checks_only_writes_nothing() {
    let Some((image, head)) = images() else {
        return;
    };
    let mut pad = Pad::new(&head);
    let mut log = |_: &str| {};
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    flash(&mut s, false, true).unwrap();
    assert_eq!(
        device_crc(&mut s, 0x3000, 0x100).unwrap(),
        crc16(&[0x55; 0x100]),
        "C5 after checks"
    );
    drop(s);
    assert!(pad.flash.iter().all(|&b| b == 0x55), "nothing written");
}

#[test]
fn failed_verify_stops_after_5_tries() {
    let Some((image, head)) = images() else {
        return;
    };
    let mut pad = Pad::new(&head);
    pad.ignore_writes = true;
    let mut log = |_: &str| {};
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    let err = flash(&mut s, true, true).unwrap_err();
    drop(s);
    assert!(err.to_string().contains("failed 5 times"), "{err}");
    assert!(
        pad.flash[0x3_F000..0x4_0000].iter().all(|&b| b == 0x55),
        "running bank untouched"
    );
}

#[test]
fn captured_c0_reply() {
    let Some(path) = fixture("captures/jlgip-c0-reply1.bin") else {
        return;
    };
    let pkt = std::fs::read(path).unwrap();
    assert_eq!(
        find_key(&pkt),
        Some(PAD_C0_KEY),
        "the pad picks its own key"
    );
    let plain = unscramble(&pkt, PAD_C0_KEY).unwrap();
    assert_eq!(u32_at(&plain, 0x10), 1, "handshake reply");
    assert_eq!(&plain[0x18..0x1C], b"HJX1", "SDK id");
    assert!(
        gsfw_core::jieli::session_key(&[0; 16], &plain).is_ok(),
        "session key"
    );
}

/// The recorded 6.40 flash (`notes/facts.md`, "Flashed 6.40 back"): region C at 0x3000,
/// 0x2d000 bytes, CRC 0xd820; the running bank's head at 0x3f000.
const C_ADDR: usize = 0x3000;
const C_LEN: usize = 0x2_D000;
const C_CRC: u16 = 0xd820;
const KILL_ADDR: usize = 0x3_F000;

/// Q1 (`docs/architecture.md`): a link drop during a writing flash. [`Pad::guard`] checks
/// the pad at every packet of every flash in this file, so the pad state after a drop at any
/// call is covered by `flash_640_over_664`. This test checks that the host reports the drop:
/// at drop points spread over the whole flash, both before and after the pad acts.
#[test]
fn a_link_drop_is_reported() {
    let Some((image, head)) = images() else {
        return;
    };
    let mut log = |_: &str| {};
    let mut pad = Pad::new(&head);
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    flash(&mut s, true, true).unwrap();
    drop(s);
    let total = pad.calls;

    for calls in (0..total).step_by(97).chain(total - 8..total) {
        for delivered in [false, true] {
            let mut pad = Pad::new(&head);
            pad.link = Link::DropAfter { calls, delivered };
            let result = open_session(&mut pad, &image, &mut log, None, &[7; 16])
                .and_then(|mut s| flash(&mut s, true, true));
            assert!(
                result.is_err(),
                "drop after {calls} of {total} calls, delivered {delivered}: not reported"
            );
        }
    }
}
