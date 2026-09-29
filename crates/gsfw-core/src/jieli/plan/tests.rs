use super::{erase_steps, plan_flash, write_chunks};
use crate::jieli::flash_head::tests::head_bytes;
use crate::jieli::{FlashHead, ProtocolError, parse_flash_head};

const DIRS: [(&str, u32); 2] = [("app_dir_head", 0x2000), ("app_dir_head2", 0x3E000)];

/// A `flash.bin` with the G7 SE 6.40 layout: head, then region C from 0x2000.
fn flash(unit: u8, dirs: &[(&str, u32)]) -> Vec<u8> {
    let mut data = head_bytes(0x7FF00, unit, dirs);
    data.resize(0x2000, 0xAA);
    data.extend((0..0x300_u32).map(|i| u8::try_from(i & 0xFF).unwrap()));
    data
}

/// The pad after the Nexus 6.64 update: C1 gives a head with the same dirs, EOFFSET 1.
fn dev(mode: u8, eoffset: u8, dirs: &[(&str, u32)]) -> FlashHead {
    parse_flash_head(&head_bytes(0x7FF00, 0x10, dirs), mode, eoffset).unwrap()
}

fn message(result: Result<super::FlashPlan, ProtocolError>) -> String {
    match result {
        Err(ProtocolError::Invalid(message)) => message,
        other => panic!("want Invalid, got {other:?}"),
    }
}

#[test]
fn erase_steps_sizes() {
    let steps = erase_steps(0x3000, 0x2D000, 0x1000).unwrap();
    let small: Vec<(u32, u32)> = (0x3000..0x10000)
        .step_by(0x1000)
        .map(|a| (a, 0x1000))
        .collect();
    assert_eq!(steps[..13], small, "4K steps up to 64K");
    assert_eq!(
        steps[13..],
        [(0x10000, 0x10000), (0x20000, 0x10000)],
        "64K steps"
    );
    assert_eq!(
        erase_steps(0x100, 0x200, 0x100).unwrap(),
        [(0x100, 0x100), (0x200, 0x100)],
        "device unit"
    );
    assert!(erase_steps(0, 0x100, 0).is_err(), "unit 0");
    assert_eq!(erase_steps(0, 0, 0x100).unwrap(), [], "nothing to erase");
}

#[test]
fn write_chunks_first_last() {
    let data: Vec<u8> = (0..0x280_u32)
        .map(|i| u8::try_from(i & 0xFF).unwrap())
        .collect();
    let chunks = write_chunks(0x3000, &data).unwrap();
    let addrs: Vec<u32> = chunks.iter().map(|&(a, _)| a).collect();
    assert_eq!(addrs, [0x3100, 0x3200, 0x3000], "first chunk last");
    assert_eq!(chunks[1].1.len(), 0x80, "short tail");
    let mut sorted = chunks;
    sorted.sort_unstable();
    let joined: Vec<u8> = sorted
        .iter()
        .flat_map(|&(_, c)| c.iter().copied())
        .collect();
    assert_eq!(joined, data, "data round trip");
    assert_eq!(write_chunks(0, &[]).unwrap(), [], "empty");
    assert!(
        write_chunks(u32::MAX, &[0; 0x101]).is_err(),
        "address overflow"
    );
}

#[test]
fn mode2_into_bank1() {
    let image = flash(0x10, &DIRS);
    let plan = plan_flash(&image, &dev(2, 1, &DIRS)).unwrap();
    assert_eq!(
        (plan.mode, plan.c_addr, plan.kill_addr, plan.unit),
        (2, 0x3000, 0x3F000, 0x1000),
        "placement"
    );
    assert_eq!(plan.region_c, image[0x2000..], "region C");
    assert!(
        plan.region_a[..0x1000].iter().all(|&b| b == 0xFF),
        "EOFFSET prepend"
    );
    assert_eq!(plan.region_a[0x1000..], image[..0x2000], "region A");
}

#[test]
fn mode1_mirrors() {
    let plan = plan_flash(&flash(0x10, &DIRS), &dev(1, 1, &DIRS)).unwrap();
    assert_eq!(
        (plan.c_addr, plan.kill_addr),
        (0x3F000, 0x3000),
        "placement"
    );
}

#[test]
fn rejections_in_order() {
    let image = flash(0x10, &DIRS);
    let mut broken = image.clone();
    broken[4] ^= 1;
    assert_eq!(
        message(plan_flash(&broken, &dev(2, 1, &DIRS))),
        "flash head CRC fails",
        "CRC"
    );
    assert_eq!(
        message(plan_flash(&flash(0x01, &DIRS), &dev(2, 1, &DIRS))),
        "erase unit 0x100, device 0x1000",
        "unit"
    );
    let other = [("app_dir_head", 0x2000), ("app_dir_head2", 0x3F000)];
    assert_eq!(
        message(plan_flash(&image, &dev(2, 1, &other))),
        "directory {'app_dir_head': 8192, 'app_dir_head2': 253952} does not match the device \
         {'app_dir_head': 8192, 'app_dir_head2': 258048}",
        "directory"
    );
    assert_eq!(
        message(plan_flash(&image, &dev(2, 2, &DIRS))),
        "device EOFFSET 0x2 not supported",
        "EOFFSET"
    );
    assert_eq!(
        message(plan_flash(&image, &dev(0, 1, &DIRS))),
        "device mode 0: the DLL writes nothing",
        "mode"
    );
    let tight = [("app_dir_head", 0x2000), ("app_dir_head2", 0x2100)];
    assert_eq!(
        message(plan_flash(&flash(0x10, &tight), &dev(2, 1, &tight))),
        "region C 0x300 at 0x3000 passes 0x3100",
        "region C"
    );
}
