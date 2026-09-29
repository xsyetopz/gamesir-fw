//! The operations of the CLI and the GUI. Both front ends build an [`Op`] and [`run`] it, so
//! they offer the same operations with the same output.

use alloc::rc::Rc;
use core::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::{
    DeviceError, device_crc, fetch as fetch_artifact, flash, is_flash_bin, open_session, random16,
};
use crate::bytes::hex;
use crate::catalog;
use crate::formats::{FormatError, crc16, flashtool, nexus, ufw};
use crate::jieli::{
    GIP_FLAGS, HEAD_LEN, ProtocolError, RAND_LEN, TOOL_ID, build, c0_handshake, c1_query,
    fragments, gip_wrap, parse_flash_head,
};
use crate::net::HttpSource;
use crate::usb::{UsbGipLink, list_devices};

/// Receives the output, one line at a time.
pub type Sink = Rc<dyn Fn(&str)>;

/// What to do with a pad in GIP mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PadAction {
    /// Handshake (C0) and query (C1).
    Probe,
    /// C0, C1, then a CRC read (C5) of `len` bytes at `addr`.
    Crc {
        /// Flash address.
        addr: u32,
        /// Byte count.
        len: u32,
    },
    /// Region C into the bank the pad is not running; checks only unless `write`.
    Flash {
        /// Write; without it only the checks run.
        write: bool,
        /// Flash region C even though the device's region A differs from the image's.
        keep_region_a: bool,
    },
}

/// How to reach the pad.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkOptions {
    /// USB product id; `None` takes any [`crate::usb::VID`] device with a GIP interface.
    pub pid: Option<u16>,
    /// GIP header flags.
    pub flags: u8,
    /// Send the xpad power-on message first.
    pub power_on: bool,
    /// Skip C0 and reuse this session key.
    pub session_key: Option<u16>,
    /// Show every USB message.
    pub verbose: bool,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self {
            pid: None,
            flags: GIP_FLAGS,
            power_on: true,
            session_key: None,
            verbose: false,
        }
    }
}

/// One operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Container entries, CRCs, chip key; flash heads of `flash*.bin`.
    Info {
        /// The `.ufw` or `.fw` image.
        image: PathBuf,
    },
    /// Every entry, decrypted, into `out`.
    Extract {
        /// The image.
        image: PathBuf,
        /// Output directory.
        out: PathBuf,
    },
    /// The decrypted `app.bin`.
    AppBin {
        /// The image.
        image: PathBuf,
        /// Output file.
        out: PathBuf,
    },
    /// The VID/PID and the descriptor files of `descdir` inside the image.
    FindUsb {
        /// The image.
        image: PathBuf,
        /// Directory of binary descriptors.
        descdir: PathBuf,
    },
    /// The `.ufw` images carved out of `HJC.GameSir.Nexus2_0.dll`.
    NexusExtract {
        /// The Nexus DLL.
        dll: PathBuf,
        /// Output directory.
        out: PathBuf,
    },
    /// The files inside a `GameSir` Flash Tool `.exe` (the `.fw` images, `config.ini`).
    FlashToolExtract {
        /// The Flash Tool `.exe`.
        exe: PathBuf,
        /// Output directory.
        out: PathBuf,
    },
    /// The artifacts of the firmware catalog.
    Catalog,
    /// A catalog artifact, checked against its SHA-256, and its files, into `out`.
    Fetch {
        /// The artifact id.
        artifact: String,
        /// Output directory.
        out: PathBuf,
        /// A downloaded copy of the artifact. With it, no URL is used.
        from: Option<PathBuf>,
    },
    /// The GIP messages `probe` would send, with a fixed random and tag.
    DryRun {
        /// The image.
        image: PathBuf,
    },
    /// USB devices with the `GameSir` VID, their interfaces and endpoints.
    List,
    /// An upgrade session with a pad.
    Pad {
        /// The image (its chip key goes into C1).
        image: PathBuf,
        /// What to do.
        action: PadAction,
        /// How to reach the pad.
        link: LinkOptions,
    },
}

/// Why an operation failed.
#[derive(Debug)]
pub enum OpError {
    /// An image or resource file.
    Format(FormatError),
    /// The pad or the protocol.
    Device(DeviceError),
    /// Writing an output file.
    Io {
        /// The file.
        path: PathBuf,
        /// The operating system error.
        source: io::Error,
    },
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(err) => err.fmt(f),
            Self::Device(err) => err.fmt(f),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl core::error::Error for OpError {}

impl From<FormatError> for OpError {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

impl From<DeviceError> for OpError {
    fn from(err: DeviceError) -> Self {
        Self::Device(err)
    }
}

impl From<ProtocolError> for OpError {
    fn from(err: ProtocolError) -> Self {
        Self::Device(err.into())
    }
}

/// Hex, with or without `0x`.
///
/// # Errors
/// The message for a bad number.
pub fn hex_u16(text: &str) -> Result<u16, String> {
    u16::from_str_radix(text.trim_start_matches("0x"), 16).map_err(|err| format!("{text}: {err}"))
}

/// Decimal, or hex with `0x`.
///
/// # Errors
/// The message for a bad number.
pub fn int_u32(text: &str) -> Result<u32, String> {
    let parsed = match text.strip_prefix("0x") {
        Some(digits) => u32::from_str_radix(digits, 16),
        None => text.parse(),
    };
    parsed.map_err(|err| format!("{text}: {err}"))
}

/// Runs `op`, sending its output to `sink`. `Ok(false)` means it ran but found a problem
/// (a CRC mismatch, no USB match).
///
/// # Errors
/// [`OpError`] when a file cannot be read or written, or the pad fails.
pub fn run(op: &Op, sink: &Sink) -> Result<bool, OpError> {
    match op {
        Op::Info { image } => info(image, sink),
        Op::Extract { image, out } => extract(image, out, sink),
        Op::AppBin { image, out } => app_bin(image, out, sink),
        Op::FindUsb { image, descdir } => {
            let found = ufw::find_usb(image, descdir)?;
            say_all(sink, &found.to_string());
            Ok(found.found())
        }
        Op::NexusExtract { dll, out } => nexus_extract(dll, out, sink),
        Op::FlashToolExtract { exe, out } => Ok(carved(flashtool::carve(exe, out)?, sink)),
        Op::Catalog => catalog_list(sink),
        Op::Fetch {
            artifact,
            out,
            from,
        } => fetch(artifact, out, from.as_deref(), sink),
        Op::DryRun { image } => dry_run(image, sink),
        Op::List => list(sink),
        Op::Pad {
            image,
            action,
            link,
        } => pad(image, action, link, sink),
    }
}

/// # Errors
/// [`OpError::Format`] when the image cannot be read or an entry cannot be written.
fn extract(image: &Path, out: &Path, sink: &Sink) -> Result<bool, OpError> {
    for path in ufw::extract(image, out)? {
        sink(&path.display().to_string());
    }
    Ok(true)
}

/// # Errors
/// [`OpError`] when the image cannot be read or `out` cannot be written.
fn app_bin(image: &Path, out: &Path, sink: &Sink) -> Result<bool, OpError> {
    let app = ufw::app_bin(&ufw::load(image)?)?;
    std::fs::write(out, app).map_err(|source| OpError::Io {
        path: out.to_path_buf(),
        source,
    })?;
    sink(&out.display().to_string());
    Ok(true)
}

/// # Errors
/// [`OpError::Format`] when the DLL cannot be read or an image cannot be written.
fn nexus_extract(dll: &Path, out: &Path, sink: &Sink) -> Result<bool, OpError> {
    Ok(carved(nexus::carve(dll, out)?, sink))
}

/// Shows the length and the path of each carved file.
fn carved(files: Vec<(usize, PathBuf)>, sink: &Sink) -> bool {
    for (len, path) in files {
        sink(&format!("{len:>9} {}", path.display()));
    }
    true
}

/// # Errors
/// [`OpError::Format`] when the built-in catalog is not valid.
fn catalog_list(sink: &Sink) -> Result<bool, OpError> {
    for artifact in catalog::builtin()? {
        sink(&format!(
            "{}: {} {}, {} ({} publisher URLs, {} mirror URLs)",
            artifact.id,
            artifact.model,
            artifact.version,
            artifact.file,
            artifact.urls.len(),
            artifact.mirror_urls.len()
        ));
        for image in &artifact.images {
            sink(&format!("  {}", image.path));
        }
    }
    Ok(true)
}

/// # Errors
/// [`OpError::Format`] when the catalog has no such artifact, no source gives it, or a file
/// cannot be written.
fn fetch(id: &str, out: &Path, from: Option<&Path>, sink: &Sink) -> Result<bool, OpError> {
    let artifact = catalog::builtin()?
        .into_iter()
        .find(|artifact| artifact.id == id)
        .ok_or_else(|| {
            FormatError::Invalid(format!("{id}: not in the catalog (see `gsfw catalog`)"))
        })?;
    let mut log = |line: &str| sink(line);
    let files = fetch_artifact(&artifact, from, &mut HttpSource::new(), out, &mut log)?;
    Ok(carved(files, sink))
}

/// # Errors
/// [`OpError::Device`] when the USB devices cannot be listed.
fn list(sink: &Sink) -> Result<bool, OpError> {
    let lines = list_devices()?;
    if lines.is_empty() {
        sink("no USB device with VID 3537");
    }
    for line in lines {
        sink(&line);
    }
    Ok(true)
}

fn say_all(sink: &Sink, text: &str) {
    for line in text.lines() {
        sink(line);
    }
}

/// # Errors
/// [`OpError::Format`] when the image cannot be read.
fn info(path: &Path, sink: &Sink) -> Result<bool, OpError> {
    let report = ufw::info(path)?;
    say_all(sink, &report.to_string());
    let image = ufw::load(path)?;
    for entry in &image.entries {
        if is_flash_bin(&entry.name) {
            let raw = image.raw(entry);
            let head = parse_flash_head(raw.get(..HEAD_LEN).unwrap_or(raw), 0, 0)?;
            sink(&format!(
                "{} head: crc ok {}, flash size {:#x}",
                entry.name, head.header_ok, head.size
            ));
            for dir in &head.entries {
                sink(&format!("  {:16} addr {:#08x}", dir.name, dir.addr));
            }
        }
    }
    Ok(report.ok())
}

/// # Errors
/// [`OpError`] when the image fails its header CRC.
fn load_image(path: &Path) -> Result<ufw::Ufw, OpError> {
    let image = ufw::load(path)?;
    if !image.hdr_ok {
        return Err(DeviceError::Pad(format!("{}: header CRC fails", path.display())).into());
    }
    Ok(image)
}

/// A fixed packet tag, so that each dry run shows the same bytes.
const DRY_RUN_TAG: u32 = 0x1234_5678;

/// # Errors
/// [`OpError`] when the image has no chip key.
fn dry_run(path: &Path, sink: &Sink) -> Result<bool, OpError> {
    let image = load_image(path)?;
    let chipkey = image
        .chipkey
        .ok_or_else(|| DeviceError::Pad("image has no chip key; C1 needs it".to_owned()))?;
    let host_rand: [u8; RAND_LEN] = core::array::from_fn(|i| u8::try_from(i).unwrap_or_default());
    let packets = [
        ("C0 handshake", c0_handshake(&host_rand)),
        (
            "C1 query (scrambled with the session key; shown with TOOL_ID)",
            c1_query(chipkey),
        ),
    ];
    for (label, body) in packets {
        sink(&format!("# {label}"));
        for frag in fragments(&build(&body, TOOL_ID, Some(DRY_RUN_TAG))?) {
            sink(&hex(&gip_wrap(&frag, 1, GIP_FLAGS), " "));
        }
    }
    Ok(true)
}

/// # Errors
/// [`OpError`] when the image or the pad fails.
fn pad(path: &Path, action: &PadAction, opts: &LinkOptions, sink: &Sink) -> Result<bool, OpError> {
    let image = load_image(path)?;
    let trace = opts.verbose.then(|| {
        let sink = Rc::clone(sink);
        let trace: crate::usb::Trace = Box::new(move |line: &str| sink(line));
        trace
    });
    let mut link = UsbGipLink::open(opts.pid, opts.flags, opts.power_on, trace)?;
    sink(&link.name);
    let mut log = |line: &str| sink(line);
    let mut s = open_session(&mut link, &image, &mut log, opts.session_key, &random16())?;
    match *action {
        PadAction::Probe => {}
        PadAction::Crc { addr, len } => {
            let crc = device_crc(&mut s, addr, len)?;
            (s.log)(&format!("C5 {addr:#x}+{len:#x}: crc {crc:#06x}"));
            if let Some(entry) = image.entry("flash.bin") {
                let raw = image.raw(entry);
                let from = crate::bytes::index(addr);
                let want = crate::bytes::index(len);
                if let Some(same) = raw.get(from..).and_then(|tail| tail.get(..want)) {
                    (s.log)(&format!(
                        "  flash.bin crc over the same range: {:#06x}",
                        crc16(same)
                    ));
                }
            }
        }
        PadAction::Flash {
            write,
            keep_region_a,
        } => flash(&mut s, write, keep_region_a)?,
    }
    Ok(true)
}
