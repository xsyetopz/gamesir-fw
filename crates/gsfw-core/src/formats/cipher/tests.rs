use super::{crc16, enc, sfc};

#[test]
fn crc16_xmodem_check_value() {
    // Published check value of CRC-16/XMODEM (poly 0x1021, init 0, no reflection).
    assert_eq!(crc16(b"123456789"), 0x31C3, "check value");
}

#[test]
fn enc_first_bytes() {
    // Key 0xFFFF: 0xFF, then (0xFFFF<<1)^0x1021 = 0xEFDF -> 0xDF, then 0xCF9F -> 0x9F.
    assert_eq!(enc(&[0; 3], 0xFFFF), [0xFF, 0xDF, 0x9F], "keystream start");
}

#[test]
fn enc_is_involution() {
    let data: Vec<u8> = (0..64).collect();
    assert_eq!(enc(&enc(&data, 0x4317), 0x4317), data, "enc twice");
}

#[test]
fn sfc_rekeys_each_block() {
    let out = sfc(&[0; 64], 0x1234, 0x100);
    assert_eq!(
        out[..32],
        enc(&[0; 32], 0x1234 ^ (0x100 >> 2)),
        "first block"
    );
    assert_eq!(
        out[32..],
        enc(&[0; 32], 0x1234 ^ (0x120 >> 2)),
        "second block"
    );
}
