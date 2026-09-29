//! Parses the command line into a [`gsfw_core::ops::Op`] and runs it; the GUI runs the same
//! operations.

extern crate alloc;

use alloc::rc::Rc;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use gsfw_core::ops::{LinkOptions, Op, PadAction, Sink, hex_u16, int_u32, run as run_op};

/// Options every pad command takes.
const LINK_HELP: &str = "\
  --pid HEX           USB PID (default: any 3537 device with GIP)
  --flags N           GIP header flags (default 0x00)
  --no-power-on       skip the xpad power-on message
  -v, --verbose       print every USB message
  --session-key HEX   skip C0 and reuse this session key";

const HELP: &str = "\
gsfw: decode GameSir (JieLi AC695X) firmware images and flash them over the GIP upgrade protocol.

Usage: gsfw COMMAND [ARGS] [OPTIONS]

Firmware downloads:
  catalog                    the firmware downloads that fetch knows
  fetch ARTIFACT OUT [--from FILE]
                             get ARTIFACT, check its SHA-256, and write it and its images into
                             OUT. --from FILE uses a downloaded copy and no URL.

Files only:
  info IMAGE                 container entries, CRCs, chip key; flash heads of flash*.bin
  extract IMAGE OUT          write every entry, decrypted, into OUT
  app-bin IMAGE OUT          write the decrypted app.bin
  find-usb IMAGE DESCDIR     find the VID/PID and the descriptor files of DESCDIR in IMAGE
  nexus-extract DLL OUT      carve the .ufw images out of HJC.GameSir.Nexus2_0.dll
  flash-tool-extract EXE OUT write the files of a GameSir Flash Tool .exe (.fw images) into OUT
  dry-run IMAGE              print the GIP messages `probe` would send (fixed random and tag)

Pad, read only:
  list                       USB devices with VID 3537, their interfaces and endpoints
  probe IMAGE                handshake (C0) and query (C1, needs the image's chip key)
  crc IMAGE ADDR LEN         handshake, C1, then a CRC read (C5) of a flash range

Pad, writes flash:
  flash IMAGE [--yes] [--keep-region-a]
                             region C of the image into the bank the pad is not running, zero
                             the running bank's dir head, C2. Without --yes only the checks run.
                             --keep-region-a flashes even though the device's region A differs.

Pad options (probe, crc, flash):
";

const NOTE: &str = "
ADDR and LEN are decimal or 0x hex. The pad must be in GIP mode (Xbox class interface
ff/47/d0). Windows: bind that interface to WinUSB with Zadig. Linux: root or a udev rule.";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Help,
    Version,
    /// The operation, and whether it is a flash without `--yes`.
    Run(Op, bool),
}

/// A command's positional arguments, and whether it takes the pad options.
fn spec(name: &str) -> Option<(&'static [&'static str], bool)> {
    Some(match name {
        "info" | "dry-run" => (&["IMAGE"], false),
        "extract" | "app-bin" => (&["IMAGE", "OUT"], false),
        "find-usb" => (&["IMAGE", "DESCDIR"], false),
        "nexus-extract" => (&["DLL", "OUT"], false),
        "flash-tool-extract" => (&["EXE", "OUT"], false),
        "list" | "catalog" => (&[], false),
        "fetch" => (&["ARTIFACT", "OUT"], false),
        "probe" | "flash" => (&["IMAGE"], true),
        "crc" => (&["IMAGE", "ADDR", "LEN"], true),
        _ => return None,
    })
}

/// The options after the command; returns the positional arguments.
///
/// # Errors
/// The message for an unknown option or a bad value.
fn options(
    name: &str,
    args: &[String],
    link: &mut LinkOptions,
    flags: &mut [(&str, bool)],
    from: &mut Option<PathBuf>,
) -> Result<Vec<String>, String> {
    let (_, pad) = spec(name).ok_or_else(|| format!("unknown command {name}"))?;
    let mut positional = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let (key, inline) = match arg.split_once('=') {
            Some((key, value)) if key.starts_with("--") => (key, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| rest.next().cloned())
                .ok_or_else(|| format!("{key} needs a value"))
        };
        if let Some(slot) = flags.iter_mut().find(|(flag, _)| *flag == key) {
            slot.1 = true;
            continue;
        }
        match key {
            "--pid" if pad => link.pid = Some(hex_u16(&value()?)?),
            "--flags" if pad => {
                link.flags =
                    u8::try_from(int_u32(&value()?)?).map_err(|err| format!("--flags: {err}"))?;
            }
            "--no-power-on" if pad => link.power_on = false,
            "-v" | "--verbose" if pad => link.verbose = true,
            "--session-key" if pad => link.session_key = Some(hex_u16(&value()?)?),
            "--from" if name == "fetch" => *from = Some(PathBuf::from(value()?)),
            _ if key.starts_with('-') && key.len() > 1 => {
                return Err(format!("{name}: unknown option {key}"));
            }
            _ => positional.push(arg.clone()),
        }
    }
    Ok(positional)
}

/// Parses the arguments after the program name.
///
/// # Errors
/// The message for a bad command line.
fn parse(args: &[String]) -> Result<Parsed, String> {
    let Some((name, rest)) = args.split_first() else {
        return Ok(Parsed::Help);
    };
    match name.as_str() {
        "-h" | "--help" | "help" => return Ok(Parsed::Help),
        "-V" | "--version" => return Ok(Parsed::Version),
        _ => {}
    }
    if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Ok(Parsed::Help);
    }
    let (names, _) = spec(name).ok_or_else(|| format!("unknown command {name}"))?;
    let mut link = LinkOptions::default();
    let flash_flags: &mut [(&str, bool)] = &mut [("--yes", false), ("--keep-region-a", false)];
    let flags: &mut [(&str, bool)] = if name == "flash" {
        flash_flags
    } else {
        &mut []
    };
    let mut from = None;
    let positional = options(name, rest, &mut link, flags, &mut from)?;
    if positional.len() != names.len() {
        return Err(format!("usage: gsfw {name} {}", names.join(" ")));
    }
    build(name, &positional, &link, flags, from)
}

/// The operation for the command `name` and its checked arguments. A separate function: it keeps
/// the frame of [`parse`] under the stack limit on Windows.
///
/// # Errors
/// The message for a bad number in the `crc` arguments.
fn build(
    name: &str,
    positional: &[String],
    link: &LinkOptions,
    flags: &[(&str, bool)],
    from: Option<PathBuf>,
) -> Result<Parsed, String> {
    let path = |i: usize| positional.get(i).map(PathBuf::from).unwrap_or_default();
    let flag = |f: &str| flags.iter().any(|&(name, set)| name == f && set);
    let pad = |action| Op::Pad {
        image: path(0),
        action,
        link: link.clone(),
    };
    let op = match name {
        "info" => Op::Info { image: path(0) },
        "dry-run" => Op::DryRun { image: path(0) },
        "extract" => Op::Extract {
            image: path(0),
            out: path(1),
        },
        "app-bin" => Op::AppBin {
            image: path(0),
            out: path(1),
        },
        "find-usb" => Op::FindUsb {
            image: path(0),
            descdir: path(1),
        },
        "nexus-extract" => Op::NexusExtract {
            dll: path(0),
            out: path(1),
        },
        "flash-tool-extract" => Op::FlashToolExtract {
            exe: path(0),
            out: path(1),
        },
        "catalog" => Op::Catalog,
        "fetch" => Op::Fetch {
            artifact: positional.first().cloned().unwrap_or_default(),
            out: path(1),
            from,
        },
        "probe" => pad(PadAction::Probe),
        "crc" => {
            let number = |i: usize| int_u32(positional.get(i).map_or("", String::as_str));
            pad(PadAction::Crc {
                addr: number(1)?,
                len: number(2)?,
            })
        }
        "flash" => {
            let write = flag("--yes");
            let action = PadAction::Flash {
                write,
                keep_region_a: flag("--keep-region-a"),
            };
            return Ok(Parsed::Run(pad(action), !write));
        }
        _ => Op::List,
    };
    Ok(Parsed::Run(op, false))
}

/// Writes one line; a closed stdout (`gsfw list | head`) leaves nowhere to report the error.
fn say(out: &mut dyn io::Write, line: &str) {
    if writeln!(out, "{line}").is_err() {
        // Nowhere left to report it.
    }
}

/// Runs the command line.
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (op, checks_only) = match parse(&args) {
        Ok(Parsed::Run(op, checks_only)) => (op, checks_only),
        Ok(Parsed::Help) => {
            say(
                &mut io::stdout().lock(),
                &format!("{HELP}{LINK_HELP}\n{NOTE}"),
            );
            return ExitCode::SUCCESS;
        }
        Ok(Parsed::Version) => {
            say(
                &mut io::stdout().lock(),
                concat!("gsfw ", env!("CARGO_PKG_VERSION")),
            );
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            say(
                &mut io::stderr().lock(),
                &format!("gsfw: {err}\nTry `gsfw --help`."),
            );
            return ExitCode::from(2);
        }
    };
    let sink: Sink = Rc::new(|line: &str| say(&mut io::stdout().lock(), line));
    match run_op(&op, &sink) {
        Ok(true) => {
            if checks_only {
                sink("add --yes to flash");
            }
            ExitCode::SUCCESS
        }
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            say(&mut io::stderr().lock(), &format!("gsfw: {err}"));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
