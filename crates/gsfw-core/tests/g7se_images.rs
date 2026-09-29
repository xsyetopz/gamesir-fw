//! Decoding and planning against the real G7 SE images under `private/`; skips without them.
#![cfg(test)]

#[path = "common/fixture.rs"]
pub mod fixture;

use fixture::fixture;

use gsfw_core::formats::crc16;
use gsfw_core::formats::ufw::{Ufw, app_bin, load};
use gsfw_core::jieli::{FlashHead, parse_flash_head, plan_flash};

const V664_KEY: &str = "firmware/Core/FirmwarePackages/G7SE/JS_SL3101_V664_Key.ufw";
const V664_NO_KEY: &str = "firmware/Core/FirmwarePackages/G7SE/JS_SL3101_V664_No_Key.ufw";
const V640_KEY: &str = "flash-tool/bundle/JS_SL3101_V640_Key.fw";
const V640_NO_KEY: &str = "flash-tool/bundle/JS_SL3101_V640_No_Key.fw";

fn entry(image: &Ufw, name: &str) -> Vec<u8> {
    image.raw(image.entry(name).unwrap()).to_vec()
}

fn check(image: &Ufw, count: u16, chipkey: u16) {
    assert!(image.hdr_ok && image.list_ok, "header and list CRCs");
    assert_eq!(image.chip, "AC695X", "chip");
    assert_eq!(
        (image.count, image.chipkey),
        (count, Some(chipkey)),
        "count and chip key"
    );
    assert!(
        image.entries.iter().all(|e| image.entry_ok(e)),
        "entry CRCs"
    );
}

#[test]
fn v664_images() {
    let (Some(key), Some(no_key)) = (fixture(V664_KEY), fixture(V664_NO_KEY)) else {
        return;
    };
    let (key, no_key) = (load(&key).unwrap(), load(&no_key).unwrap());
    check(&key, 10, 0x4317);
    check(&no_key, 10, 0xFFFF);
    let app = app_bin(&key).unwrap();
    assert_eq!(app.len(), 0x3_7570, "app.bin length");
    assert_eq!(app, app_bin(&no_key).unwrap(), "same app.bin from both");
}

#[test]
fn v640_images() {
    let (Some(key), Some(no_key)) = (fixture(V640_KEY), fixture(V640_NO_KEY)) else {
        return;
    };
    let (key, no_key) = (load(&key).unwrap(), load(&no_key).unwrap());
    check(&key, 6, 0x4317);
    check(&no_key, 6, 0xFFFF);
    assert_eq!(
        usize::try_from(key.size).unwrap(),
        key.data.len(),
        "size field"
    );
    let app = app_bin(&key).unwrap();
    assert_eq!(app.len(), 0x2_C258, "app.bin length");
    assert_eq!(app, app_bin(&no_key).unwrap(), "same app.bin from both");
}

#[test]
fn v664_flash_head() {
    let Some(path) = fixture(V664_NO_KEY) else {
        return;
    };
    let head = parse_flash_head(&entry(&load(&path).unwrap(), "flash.bin")[..0x100], 0, 1).unwrap();
    assert!(head.header_ok, "head CRC");
    assert_eq!(head.size, 0x7_FF00, "flash size");
    assert_eq!(head.addr("app_dir_head"), Some(0x1B00), "dir 1");
    assert_eq!(head.addr("app_dir_head2"), Some(0x3_ED00), "dir 2");
    assert_eq!(
        head.app_dirs(),
        (Some(0x2B00), Some(0x3_FD00)),
        "dirs after eoffset"
    );
}

/// The pad after the Nexus 6.64 update: C1 gives `flash3.bin`'s head, eoffset 1.
fn device(mode: u8) -> Option<FlashHead> {
    let image = load(&fixture(V664_KEY)?).unwrap();
    Some(parse_flash_head(&entry(&image, "flash3.bin")[..0x100], mode, 1).unwrap())
}

#[test]
fn plan_640_into_bank1() {
    let (Some(dev), Some(path)) = (device(2), fixture(V640_KEY)) else {
        return;
    };
    let flash = entry(&load(&path).unwrap(), "flash.bin");
    let plan = plan_flash(&flash, &dev).unwrap();
    assert_eq!(
        (plan.c_addr, plan.kill_addr, plan.unit),
        (0x3000, 0x3_F000, 0x1000),
        "addresses"
    );
    assert_eq!(plan.region_c, flash[0x2000..], "region C");
    assert_eq!(plan.region_c.len(), 0x2_D000, "region C length");
    assert_eq!(crc16(&plan.region_c), 0xD820, "region C crc");
    assert!(
        plan.region_a[..0x1000].iter().all(|&b| b == 0xFF),
        "region A starts blank"
    );
    // The pad's bank 1 read back 0xb226 over 0x3000+0x1000: this region with 32 bytes zeroed.
    let zeroed = [&[0; 32][..], &plan.region_c[0x20..0x1000]].concat();
    assert_eq!(crc16(&zeroed), 0xB226, "captured bank 1 crc");
}

#[test]
fn plan_mode1_mirrors() {
    let (Some(dev), Some(path)) = (device(1), fixture(V640_KEY)) else {
        return;
    };
    let plan = plan_flash(&entry(&load(&path).unwrap(), "flash.bin"), &dev).unwrap();
    assert_eq!(
        (plan.c_addr, plan.kill_addr),
        (0x3_F000, 0x3000),
        "mirrored"
    );
}

#[test]
fn plan_rejects_other_layout() {
    let (Some(dev), Some(path)) = (device(2), fixture(V664_KEY)) else {
        return;
    };
    assert!(
        plan_flash(&entry(&load(&path).unwrap(), "flash.bin"), &dev).is_err(),
        "0x100 unit variant"
    );
}
