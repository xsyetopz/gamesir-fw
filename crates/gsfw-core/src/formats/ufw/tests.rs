use super::{ENTRY_LEN, HEADER_LEN, ISD_CONFIG_TYPE, Ufw, find_header_key};
use crate::formats::{crc16, enc, sfc};

fn pad(mut data: Vec<u8>, len: usize) -> Vec<u8> {
    data.resize(len, 0);
    data
}

/// One `isd_config.ini` entry: header and table ENC-keyed with `key`, body SFC with offsets
/// relative to the entry, entry CRC over the padded stored bytes.
fn synthetic_fw(key: u16, plain: &[u8]) -> Vec<u8> {
    let mut padded = plain.to_vec();
    padded.resize(plain.len().next_multiple_of(32), 0xFF);
    let body = sfc(&padded, key, 0);
    let offset = HEADER_LEN.wrapping_add(ENTRY_LEN);
    let u32_of = |n: usize| u32::try_from(n).unwrap().to_le_bytes();
    let record = [
        &ISD_CONFIG_TYPE.to_le_bytes()[..],
        &[0, 0],
        &crc16(&body).to_le_bytes(),
        &[0, 0],
        &u32_of(offset),
        &u32_of(plain.len()),
        &u32_of(padded.len()),
    ]
    .concat();
    let entry = [pad(record, 0x40), pad(b"isd_config.ini".to_vec(), 16)].concat();
    let table = enc(&entry, key);
    let head = [
        &u32_of(offset.wrapping_add(body.len()))[..],
        &1_u16.to_le_bytes(),
        &4_u16.to_le_bytes(),
        &0x200_u32.to_le_bytes(),
        &pad(b"AC695X".to_vec(), 0x30),
    ]
    .concat();
    let head = [&crc16(&table).to_le_bytes()[..], &head].concat();
    let head = [&crc16(&head).to_le_bytes()[..], &head].concat();
    [enc(&head, key), table, body].concat()
}

#[test]
fn header_key_is_chip_key() {
    let plain = [&[b'#'; 32][..], b"PID=TEST;\r\n"].concat();
    let fw = Ufw::parse_fw(synthetic_fw(0x1234, &plain)).unwrap();
    assert!(fw.hdr_ok && fw.list_ok, "header and table CRCs");
    assert_eq!(
        (fw.chip.as_str(), fw.chipkey),
        ("AC695X", Some(0x1234)),
        "chip and key"
    );
    let [entry] = fw.entries.as_slice() else {
        panic!("want one entry, got {}", fw.entries.len());
    };
    assert_eq!(
        (entry.name.as_str(), entry.size, entry.padded),
        ("isd_config.ini", 43, 64),
        "entry fields"
    );
    assert!(fw.entry_ok(entry), "entry CRC");
    assert_eq!(fw.plain(entry), plain, "plaintext");
}

#[test]
fn no_key_image_uses_ffff() {
    let fw = synthetic_fw(0xFFFF, &[b'#'; 40]);
    assert_eq!(find_header_key(&fw).unwrap(), 0xFFFF, "header key");
}
