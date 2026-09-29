//! Fragments (0x1f80 out, 0x2120 in) and the GIP framing the Windows GIP driver adds. The GIP
//! header bytes are a guess, see the module doc of [`super`].

use std::collections::HashMap;

use crate::bytes::{put, window};

/// Command byte of an outgoing or reply fragment (0x1f80).
pub const FRAG_CMD: u8 = 0x4A;
/// Command byte of a fragment acknowledgement.
pub const ACK_CMD: u8 = 0x4C;
/// Fragments per packet.
pub const FRAG_TOTAL: u8 = 10;
/// Data bytes per fragment.
pub const FRAG_DATA: usize = 0x34;
/// Fragment length, 56: what `JL_handleGipData` (0x2120) reads from each message.
pub const FRAG_LEN: usize = 4 + FRAG_DATA;
/// Nexus `SendToDevice` prefix: 0x3C bytes = prefix + fragment.
pub const OUT_PREFIX: [u8; 4] = [0xF0, 0x00, 0x00, 0x00];
/// First payload byte of a vendor reply.
pub const REPLY_MARK: u8 = 0xF1;
/// GIP vendor message id (`ID4XDevice.SendMessage(data, 0x0F)`).
pub const GIP_VENDOR_CMD: u8 = 0x0F;
/// GUESS (g7ctl framing); the Windows GIP driver builds the real header.
pub const GIP_FLAGS: u8 = 0x00;
/// Linux xpad `xboxone_power_on`.
pub const GIP_POWER_ON: [u8; 5] = [0x05, 0x20, 0x00, 0x01, 0x00];

/// Ten 56-byte `4A 0A idx len data` fragments of `pkt`, idx 1..=10, len 0x34 (0x2c for the
/// last of a 512-byte packet), data zero-padded.
#[must_use]
pub fn fragments(pkt: &[u8]) -> Vec<[u8; FRAG_LEN]> {
    (1..=FRAG_TOTAL)
        .zip((0..).step_by(FRAG_DATA))
        .map(|(idx, at)| {
            let chunk = window(pkt, at, FRAG_DATA);
            let len = u8::try_from(chunk.len()).unwrap_or(u8::MAX);
            let mut frag = [0; FRAG_LEN];
            put(&mut frag, 0, &[FRAG_CMD, FRAG_TOTAL, idx, len]);
            put(&mut frag, 4, chunk);
            frag
        })
        .collect()
}

/// Whether `frag` acknowledges fragment `idx`.
#[must_use]
pub fn is_ack(frag: &[u8], idx: u8) -> bool {
    frag.get(..3) == Some(&[ACK_CMD, FRAG_TOTAL, idx][..])
}

/// Collects reply fragments `4A total idx len data` into one packet.
#[derive(Clone, Debug, Default)]
pub struct Reassembler {
    parts: HashMap<u8, Vec<u8>>,
    total: u8,
}

impl Reassembler {
    /// An empty reassembler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one fragment; returns the packet once all `total` fragments are in. Fragments that
    /// are not `4A` or have a bad index or length are ignored; a new `total` starts over.
    pub fn feed(&mut self, frag: &[u8]) -> Option<Vec<u8>> {
        let &[cmd, total, idx, n, ref data @ ..] = frag else {
            return None;
        };
        if cmd != FRAG_CMD || idx == 0 || idx > total || usize::from(n) > data.len() {
            return None;
        }
        if total != self.total {
            self.parts.clear();
            self.total = total;
        }
        self.parts
            .insert(idx, window(data, 0, usize::from(n)).to_vec());
        if self.parts.len() < usize::from(total) {
            return None;
        }
        let pkt = (1..=total)
            .map(|i| self.parts.get(&i).map(Vec::as_slice))
            .collect::<Option<Vec<_>>>()?
            .concat();
        self.parts.clear();
        self.total = 0;
        Some(pkt)
    }
}

/// One GIP vendor message: `0F flags seq len` + [`OUT_PREFIX`] + `frag`.
#[must_use]
pub fn gip_wrap(frag: &[u8; FRAG_LEN], seq: u8, flags: u8) -> Vec<u8> {
    let len = u8::try_from(OUT_PREFIX.len().saturating_add(FRAG_LEN)).unwrap_or(u8::MAX);
    [&[GIP_VENDOR_CMD, flags, seq, len][..], &OUT_PREFIX, frag].concat()
}

/// `(cmd, flags, seq, payload)` of a single-byte-length GIP message, else `None`.
#[must_use]
pub fn gip_unwrap(msg: &[u8]) -> Option<(u8, u8, u8, &[u8])> {
    let &[cmd, flags, seq, len, ref rest @ ..] = msg else {
        return None;
    };
    if len & 0x80 != 0 {
        return None;
    }
    Some((cmd, flags, seq, window(rest, 0, usize::from(len))))
}

/// The 56-byte fragment inside a vendor reply payload `F1 ?? ?? ??` + fragment.
#[must_use]
pub fn reply_fragment(payload: &[u8]) -> Option<&[u8]> {
    (payload.len() >= 8 && payload.first() == Some(&REPLY_MARK))
        .then(|| window(payload, 4, FRAG_LEN))
}

#[cfg(test)]
mod tests;
