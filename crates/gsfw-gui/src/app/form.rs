//! The window's inputs, turned into the same [`Op`] the CLI builds.

use std::path::PathBuf;

use gsfw_core::jieli::GIP_FLAGS;
use gsfw_core::ops::{LinkOptions, Op, PadAction, hex_u16, int_u32};

/// One operation per CLI command, in the CLI's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Catalog,
    Fetch,
    Info,
    Extract,
    AppBin,
    FindUsb,
    NexusExtract,
    FlashToolExtract,
    DryRun,
    List,
    Probe,
    Crc,
    Flash,
}

impl Kind {
    pub(super) const ALL: [Self; 13] = [
        Self::Catalog,
        Self::Fetch,
        Self::Info,
        Self::Extract,
        Self::AppBin,
        Self::FindUsb,
        Self::NexusExtract,
        Self::FlashToolExtract,
        Self::DryRun,
        Self::List,
        Self::Probe,
        Self::Crc,
        Self::Flash,
    ];

    /// The CLI command name and what it does.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Catalog => "catalog: the firmware downloads that fetch knows",
            Self::Fetch => "fetch: get a catalog download and write its images",
            Self::Info => "info: entries, CRCs, chip key, flash heads",
            Self::Extract => "extract: every entry, decrypted",
            Self::AppBin => "app-bin: the decrypted app.bin",
            Self::FindUsb => "find-usb: VID/PID and descriptors in the image",
            Self::NexusExtract => "nexus-extract: .ufw images from the Nexus DLL",
            Self::FlashToolExtract => "flash-tool-extract: .fw images from a Flash Tool .exe",
            Self::DryRun => "dry-run: the GIP messages probe would send",
            Self::List => "list: USB devices with VID 3537",
            Self::Probe => "probe: handshake and query (pad, read only)",
            Self::Crc => "crc: CRC of a flash range (pad, read only)",
            Self::Flash => "flash: write the image (pad, writes flash)",
        }
    }

    /// Labels of the two path fields; `None` when the operation does not take one.
    pub(super) const fn paths(self) -> (Option<&'static str>, Option<&'static str>) {
        match self {
            Self::Info | Self::DryRun | Self::Probe | Self::Crc | Self::Flash => {
                (Some("Image (.ufw, .fw)"), None)
            }
            Self::Extract => (Some("Image (.ufw, .fw)"), Some("Output directory")),
            Self::AppBin => (Some("Image (.ufw, .fw)"), Some("Output file")),
            Self::FindUsb => (Some("Image (.ufw, .fw)"), Some("Descriptor directory")),
            Self::NexusExtract => (Some("HJC.GameSir.Nexus2_0.dll"), Some("Output directory")),
            Self::FlashToolExtract => (Some("Flash Tool .exe"), Some("Output directory")),
            Self::Fetch => (Some("Artifact (see catalog)"), Some("Output directory")),
            Self::List | Self::Catalog => (None, None),
        }
    }

    /// Whether the operation talks to a pad through the upgrade protocol.
    pub(super) const fn pad(self) -> bool {
        matches!(self, Self::Probe | Self::Crc | Self::Flash)
    }
}

/// The pad options (the CLI's link options) as typed.
#[derive(Clone, Debug)]
pub(super) struct LinkFields {
    pub(super) pid: String,
    pub(super) flags: String,
    pub(super) power_on: bool,
    pub(super) session_key: String,
    pub(super) verbose: bool,
}

impl Default for LinkFields {
    fn default() -> Self {
        Self {
            pid: String::new(),
            flags: format!("{GIP_FLAGS:#04x}"),
            power_on: true,
            session_key: String::new(),
            verbose: false,
        }
    }
}

impl LinkFields {
    /// # Errors
    /// The message for a bad number.
    fn options(&self) -> Result<LinkOptions, String> {
        let flags = optional(&self.flags, int_u32)?
            .map(|flags| u8::try_from(flags).map_err(|err| format!("flags: {err}")))
            .transpose()?;
        Ok(LinkOptions {
            pid: optional(&self.pid, hex_u16)?,
            flags: flags.unwrap_or(GIP_FLAGS),
            power_on: self.power_on,
            session_key: optional(&self.session_key, hex_u16)?,
            verbose: self.verbose,
        })
    }
}

/// The window's inputs as typed.
#[derive(Clone, Debug)]
pub(super) struct Form {
    pub(super) kind: Kind,
    pub(super) first: String,
    pub(super) second: String,
    pub(super) addr: String,
    pub(super) len: String,
    /// A downloaded copy for `fetch`; empty means the catalog URLs.
    pub(super) from: String,
    pub(super) link: LinkFields,
    pub(super) write: bool,
    pub(super) keep_region_a: bool,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            kind: Kind::Info,
            first: String::new(),
            second: String::new(),
            addr: String::new(),
            len: String::new(),
            from: String::new(),
            link: LinkFields::default(),
            write: false,
            keep_region_a: false,
        }
    }
}

/// # Errors
/// The message when `text` is empty.
fn path(text: &str, label: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(PathBuf::from(text))
}

/// # Errors
/// The message for a bad number.
fn optional<T>(text: &str, parse: fn(&str) -> Result<T, String>) -> Result<Option<T>, String> {
    let text = text.trim();
    if text.is_empty() {
        Ok(None)
    } else {
        parse(text).map(Some)
    }
}

impl Form {
    /// The operation the inputs describe.
    ///
    /// # Errors
    /// The message for an empty path or a bad number.
    pub(super) fn op(&self) -> Result<Op, String> {
        match self.kind {
            Kind::List => Ok(Op::List),
            Kind::Catalog => Ok(Op::Catalog),
            Kind::Probe | Kind::Crc | Kind::Flash => self.pad_op(),
            _ => self.file_op(),
        }
    }

    /// The first path field.
    ///
    /// # Errors
    /// The message when the field is empty.
    fn first(&self) -> Result<PathBuf, String> {
        path(&self.first, self.kind.paths().0.unwrap_or("path"))
    }

    /// The second path field.
    ///
    /// # Errors
    /// The message when the field is empty.
    fn second(&self) -> Result<PathBuf, String> {
        path(&self.second, self.kind.paths().1.unwrap_or("path"))
    }

    /// # Errors
    /// The message for an empty path.
    fn file_op(&self) -> Result<Op, String> {
        Ok(match self.kind {
            Kind::Extract => Op::Extract {
                image: self.first()?,
                out: self.second()?,
            },
            Kind::AppBin => Op::AppBin {
                image: self.first()?,
                out: self.second()?,
            },
            Kind::FindUsb => Op::FindUsb {
                image: self.first()?,
                descdir: self.second()?,
            },
            Kind::NexusExtract => Op::NexusExtract {
                dll: self.first()?,
                out: self.second()?,
            },
            Kind::FlashToolExtract => Op::FlashToolExtract {
                exe: self.first()?,
                out: self.second()?,
            },
            Kind::Fetch => Op::Fetch {
                artifact: self.first.trim().to_owned(),
                out: self.second()?,
                from: optional(&self.from, |text| Ok(PathBuf::from(text)))?,
            },
            Kind::DryRun => Op::DryRun {
                image: self.first()?,
            },
            _ => Op::Info {
                image: self.first()?,
            },
        })
    }

    /// # Errors
    /// The message for an empty path or a bad number.
    fn pad_op(&self) -> Result<Op, String> {
        let action = match self.kind {
            Kind::Crc => PadAction::Crc {
                addr: int_u32(self.addr.trim()).map_err(|err| format!("address: {err}"))?,
                len: int_u32(self.len.trim()).map_err(|err| format!("length: {err}"))?,
            },
            Kind::Flash => PadAction::Flash {
                write: self.write,
                keep_region_a: self.keep_region_a,
            },
            _ => PadAction::Probe,
        };
        Ok(Op::Pad {
            image: self.first()?,
            action,
            link: self.link.options()?,
        })
    }
}

#[cfg(test)]
mod tests;
