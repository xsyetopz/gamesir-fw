//! Open an upgrade session with a pad: handshake (C0), query (C1), CRC reads (C5).

use core::fmt;
use core::hash::{BuildHasher as _, Hasher as _};
use core::time::Duration;
use std::hash::RandomState;

use crate::formats::ufw::Ufw;
use crate::jieli::{
    FlashHead, ProtocolError, STATUS_KEY_MISMATCH, TOOL_ID, build, c0_handshake, c1_query, c5_crc,
    find_key, parse_c1, parse_crc, session_key, tag_of, unscramble,
};

/// Receives one progress line at a time.
pub type Log<'a> = &'a mut dyn FnMut(&str);

/// The pad did not answer, or answered in a way the upgrade cannot continue from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceError {
    /// The transport or the pad failed.
    Pad(String),
    /// A packet or an image failed a protocol check.
    Protocol(ProtocolError),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pad(message) => f.write_str(message),
            Self::Protocol(err) => err.fmt(f),
        }
    }
}

impl core::error::Error for DeviceError {}

impl From<ProtocolError> for DeviceError {
    fn from(err: ProtocolError) -> Self {
        Self::Protocol(err)
    }
}

/// Packet transport to a pad in GIP mode (USB adapter; a simulated pad in tests).
pub trait GipLink {
    /// Sends one 512-byte packet, fragment by fragment, each acknowledged by the pad.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when a fragment gets no acknowledgement.
    fn send(&mut self, pkt: &[u8]) -> Result<(), DeviceError>;

    /// Waits up to `waits` seconds for one reassembled reply packet.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when no complete reply arrives.
    fn reply(&mut self, waits: u32) -> Result<Vec<u8>, DeviceError>;

    /// Reads and drops whatever the pad sends for `time`.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when the transport fails.
    fn drain(&mut self, time: Duration) -> Result<(), DeviceError>;
}

/// An open upgrade session.
pub struct Session<'a> {
    /// The transport.
    pub link: &'a mut dyn GipLink,
    /// The session key from C0.
    pub key: u16,
    /// The image the session was opened with (its chip key went into C1).
    pub image: &'a Ufw,
    /// The flash head the pad reported in its C1 reply.
    pub head: FlashHead,
    /// Progress lines.
    pub log: Log<'a>,
}

/// 16 bytes for C0. The DLL uses `rand()`; any 16 bytes work, so the randomly keyed standard
/// hasher is enough.
#[must_use]
pub fn random16() -> [u8; 16] {
    let state = RandomState::new();
    let mut first = state.build_hasher();
    first.write_u8(1);
    let mut second = state.build_hasher();
    second.write_u8(2);
    let bytes = first
        .finish()
        .to_le_bytes()
        .into_iter()
        .chain(second.finish().to_le_bytes());
    let mut out = [0; 16];
    for (slot, byte) in out.iter_mut().zip(bytes) {
        *slot = byte;
    }
    out
}

/// Descrambles `pkt` with `key`; if that fails, finds the key the pad used and logs it.
///
/// # Errors
/// [`DeviceError::Pad`] when no key descrambles the packet.
pub fn unscramble_any(
    pkt: &[u8],
    key: u16,
    what: &str,
    log: Log<'_>,
) -> Result<Vec<u8>, DeviceError> {
    if let Ok(plain) = unscramble(pkt, key) {
        return Ok(plain);
    }
    let Some(found) = find_key(pkt) else {
        let head = pkt.get(..32).unwrap_or(pkt);
        return Err(DeviceError::Pad(format!(
            "{what}: no key descrambles it; raw: {} ...",
            crate::bytes::hex(head, " ")
        )));
    };
    log(&format!(
        "{what}: scrambled with {found:#06x}, not {key:#06x}"
    ));
    Ok(unscramble(pkt, found)?)
}

/// Sends `body` and returns the descrambled reply that echoes its tag. Stale replies (to C3 and
/// C4 packets, which are not read) are skipped.
///
/// # Errors
/// [`DeviceError`] from the link, or when 20 replies pass without the tag.
pub fn request(
    link: &mut dyn GipLink,
    body: &[u8],
    key: u16,
    what: &str,
    log: Log<'_>,
    waits: u32,
) -> Result<Vec<u8>, DeviceError> {
    let pkt = build(body, key, None)?;
    let tag = tag_of(&unscramble(&pkt, key)?)?;
    link.send(&pkt)?;
    for _ in 0..20 {
        let plain = unscramble_any(&link.reply(waits)?, key, what, log)?;
        if tag_of(&plain)? == tag {
            return Ok(plain);
        }
    }
    Err(DeviceError::Pad(format!(
        "{what}: no reply with tag {tag:#x}"
    )))
}

/// C0 (skipped when `session_key` is given: the pad answers one C0 per power-up), then C1.
///
/// # Errors
/// [`DeviceError`] when the image has no chip key, the pad does not answer, or C1 fails.
pub fn open_session<'a>(
    link: &'a mut dyn GipLink,
    image: &'a Ufw,
    log: Log<'a>,
    session: Option<u16>,
    host_rand: &[u8; 16],
) -> Result<Session<'a>, DeviceError> {
    let chipkey = image
        .chipkey
        .ok_or_else(|| DeviceError::Pad("image has no chip key; C1 needs it".to_owned()))?;
    let key = match session {
        Some(key) => {
            log(&format!("reusing session key {key:#06x}"));
            key
        }
        None => handshake(link, log, host_rand)?,
    };
    let head = query(link, chipkey, key, log)?;
    Ok(Session {
        link,
        key,
        image,
        head,
        log,
    })
}

/// C0: the session key from both randoms.
///
/// # Errors
/// [`DeviceError`] when the pad does not answer with a handshake reply.
fn handshake(
    link: &mut dyn GipLink,
    log: Log<'_>,
    host_rand: &[u8; 16],
) -> Result<u16, DeviceError> {
    link.send(&build(&c0_handshake(host_rand), TOOL_ID, None)?)?;
    let reply = unscramble_any(&link.reply(5)?, TOOL_ID, "C0 reply", log)?;
    let key = session_key(host_rand, &reply)?;
    log(&format!("handshake ok, session key {key:#06x}"));
    Ok(key)
}

/// C1: the device flash head, logged.
///
/// # Errors
/// [`DeviceError`] on a chip key mismatch or a non-zero status.
fn query(
    link: &mut dyn GipLink,
    chipkey: u16,
    key: u16,
    log: Log<'_>,
) -> Result<FlashHead, DeviceError> {
    let plain = request(link, &c1_query(chipkey), key, "C1 reply", log, 5)?;
    let (status, head) = parse_c1(&plain)?;
    if status.code == STATUS_KEY_MISMATCH {
        return Err(DeviceError::Pad(
            "C1: image chip key does not match the device key (status -3)".to_owned(),
        ));
    }
    let head = match head {
        Some(head) if status.code == 0 => head,
        _ => return Err(DeviceError::Pad(format!("C1 status {}", status.code))),
    };
    log(&format!(
        "C1 ok: mode {}, eoffset {:#x}, flash size {:#x}, head crc ok {}",
        head.mode, head.eoffset, head.size, head.header_ok
    ));
    for entry in &head.entries {
        log(&format!("  {:16} addr {:#08x}", entry.name, entry.addr));
    }
    let (dir1, dir2) = head.app_dirs();
    let show = |dir: Option<u32>| dir.map_or_else(|| "-".to_owned(), |a| format!("{a:#x}"));
    log(&format!(
        "  app dirs (after eoffset): {}, {}",
        show(dir1),
        show(dir2)
    ));
    Ok(head)
}

/// C5: the pad's CRC-16 over `length` bytes of flash at `addr`.
///
/// # Errors
/// [`DeviceError`] from the link, or a non-zero C5 status.
pub fn device_crc(s: &mut Session<'_>, addr: u32, length: u32) -> Result<u16, DeviceError> {
    let plain = request(s.link, &c5_crc(addr, length), s.key, "C5 reply", s.log, 10)?;
    let (status, crc) = parse_crc(&plain)?;
    if status != 0 {
        return Err(DeviceError::Pad(format!(
            "C5 {addr:#x}+{length:#x}: status {status}"
        )));
    }
    Ok(crc)
}

#[cfg(test)]
mod tests;
