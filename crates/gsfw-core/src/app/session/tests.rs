use core::time::Duration;

use super::{DeviceError, GipLink, request};
use crate::jieli::{build, c5_crc, tag_of, unscramble};

const KEY: u16 = 0x4242;

/// Answers each packet with `stale` replies carrying other tags, then (if `answer`) the reply.
struct Echo {
    stale: usize,
    answer: bool,
    queue: Vec<Vec<u8>>,
}

impl GipLink for Echo {
    fn send(&mut self, pkt: &[u8]) -> Result<(), DeviceError> {
        let tag = tag_of(&unscramble(pkt, KEY)?)?;
        for n in 1..=self.stale {
            let other = tag.wrapping_add(u32::try_from(n).unwrap());
            self.queue.push(build(&[0; 8], KEY, Some(other))?);
        }
        if self.answer {
            self.queue.push(build(b"answer!!", KEY, Some(tag))?);
        }
        Ok(())
    }

    fn reply(&mut self, _waits: u32) -> Result<Vec<u8>, DeviceError> {
        if self.queue.is_empty() {
            return Err(DeviceError::Pad("no reply packet".to_owned()));
        }
        Ok(self.queue.remove(0))
    }

    fn drain(&mut self, _time: Duration) -> Result<(), DeviceError> {
        self.queue.clear();
        Ok(())
    }
}

#[test]
fn request_skips_stale_replies() {
    let mut link = Echo {
        stale: 3,
        answer: true,
        queue: Vec::new(),
    };
    let plain = request(&mut link, &c5_crc(0, 1), KEY, "C5", &mut |_| {}, 1).unwrap();
    assert_eq!(&plain[0x10..0x18], b"answer!!", "the reply with the tag");
    assert!(link.queue.is_empty(), "nothing left over");
}

#[test]
fn request_gives_up_after_20_stale_replies() {
    let mut link = Echo {
        stale: 25,
        answer: true,
        queue: Vec::new(),
    };
    let err = request(&mut link, &c5_crc(0, 1), KEY, "C5", &mut |_| {}, 1).unwrap_err();
    assert!(
        matches!(err, DeviceError::Pad(ref m) if m.contains("no reply with tag")),
        "{err}"
    );
}

#[test]
fn request_passes_link_errors() {
    let mut link = Echo {
        stale: 0,
        answer: false,
        queue: Vec::new(),
    };
    let err = request(&mut link, &c5_crc(0, 1), KEY, "C5", &mut |_| {}, 1).unwrap_err();
    assert_eq!(
        err,
        DeviceError::Pad("no reply packet".to_owned()),
        "link error"
    );
}
