//! The flash use case against a simulated pad (C1 reports mode 2 and EOFFSET 1), writing an
//! image that the test builds: a `flash.bin` whose directory matches the pad's.
#![cfg(test)]

use core::time::Duration;
use gsfw_core::app::{DeviceError, GipLink, device_crc, flash, open_session};
use gsfw_core::formats::ufw::{Entry, Format, Ufw};
use gsfw_core::formats::{crc16, enc};
use gsfw_core::jieli::{
    FLASH_HEAD_KEY, HEAD_LEN, KILL_LEN, TOOL_ID, build, parse_flash_head, plan_flash, tag_of,
    unscramble,
};

/// The key the pad scrambles its C0 reply with. It is not the host's key.
const PAD_C0_KEY: u16 = 0x5A5A;
const DEV_RAND: [u8; 16] = *b"0123456789abcdef";

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
    /// The CRC of the complete region C.
    c_crc: u16,
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
            c_crc: crc16(&region_c()),
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
    /// Q1 (`ARCHITECTURE.md`): checks a C3 or C4 of `len` bytes at `addr` before the pad
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
                self.c_crc,
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

/// The directory addresses in the image and in the pad head. EOFFSET 1 moves them by 0x1000.
const DIR_HEAD: u32 = 0x2000;
const DIR_HEAD2: u32 = 0x3_E000;
/// Mode 2: region C goes to the moved `app_dir_head`, the zero bytes to the moved
/// `app_dir_head2`.
const C_ADDR: usize = 0x3000;
const KILL_ADDR: usize = 0x3_F000;
/// Two 4 KiB sectors, 32 C4 writes.
const C_LEN: usize = 0x2000;
/// Header byte 13: an erase unit of 0x1000.
const UNIT_BYTE: u8 = 0x10;

/// The region C bytes of the image: no byte is 0xFF or the pad's 0x55.
fn region_c() -> Vec<u8> {
    (0..C_LEN)
        .map(|i| [0x12, 0x34, 0xA0, 0x0F][i % 4])
        .collect()
}

/// A flash head: a 32-byte header with its CRC, the size and the erase unit, then the two
/// directory entries and erased records. Each 32-byte record is ENC-scrambled on its own.
fn flash_head() -> Vec<u8> {
    let mut plain = vec![0xFF; HEAD_LEN];
    plain[..32].fill(0);
    plain[8..12].copy_from_slice(&0x4_0000_u32.to_le_bytes());
    plain[13] = UNIT_BYTE;
    let crc = crc16(&plain[2..32]);
    plain[..2].copy_from_slice(&crc.to_le_bytes());
    for (at, name, addr) in [
        (32, "app_dir_head", DIR_HEAD),
        (64, "app_dir_head2", DIR_HEAD2),
    ] {
        let record = &mut plain[at..][..32];
        record.fill(0);
        record[8..12].copy_from_slice(&addr.to_le_bytes());
        record[16..][..name.len()].copy_from_slice(name.as_bytes());
    }
    plain
        .chunks(32)
        .flat_map(|r| enc(r, FLASH_HEAD_KEY))
        .collect()
}

/// `flash.bin`: the head, filler up to `app_dir_head`, then region C.
fn flash_bin() -> Vec<u8> {
    let mut bin = flash_head();
    bin.resize(index(DIR_HEAD), 0x77);
    bin.extend(region_c());
    bin
}

/// An image with one `flash.bin` entry and a chip key for C1.
fn image() -> Ufw {
    let data = flash_bin();
    let size = u32::try_from(data.len()).unwrap();
    Ufw {
        format: Format::Fw,
        hdr_key: 0x1234,
        hdr_crc: 0,
        list_crc: 0,
        size,
        count: 1,
        f0a: 0,
        f0c: 0,
        chip: "AC695X".to_owned(),
        hdr_ok: true,
        list_ok: true,
        entries: vec![Entry {
            kind: 0,
            index: 0,
            crc: crc16(&data),
            offset: 0,
            size,
            padded: size,
            name: "flash.bin".to_owned(),
        }],
        chipkey: Some(0x1234),
        data,
    }
}

fn images() -> (Ufw, Vec<u8>) {
    (image(), flash_head())
}

#[test]
fn flash_writes_region_c_then_kills_the_running_head() {
    let (image, head) = images();
    let mut pad = Pad::new(&head);
    let mut lines = Vec::new();
    let mut log = |line: &str| lines.push(line.to_owned());
    let mut s = open_session(&mut pad, &image, &mut log, None, &[7; 16]).unwrap();
    flash(&mut s, true, true).unwrap();
    drop(s);
    let plan = plan_flash(&flash_bin(), &parse_flash_head(&head, 2, 1).unwrap()).unwrap();
    assert_eq!(
        (plan.c_addr, plan.kill_addr),
        (0x3000, 0x3_F000),
        "bank 1, kill bank 2"
    );
    let c = index(plan.c_addr);
    assert_eq!(pad.flash[c..][..C_LEN], region_c(), "region C written");
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
    let (image, head) = images();
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
    let (image, head) = images();
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
    let (image, head) = images();
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

/// Q1 (`ARCHITECTURE.md`): a link drop during a writing flash. [`Pad::guard`] checks
/// the pad at every packet of every flash in this file, so the pad state after a drop at any
/// call is covered by `flash_writes_region_c_then_kills_the_running_head`. This test checks that the host reports the drop:
/// at drop points spread over the whole flash, both before and after the pad acts.
#[test]
fn a_link_drop_is_reported() {
    let (image, head) = images();
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
