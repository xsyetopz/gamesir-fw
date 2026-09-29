use super::{TOOL_ID, build, unscramble};
use crate::formats::{crc16, enc};
use crate::jieli::{c0_handshake, c2_result};

#[test]
fn header_crc_constant() {
    // 0x24a0 folds the first two header bytes (u16 1 at [6]) into the constant 0x3310/^3, which
    // is the CRC state after 01 00; crc16(pkt[6:16]) must reproduce that fold.
    assert_eq!(crc16(b"\x01\x00"), 0x3331, "folded constant");
}

#[test]
fn layout_and_scramble() {
    let rand: [u8; 16] = core::array::from_fn(|i| u8::try_from(i).unwrap());
    let body = c0_handshake(&rand);
    let pkt = build(&body, TOOL_ID, Some(0x1122_3344)).unwrap();
    assert_eq!(pkt.len(), 512, "packet length");
    assert_eq!(pkt[0], b'J' ^ 0x11, "first byte XOR key low byte (0x1ab0)");
    let plain = enc(&pkt, TOOL_ID);
    assert_eq!(&plain[..4], b"JSUD", "magic");
    assert_eq!(&plain[6..8], &1_u16.to_le_bytes(), "version");
    assert_eq!(&plain[8..10], &crc16(&body).to_le_bytes(), "body CRC");
    assert_eq!(&plain[10..12], &0x18_u16.to_le_bytes(), "body length");
    assert_eq!(&plain[12..16], &0x1122_3344_u32.to_le_bytes(), "tag");
    assert_eq!(plain[0x10], 0xC0, "command");
    assert_eq!(&plain[0x14..0x18], b"HJX1", "SDK id");
    assert_eq!(plain[0x18..0x28], rand, "host random");
    assert!(plain[0x28..].iter().all(|&b| b == 0), "zero tail");
    assert_eq!(unscramble(&pkt, TOOL_ID).unwrap(), plain, "unscramble");
}

#[test]
fn unscramble_rejects() {
    let mut pkt = build(&c2_result(), 0x1234, None).unwrap();
    assert!(unscramble(&pkt, 0x1235).is_err(), "wrong key");
    pkt[0x11] ^= 1; // body byte
    assert!(unscramble(&pkt, 0x1234).is_err(), "body CRC");
}
