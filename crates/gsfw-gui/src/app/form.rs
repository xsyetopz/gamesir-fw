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
    /// Every operation, in the CLI's order.
    #[cfg(test)]
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

    /// The operation's name in the window.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Catalog => "Firmware list",
            Self::Fetch => "Download firmware",
            Self::NexusExtract => "From the Nexus app",
            Self::FlashToolExtract => "From a Flash Tool",
            Self::Flash => "Install firmware",
            Self::List => "Find controllers",
            Self::Probe => "Check controller",
            Self::Crc => "Read checksum",
            Self::Info => "File details",
            Self::Extract => "Unpack file",
            Self::AppBin => "Save app.bin",
            Self::FindUsb => "Find USB IDs",
            Self::DryRun => "Preview messages",
        }
    }

    /// What the operation does, for the user.
    pub(super) const fn about(self) -> &'static str {
        match self {
            Self::Catalog => "Shows the firmware files that this tool can download.",
            Self::Fetch => {
                "Downloads the firmware for your controller and checks it. The firmware files go \
                 into the folder that you choose."
            }
            Self::NexusExtract => {
                "Gets the firmware files from the GameSir Nexus app. Choose the file \
                 HJC.GameSir.Nexus2_0.dll from the Nexus app folder."
            }
            Self::FlashToolExtract => {
                "Gets the firmware files from a GameSir Flash Tool program (.exe)."
            }
            Self::Flash => {
                "Installs firmware on the controller. Use this to repair a controller that does \
                 not start. The tool first checks the file and the controller. It writes only \
                 when you select Write firmware."
            }
            Self::List => "Shows the GameSir controllers that are connected by USB.",
            Self::Probe => {
                "Connects to the controller and reads its firmware state. The controller does \
                 not change."
            }
            Self::Crc => {
                "Reads the checksum of an area of the controller's memory. The controller does \
                 not change."
            }
            Self::Info => "Shows the parts of a firmware file.",
            Self::Extract => "Saves each part of a firmware file into a folder.",
            Self::AppBin => "Saves the main program (app.bin) of a firmware file.",
            Self::FindUsb => {
                "Finds the USB IDs, and the descriptor files of a folder, in a firmware file."
            }
            Self::DryRun => {
                "Shows the messages that Check controller sends. The tool sends nothing."
            }
        }
    }

    /// The text of the button that starts the operation.
    pub(super) const fn action(self, write: bool) -> &'static str {
        match self {
            Self::Catalog => "Show list",
            Self::Fetch => "Download",
            Self::NexusExtract | Self::FlashToolExtract => "Get files",
            Self::Flash if write => "Install",
            Self::Flash => "Check only",
            Self::List | Self::FindUsb => "Find",
            Self::Probe => "Check",
            Self::Crc => "Read",
            Self::Info | Self::DryRun => "Show",
            Self::Extract => "Unpack",
            Self::AppBin => "Save",
        }
    }

    /// Labels of the two path fields; `None` when the operation does not take one.
    pub(super) const fn paths(self) -> (Option<&'static str>, Option<&'static str>) {
        const FILE: Option<&str> = Some("Firmware file");
        const FOLDER: Option<&str> = Some("Save to folder");
        match self {
            Self::Info | Self::DryRun | Self::Probe | Self::Crc | Self::Flash => (FILE, None),
            Self::Extract => (FILE, FOLDER),
            Self::AppBin => (FILE, Some("Save as")),
            Self::FindUsb => (FILE, Some("Descriptor folder")),
            Self::NexusExtract => (Some("Nexus app file"), FOLDER),
            Self::FlashToolExtract => (Some("Flash Tool"), FOLDER),
            Self::Fetch => (Some("Firmware"), FOLDER),
            Self::List | Self::Catalog => (None, None),
        }
    }

    /// Whether the operation talks to a pad through the upgrade protocol.
    pub(super) const fn pad(self) -> bool {
        matches!(self, Self::Probe | Self::Crc | Self::Flash)
    }
}

/// A group of operations in the side panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Group {
    Get,
    Repair,
    Controller,
    Files,
}

impl Group {
    /// The side panel's order: get firmware, install it, then the rest.
    pub(super) const ALL: [Self; 4] = [Self::Get, Self::Repair, Self::Controller, Self::Files];

    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Get => "Get firmware",
            Self::Repair => "Repair",
            Self::Controller => "Controller",
            Self::Files => "Firmware files",
        }
    }

    pub(super) const fn kinds(self) -> &'static [Kind] {
        match self {
            Self::Get => &[
                Kind::Catalog,
                Kind::Fetch,
                Kind::NexusExtract,
                Kind::FlashToolExtract,
            ],
            Self::Repair => &[Kind::Flash],
            Self::Controller => &[Kind::List, Kind::Probe, Kind::Crc],
            Self::Files => &[
                Kind::Info,
                Kind::Extract,
                Kind::AppBin,
                Kind::FindUsb,
                Kind::DryRun,
            ],
        }
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
        let flags = optional(&self.flags, int_u32)
            .and_then(|flags| {
                flags
                    .map(u8::try_from)
                    .transpose()
                    .map_err(|e| e.to_string())
            })
            .map_err(|err| format!("Message flags: {err}"))?;
        Ok(LinkOptions {
            pid: optional(&self.pid, hex_u16).map_err(|err| format!("USB product ID: {err}"))?,
            flags: flags.unwrap_or(GIP_FLAGS),
            power_on: self.power_on,
            session_key: optional(&self.session_key, hex_u16)
                .map_err(|err| format!("Session key: {err}"))?,
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
            kind: Kind::Fetch,
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
        return Err(format!("Fill in {label}."));
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
                addr: int_u32(self.addr.trim()).map_err(|err| format!("Start address: {err}"))?,
                len: int_u32(self.len.trim()).map_err(|err| format!("Length: {err}"))?,
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
