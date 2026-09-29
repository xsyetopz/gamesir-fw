use std::path::PathBuf;

use gsfw_core::jieli::GIP_FLAGS;
use gsfw_core::ops::{LinkOptions, Op, PadAction};

use super::{LINK_HELP, Parsed, parse};

fn run(argv: &[&str]) -> Result<Parsed, String> {
    parse(&argv.iter().map(|&a| a.to_owned()).collect::<Vec<_>>())
}

fn pad(action: PadAction, link: LinkOptions) -> Op {
    Op::Pad {
        image: PathBuf::from("x.fw"),
        action,
        link,
    }
}

#[test]
fn flash_without_yes_only_checks() {
    let link = LinkOptions {
        pid: Some(0x1010),
        session_key: Some(0x5A5A),
        ..LinkOptions::default()
    };
    let action = PadAction::Flash {
        write: false,
        keep_region_a: false,
    };
    assert_eq!(
        run(&["flash", "x.fw", "--pid", "1010", "--session-key=0x5a5a"]),
        Ok(Parsed::Run(pad(action, link), true)),
        "checks only"
    );
    let action = PadAction::Flash {
        write: true,
        keep_region_a: true,
    };
    assert_eq!(
        run(&["flash", "--yes", "x.fw", "--keep-region-a"]),
        Ok(Parsed::Run(pad(action, LinkOptions::default()), false)),
        "write"
    );
}

#[test]
fn crc_takes_hex_and_decimal() {
    let link = LinkOptions {
        flags: 0x20,
        power_on: false,
        verbose: true,
        ..LinkOptions::default()
    };
    let action = PadAction::Crc {
        addr: 0x3000,
        len: 4096,
    };
    assert_eq!(
        run(&[
            "crc",
            "x.fw",
            "0x3000",
            "4096",
            "-v",
            "--no-power-on",
            "--flags",
            "0x20"
        ]),
        Ok(Parsed::Run(pad(action, link), false)),
        "crc"
    );
    assert!(run(&["crc", "x.fw", "zz", "1"]).is_err(), "bad number");
    assert!(
        run(&["crc", "x.fw", "1", "1", "--flags", "256"]).is_err(),
        "flags over u8"
    );
}

#[test]
fn file_commands() {
    assert_eq!(
        run(&["app-bin", "x.fw", "app.bin"]),
        Ok(Parsed::Run(
            Op::AppBin {
                image: "x.fw".into(),
                out: "app.bin".into()
            },
            false
        )),
        "app-bin"
    );
    assert_eq!(
        run(&["flash-tool-extract", "tool.exe", "out"]),
        Ok(Parsed::Run(
            Op::FlashToolExtract {
                exe: "tool.exe".into(),
                out: "out".into()
            },
            false
        )),
        "flash-tool-extract"
    );
    assert_eq!(run(&["list"]), Ok(Parsed::Run(Op::List, false)), "list");
}

#[test]
fn help_version_and_errors() {
    assert_eq!(run(&[]), Ok(Parsed::Help), "no args");
    assert_eq!(run(&["info", "--help"]), Ok(Parsed::Help), "help");
    assert_eq!(run(&["-V"]), Ok(Parsed::Version), "version");
    assert!(run(&["info"]).is_err(), "missing image");
    assert!(
        run(&["info", "x.fw", "--pid", "1"]).is_err(),
        "pad option on a file command"
    );
    assert!(run(&["flash", "x.fw", "--pid"]).is_err(), "missing value");
    assert!(run(&["frob"]).is_err(), "unknown command");
}

#[test]
fn fetch_takes_an_optional_file() {
    let fetch = |from: Option<&str>| {
        Ok(Parsed::Run(
            Op::Fetch {
                artifact: "g7se".to_owned(),
                out: "out".into(),
                from: from.map(PathBuf::from),
            },
            false,
        ))
    };
    assert_eq!(run(&["fetch", "g7se", "out"]), fetch(None), "no file");
    assert_eq!(
        run(&["fetch", "--from", "t.exe", "g7se", "out"]),
        fetch(Some("t.exe")),
        "file"
    );
    assert_eq!(
        run(&["fetch", "g7se", "out", "--from=t.exe"]),
        fetch(Some("t.exe")),
        "file, inline"
    );
    assert!(
        run(&["fetch", "g7se", "out", "--from"]).is_err(),
        "no value"
    );
    assert!(
        run(&["info", "x.fw", "--from", "t.exe"]).is_err(),
        "--from on another command"
    );
    assert_eq!(
        run(&["catalog"]),
        Ok(Parsed::Run(Op::Catalog, false)),
        "catalog"
    );
}

#[test]
fn help_shows_the_default_flags() {
    assert!(
        LINK_HELP.contains(&format!("default {GIP_FLAGS:#04x}")),
        "{LINK_HELP}"
    );
}
