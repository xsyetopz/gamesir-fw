use std::path::PathBuf;

use gsfw_core::ops::{LinkOptions, Op, PadAction};

use super::{confirm_text, writes};

fn flash(write: bool, pid: Option<u16>) -> Op {
    Op::Pad {
        image: PathBuf::from("x.fw"),
        action: PadAction::Flash {
            write,
            keep_region_a: false,
        },
        link: LinkOptions {
            pid,
            ..LinkOptions::default()
        },
    }
}

#[test]
fn only_a_writing_flash_asks() {
    assert!(writes(&flash(true, None)), "flash --yes");
    assert!(!writes(&flash(false, None)), "checks only");
    assert!(!writes(&Op::List), "list");
}

#[test]
fn confirmation_names_image_and_pad() {
    let text = confirm_text(&flash(true, Some(0x1010)));
    assert!(
        text.starts_with("Write x.fw to the pad with PID 1010?"),
        "{text}"
    );
    let any = confirm_text(&flash(true, None));
    assert!(any.contains("the first GameSir pad in GIP mode"), "{any}");
}
