use super::{
    c0_handshake, c1_query, c2_result, c3_erase, c4_write, c5_crc, ca_reset, cb_range, parse_c1,
    parse_crc, session_key,
};
use crate::formats::crc16;
use crate::jieli::{ProtocolError, TOOL_ID, build, unscramble};

/// A device reply as it arrives, descrambled: framed and scrambled like a host packet.
fn reply_plain(body: &[u8], key: u16) -> Vec<u8> {
    unscramble(&build(body, key, Some(7)).unwrap(), key).unwrap()
}

#[test]
fn builders() {
    assert_eq!(c1_query(0xBEEF)[..6], [0xC1, 0, 0, 0, 0xEF, 0xBE], "C1");
    assert_eq!(
        c3_erase(0x2000, 0x1000).unwrap()[4..9],
        [0, 0x20, 0, 0, 2],
        "C3"
    );
    assert!(
        matches!(c3_erase(0, 0x800), Err(ProtocolError::Invalid(_))),
        "unknown unit"
    );
    let c5 = c5_crc(0x1000, 0x80);
    assert_eq!(c5[4..8], 0x1000_u32.to_le_bytes(), "C5 address");
    assert_eq!(c5[8..12], 0x80_u32.to_le_bytes(), "C5 length");
    assert_eq!(cb_range(0x10000, 0x20000)[0], 0xCB, "CB");
    for body in [
        c0_handshake(&[0; 16]),
        c1_query(1),
        c2_result(),
        c5_crc(0, 1),
        ca_reset(),
    ] {
        assert_eq!(body.len(), 0x18, "fixed body length");
    }
}

#[test]
fn c4_write_layout() {
    let data: Vec<u8> = (0..=0xFF).collect();
    let body = c4_write(0x3000, &data).unwrap();
    assert_eq!(body.len(), 0x118, "len + 0x18 (0x3540)");
    assert_eq!(body[4..8], 0x3000_u32.to_le_bytes(), "address");
    assert_eq!(body[8..10], [0, 0], "zero field");
    assert_eq!(body[10..12], crc16(&data).to_le_bytes(), "data CRC");
    assert_eq!(body[12..16], 0x100_u32.to_le_bytes(), "length");
    assert_eq!(body[0x10..0x110], data, "data");
    assert_eq!(body[0x110..], [0; 8], "zero tail");
    assert!(c4_write(0, &[0; 0x101]).is_err(), "too long");
    assert!(c4_write(0, &[]).is_err(), "empty");
}

#[test]
fn session_key_mixes_randoms() {
    let host: [u8; 16] = core::array::from_fn(|i| u8::try_from(i).unwrap());
    let dev: [u8; 16] = core::array::from_fn(|i| u8::try_from(i | 0x40).unwrap());
    let body = [&1_u32.to_le_bytes()[..], &[0; 4], b"HJX1", &dev].concat();
    let plain = reply_plain(&body, TOOL_ID);
    assert_eq!(
        session_key(&host, &plain).unwrap(),
        crc16(&host) ^ crc16(&dev),
        "session key"
    );
    let other = reply_plain(&[&2_u32.to_le_bytes()[..], &[0; 0x14]].concat(), 1);
    assert!(
        matches!(session_key(&host, &other), Err(ProtocolError::BadPacket(_))),
        "not a handshake reply"
    );
}

#[test]
fn status_reply() {
    let body = [
        &2_u32.to_le_bytes()[..],
        &(-3_i32).to_le_bytes(),
        &[0; 0x10],
    ]
    .concat();
    let (status, head) = parse_c1(&reply_plain(&body, 0x55AA)).unwrap();
    assert_eq!((status.code, head), (-3, None), "key mismatch has no head");
    // the pad answers C5 with type 4
    let body = [
        &4_u32.to_le_bytes()[..],
        &0_i32.to_le_bytes(),
        &0xABCD_u16.to_le_bytes(),
        &[0; 0x0E],
    ]
    .concat();
    assert_eq!(
        parse_crc(&reply_plain(&body, 9)).unwrap(),
        (0, 0xABCD),
        "C5 reply"
    );
}
