use std::path::PathBuf;

use gsfw_core::ops::{LinkOptions, Op, PadAction};

use super::{Form, Kind, LinkFields};

#[test]
fn flash_form_matches_the_cli_op() {
    let form = Form {
        kind: Kind::Flash,
        first: " x.fw ".to_owned(),
        link: LinkFields {
            pid: "1010".to_owned(),
            session_key: "0x5a5a".to_owned(),
            ..LinkFields::default()
        },
        ..Form::default()
    };
    let link = LinkOptions {
        pid: Some(0x1010),
        session_key: Some(0x5A5A),
        ..LinkOptions::default()
    };
    let action = PadAction::Flash {
        write: false,
        keep_region_a: false,
    };
    let want = Op::Pad {
        image: PathBuf::from("x.fw"),
        action,
        link,
    };
    assert_eq!(
        form.op(),
        Ok(want),
        "same op as `gsfw flash x.fw --pid 1010 --session-key 0x5a5a`"
    );
}

#[test]
fn crc_form_parses_numbers() {
    let form = Form {
        kind: Kind::Crc,
        first: "x.fw".to_owned(),
        addr: "0x3000".to_owned(),
        len: "4096".to_owned(),
        link: LinkFields {
            flags: String::new(),
            ..LinkFields::default()
        },
        ..Form::default()
    };
    let Ok(Op::Pad { action, link, .. }) = form.op() else {
        panic!("not a pad op");
    };
    assert_eq!(
        action,
        PadAction::Crc {
            addr: 0x3000,
            len: 4096
        },
        "numbers"
    );
    assert_eq!(link, LinkOptions::default(), "empty flags take the default");
    let bad = Form {
        link: LinkFields {
            flags: "256".to_owned(),
            ..LinkFields::default()
        },
        ..form
    };
    assert!(bad.op().is_err(), "flags over u8");
}

#[test]
fn empty_paths_are_errors() {
    let form = Form {
        kind: Kind::AppBin,
        first: "x.fw".to_owned(),
        ..Form::default()
    };
    assert_eq!(
        form.op(),
        Err("Output file is empty".to_owned()),
        "second path"
    );
    let list = Form {
        kind: Kind::List,
        ..Form::default()
    };
    assert_eq!(list.op(), Ok(Op::List), "list needs nothing");
}

/// The op of each kind's CLI command, with `a` and `b` as the two paths, 1 and 2 as the numbers.
fn cli_op(kind: Kind) -> Op {
    let a = || PathBuf::from("a");
    let b = || PathBuf::from("b");
    let pad = |action| Op::Pad {
        image: a(),
        action,
        link: LinkOptions::default(),
    };
    match kind {
        Kind::Catalog => Op::Catalog,
        Kind::Fetch => Op::Fetch {
            artifact: "a".to_owned(),
            out: b(),
            from: None,
        },
        Kind::Info => Op::Info { image: a() },
        Kind::Extract => Op::Extract {
            image: a(),
            out: b(),
        },
        Kind::AppBin => Op::AppBin {
            image: a(),
            out: b(),
        },
        Kind::FindUsb => Op::FindUsb {
            image: a(),
            descdir: b(),
        },
        Kind::NexusExtract => Op::NexusExtract { dll: a(), out: b() },
        Kind::FlashToolExtract => Op::FlashToolExtract { exe: a(), out: b() },
        Kind::DryRun => Op::DryRun { image: a() },
        Kind::List => Op::List,
        Kind::Probe => pad(PadAction::Probe),
        Kind::Crc => pad(PadAction::Crc { addr: 1, len: 2 }),
        Kind::Flash => pad(PadAction::Flash {
            write: false,
            keep_region_a: false,
        }),
    }
}

#[test]
fn each_kind_gives_its_cli_op() {
    for kind in Kind::ALL {
        let form = Form {
            kind,
            first: "a".to_owned(),
            second: "b".to_owned(),
            addr: "1".to_owned(),
            len: "2".to_owned(),
            ..Form::default()
        };
        assert_eq!(form.op(), Ok(cli_op(kind)), "{kind:?}");
    }
}
