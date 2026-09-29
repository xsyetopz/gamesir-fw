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
fn confirmation_names_the_image_and_the_target() {
    let one = confirm_text(&flash(true, Some(0x1010)));
    assert!(one.contains("x.fw"), "image: {one}");
    assert!(one.contains("1010"), "product ID: {one}");
    let any = confirm_text(&flash(true, None));
    assert!(any.contains("x.fw"), "image: {any}");
    assert!(!any.contains("1010"), "no product ID: {any}");
}
