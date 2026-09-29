//! [`GipLink`] over USB (nusb). The pad must enumerate with an Xbox GIP interface (class
//! ff/47/d0). Windows: bind that interface to `WinUSB` with Zadig. Linux: root or a udev rule;
//! the xpad driver is detached while the link is open. macOS: no driver claims it, but no other
//! program (for example `OpenJoystickDriver`) may hold it.

use core::fmt::Write as _;
use core::time::Duration;
use std::time::Instant;

use nusb::descriptors::TransferType;
use nusb::transfer::{Buffer, Direction, In, Interrupt, Out};
use nusb::{Device, Endpoint, Interface, MaybeFuture as _};

use crate::app::{DeviceError, GipLink};
use crate::bytes::hex;
use crate::jieli::{
    GIP_NEEDS_ACK, GIP_POWER_ON, Reassembler, fragments, gip_ack, gip_unwrap, gip_wrap, is_ack,
    reply_fragment,
};

/// `GameSir`'s USB vendor id.
pub const VID: u16 = 0x3537;
/// Class, subclass and protocol of an Xbox GIP interface.
pub const GIP_CLASS: (u8, u8, u8) = (0xFF, 0x47, 0xD0);
/// How long the pad may send messages after the power-on message.
const POWER_ON_WAIT: Duration = Duration::from_millis(500);
/// Sends of one fragment before [`GipLink::send`] stops. The DLL resends without a limit.
const SEND_TRIES: usize = 5;
const POLL: Duration = Duration::from_millis(100);
const ACK_WAIT: Duration = Duration::from_secs(1);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Receives one traced USB message at a time.
pub type Trace = Box<dyn FnMut(&str)>;

fn usb(what: &str, err: impl core::fmt::Display) -> DeviceError {
    DeviceError::Pad(format!("USB {what}: {err}"))
}

/// The USB devices, collected: the Linux iterator is larger than the stack frame limit.
///
/// # Errors
/// [`DeviceError::Pad`] when the USB devices cannot be listed.
fn devices() -> Result<Vec<nusb::DeviceInfo>, DeviceError> {
    Ok(nusb::list_devices()
        .wait()
        .map_err(|e| usb("list", e))?
        .collect())
}

/// One line per device with [`VID`] and one per interface, GIP interfaces marked.
///
/// # Errors
/// [`DeviceError::Pad`] when the USB devices cannot be listed.
pub fn list_devices() -> Result<Vec<String>, DeviceError> {
    let mut lines = Vec::new();
    for info in devices()? {
        if info.vendor_id() != VID {
            continue;
        }
        lines.push(format!(
            "{:04x}:{:04x} bcdDevice {:#06x} bus {} addr {} {}",
            info.vendor_id(),
            info.product_id(),
            info.device_version(),
            info.bus_id(),
            info.device_address(),
            info.product_string().unwrap_or(""),
        ));
        match info.open().wait() {
            Ok(device) => lines.extend(interface_lines(&device)),
            Err(err) => lines.push(format!("  cannot open: {err}")),
        }
    }
    Ok(lines)
}

fn interface_lines(device: &Device) -> Vec<String> {
    let Ok(config) = device.active_configuration() else {
        return vec!["  no active configuration".to_owned()];
    };
    config
        .interface_alt_settings()
        .map(|alt| {
            let class = (alt.class(), alt.subclass(), alt.protocol());
            let mut line = format!(
                "  if{} alt{} class {:02x}/{:02x}/{:02x} ep",
                alt.interface_number(),
                alt.alternate_setting(),
                class.0,
                class.1,
                class.2
            );
            for ep in alt.endpoints() {
                let kind = match ep.transfer_type() {
                    TransferType::Control => "control",
                    TransferType::Isochronous => "iso",
                    TransferType::Bulk => "bulk",
                    TransferType::Interrupt => "int",
                };
                // Writing to a String cannot fail.
                if write!(
                    line,
                    " {:#04x}/{}/{kind}",
                    ep.address(),
                    ep.max_packet_size()
                )
                .is_err()
                {
                    line.push_str(" ?");
                }
            }
            if class == GIP_CLASS {
                line.push_str("  <- GIP");
            }
            line
        })
        .collect()
}

/// Sets the first configuration of `device` when it has no active configuration.
///
/// macOS configures a device only when a driver matches it, and no macOS driver matches the GIP
/// class. So a GIP pad that no program opened stays unconfigured.
///
/// # Errors
/// [`DeviceError::Pad`] when the device refuses the configuration.
fn configure(device: &Device) -> Result<(), DeviceError> {
    if device.active_configuration().is_ok() {
        return Ok(());
    }
    let Some(value) = device
        .configurations()
        .next()
        .map(|config| config.configuration_value())
    else {
        return Ok(());
    };
    device
        .set_configuration(value)
        .wait()
        .map_err(|e| usb("set configuration", e))
}

/// The first GIP interface (alt 0) on a [`VID`] device, with its endpoint addresses.
///
/// # Errors
/// [`DeviceError::Pad`] when no device has one.
fn find_gip(pid: Option<u16>) -> Result<(Device, u8, u8, u8), DeviceError> {
    let mut seen = Vec::new();
    for info in devices()? {
        if info.vendor_id() != VID || pid.is_some_and(|p| p != info.product_id()) {
            continue;
        }
        seen.push(format!("{:04x}", info.product_id()));
        let device = info.open().wait().map_err(|e| usb("open", e))?;
        configure(&device)?;
        let found = device.active_configuration().ok().and_then(|config| {
            config.interface_alt_settings().find_map(|alt| {
                let class = (alt.class(), alt.subclass(), alt.protocol());
                if class != GIP_CLASS || alt.alternate_setting() != 0 {
                    return None;
                }
                let out = alt
                    .endpoints()
                    .find(|ep| ep.direction() == Direction::Out)?
                    .address();
                let inp = alt
                    .endpoints()
                    .find(|ep| ep.direction() == Direction::In)?
                    .address();
                Some((alt.interface_number(), out, inp))
            })
        });
        if let Some((number, out, inp)) = found {
            return Ok((device, number, out, inp));
        }
    }
    let seen = if seen.is_empty() {
        "none".to_owned()
    } else {
        seen.join(", ")
    };
    Err(DeviceError::Pad(format!(
        "no GIP interface on VID 3537 (devices: {seen}); the pad must boot into GIP mode"
    )))
}

/// An open GIP interface.
pub struct UsbGipLink {
    out: Endpoint<Interrupt, Out>,
    inp: Endpoint<Interrupt, In>,
    flags: u8,
    seq: u8,
    trace: Option<Trace>,
    /// Product id, device version, interface and endpoints, for the log.
    pub name: String,
    // Declared last: the endpoints are dropped (and their transfers cancelled) first.
    _interface: Interface,
}

impl UsbGipLink {
    /// Claims the GIP interface of the first matching pad; sends the xpad power-on message
    /// when `power_on`.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when no pad has a GIP interface or it cannot be claimed.
    pub fn open(
        pid: Option<u16>,
        flags: u8,
        power_on: bool,
        trace: Option<Trace>,
    ) -> Result<Self, DeviceError> {
        let (device, number, out, inp) = find_gip(pid)?;
        let name = format!(
            "GIP {:04x} bcdDevice {:#06x} if{number} out {out:#04x} in {inp:#04x}",
            device.device_descriptor().product_id(),
            device.device_descriptor().device_version(),
        );
        let interface = device
            .detach_and_claim_interface(number)
            .wait()
            .map_err(|e| usb("claim", e))?;
        let mut link = Self {
            out: interface
                .endpoint(out)
                .map_err(|e| usb("OUT endpoint", e))?,
            inp: interface.endpoint(inp).map_err(|e| usb("IN endpoint", e))?,
            flags,
            seq: 0,
            trace,
            name,
            _interface: interface,
        };
        if power_on {
            link.write(&GIP_POWER_ON)?;
            link.drain(POWER_ON_WAIT)?;
        }
        Ok(link)
    }

    fn trace(&mut self, dir: &str, msg: &[u8]) {
        if let Some(trace) = self.trace.as_mut() {
            trace(&format!("{dir} {}", hex(msg, " ")));
        }
    }

    /// Writes one message to the OUT endpoint.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when a USB transfer fails.
    fn write(&mut self, msg: &[u8]) -> Result<(), DeviceError> {
        self.trace(" >", msg);
        let done = self
            .out
            .transfer_blocking(Buffer::from(msg.to_vec()), WRITE_TIMEOUT);
        done.status.map_err(|e| usb("write", e))
    }

    /// One IN message, or `None` after `timeout`. A read stays queued across calls.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when a USB transfer fails.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError> {
        if self.inp.pending() == 0 {
            let buffer = self.inp.allocate(self.inp.max_packet_size());
            self.inp.submit(buffer);
        }
        let Some(done) = self.inp.wait_next_complete(timeout) else {
            return Ok(None);
        };
        done.status.map_err(|e| usb("read", e))?;
        let msg = done.buffer.to_vec();
        self.trace(" <", &msg);
        if let Some((cmd, flags, seq, payload)) = gip_unwrap(&msg)
            && flags & GIP_NEEDS_ACK != 0
        {
            let len = u8::try_from(payload.len()).unwrap_or(u8::MAX);
            self.write(&gip_ack(cmd, flags, seq, len))?;
        }
        Ok(Some(msg))
    }

    /// The upgrade fragment inside the next IN message, if any.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when a USB transfer fails.
    fn fragment_in(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError> {
        let Some(msg) = self.read(timeout)? else {
            return Ok(None);
        };
        let Some((_, _, _, payload)) = gip_unwrap(&msg) else {
            return Ok(None);
        };
        Ok(reply_fragment(payload).map(<[u8]>::to_vec))
    }

    /// Whether the ack for fragment `idx` arrives within a second.
    ///
    /// # Errors
    /// [`DeviceError::Pad`] when a USB transfer fails.
    fn acked(&mut self, idx: u8) -> Result<bool, DeviceError> {
        let end = Instant::now().checked_add(ACK_WAIT);
        while end.is_some_and(|end| Instant::now() < end) {
            if self.fragment_in(POLL)?.is_some_and(|f| is_ack(&f, idx)) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl GipLink for UsbGipLink {
    /// Each fragment waits for its `4C 0A idx` ack. Reply fragments that arrive meanwhile are
    /// dropped (the DLL does not read C3 and C4 replies either).
    fn send(&mut self, pkt: &[u8]) -> Result<(), DeviceError> {
        for frag in fragments(pkt) {
            let idx = frag.get(2).copied().unwrap_or_default();
            let mut acked = false;
            for _ in 0..SEND_TRIES {
                // The sequence runs 1..=255, then starts at 1 again.
                self.seq = self.seq.checked_add(1).unwrap_or(1);
                self.write(&gip_wrap(&frag, self.seq, self.flags))?;
                if self.acked(idx)? {
                    acked = true;
                    break;
                }
            }
            if !acked {
                return Err(DeviceError::Pad(format!("no ack for fragment {idx}")));
            }
        }
        Ok(())
    }

    fn reply(&mut self, waits: u32) -> Result<Vec<u8>, DeviceError> {
        let mut parts = Reassembler::new();
        let end = Instant::now().checked_add(Duration::from_secs(waits.into()));
        while end.is_some_and(|end| Instant::now() < end) {
            if let Some(frag) = self.fragment_in(POLL)?
                && let Some(pkt) = parts.feed(&frag)
            {
                return Ok(pkt);
            }
        }
        Err(DeviceError::Pad("no reply packet".to_owned()))
    }

    fn drain(&mut self, time: Duration) -> Result<(), DeviceError> {
        let end = Instant::now().checked_add(time);
        while end.is_some_and(|end| Instant::now() < end) {
            self.read(POLL)?;
        }
        Ok(())
    }
}
