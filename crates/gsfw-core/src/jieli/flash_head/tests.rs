use super::{FLASH_HEAD_KEY, parse_flash_head};
use crate::bytes::put;
use crate::formats::{crc16, enc};
use crate::jieli::{build, parse_c1, unscramble};

/// A scrambled 0x100-byte flash head: header with `size` and erase unit byte `unit`, then
/// seven directory entries, the unused ones 0xFF-filled.
pub(in crate::jieli) fn head_bytes(size: u32, unit: u8, dirs: &[(&str, u32)]) -> Vec<u8> {
    let mut header = [0; 32];
    put(&mut header, 8, &size.to_le_bytes());
    header[13] = unit;
    let crc = crc16(&header[2..]);
    put(&mut header, 0, &crc.to_le_bytes());
    let mut out = enc(&header, FLASH_HEAD_KEY);
    for slot in 0..7 {
        let mut entry = [0xFF; 32];
        if let Some(&(name, addr)) = dirs.get(slot) {
            put(&mut entry, 8, &addr.to_le_bytes());
            put(&mut entry, 16, &[0; 16]);
            put(&mut entry, 16, name.as_bytes());
        }
        out.extend(enc(&entry, FLASH_HEAD_KEY));
    }
    out
}

const G7SE_DIRS: [(&str, u32); 3] = [
    ("app_dir_head", 0x1B00),
    ("app_dir_head2", 0x3ED00),
    ("other", 0x100),
];

#[test]
fn fields_and_app_dirs() {
    let head = parse_flash_head(&head_bytes(0x7FF00, 0x10, &G7SE_DIRS), 2, 1).unwrap();
    assert!(head.header_ok, "header CRC");
    assert_eq!(head.size, 0x7FF00, "size");
    assert_eq!(head.unit_byte(), 0x10, "unit byte");
    let names: Vec<&str> = head.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["app_dir_head", "app_dir_head2", "other"], "entries");
    assert_eq!(head.addr("app_dir_head2"), Some(0x3ED00), "addr");
    assert_eq!(
        head.app_dirs(),
        (Some(0x2B00), Some(0x3FD00)),
        "EOFFSET 1 shift"
    );
}

#[test]
fn app_dirs_shift_and_missing() {
    let bytes = head_bytes(0, 0, &G7SE_DIRS[..1]);
    let dirs = |eoffset| parse_flash_head(&bytes, 0, eoffset).unwrap().app_dirs();
    assert_eq!(dirs(0x10), (Some(0x11B00), None), "EOFFSET 0x10 shift");
    assert_eq!(dirs(7), (Some(0x1B00), None), "unknown EOFFSET: no shift");
}

#[test]
fn last_duplicate_wins() {
    let dirs = [("a", 1), ("b", 2), ("a", 3)];
    let head = parse_flash_head(&head_bytes(0, 0, &dirs), 0, 0).unwrap();
    assert_eq!(head.addr("a"), Some(3), "last a");
    assert_eq!(head.addr("c"), None, "missing");
}

#[test]
fn skips_empty_and_non_ascii_names() {
    let dirs = [("", 1), ("\u{e9}", 2), ("ok", 3)];
    let head = parse_flash_head(&head_bytes(0, 0, &dirs), 0, 0).unwrap();
    let names: Vec<&str> = head.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["ok"], "entries");
}

#[test]
fn bad_crc_and_short_head() {
    let mut bytes = head_bytes(0, 0, &[]);
    bytes[5] ^= 1;
    assert!(
        !parse_flash_head(&bytes, 0, 0).unwrap().header_ok,
        "CRC fails"
    );
    assert!(parse_flash_head(&bytes[..13], 0, 0).is_err(), "too short");
}

#[test]
fn c1_reply_carries_head() {
    let mut body = vec![0; 0x10C];
    put(&mut body, 0, &2_u32.to_le_bytes());
    put(&mut body, 8, &head_bytes(0x7FF00, 0x10, &G7SE_DIRS)); // lands at reply 0x18
    body[0x10A] = 2; // reply 0x11a mode
    body[0x10B] = 1; // reply 0x11b eoffset
    let plain = unscramble(&build(&body, 0x4242, Some(7)).unwrap(), 0x4242).unwrap();
    let (status, head) = parse_c1(&plain).unwrap();
    let head = head.unwrap();
    assert_eq!(
        (status.code, head.mode, head.eoffset, head.header_ok),
        (0, 2, 1, true),
        "C1 fields"
    );
}
