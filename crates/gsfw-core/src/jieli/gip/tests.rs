use super::{GIP_FLAGS, Reassembler, fragments, gip_unwrap, gip_wrap, is_ack, reply_fragment};
use crate::bytes::window;

fn packet(mul: usize) -> Vec<u8> {
    (0..512_usize)
        .map(|i| u8::try_from(i.wrapping_mul(mul) & 0xFF).unwrap())
        .collect()
}

#[test]
fn split() {
    let pkt = packet(1);
    let frags = fragments(&pkt);
    assert_eq!(frags.len(), 10, "fragment count");
    assert_eq!(frags[0][..4], [0x4A, 0x0A, 1, 0x34], "first header");
    assert_eq!(frags[9][..4], [0x4A, 0x0A, 10, 0x2C], "last header");
    assert!(frags[9][4 + 0x2C..].iter().all(|&b| b == 0), "zero padding");
    let joined: Vec<u8> = frags
        .iter()
        .flat_map(|f| window(f, 4, f[3].into()).iter().copied())
        .collect();
    assert_eq!(joined, pkt, "data round trip");
}

#[test]
fn reassemble_out_of_order() {
    let pkt = packet(7);
    let mut rs = Reassembler::new();
    let out: Vec<Option<Vec<u8>>> = fragments(&pkt).iter().rev().map(|f| rs.feed(f)).collect();
    assert!(out[..9].iter().all(Option::is_none), "incomplete");
    assert_eq!(out[9].as_deref(), Some(&pkt[..]), "complete");
    assert_eq!(
        rs.feed(&[0x4C, 0x0A, 1]),
        None,
        "an ack is not a data fragment"
    );
}

#[test]
fn reassemble_drops_bad_fragments() {
    let mut rs = Reassembler::new();
    assert_eq!(rs.feed(&[0x4A, 1, 0, 0]), None, "index 0");
    assert_eq!(rs.feed(&[0x4A, 1, 2, 0]), None, "index past total");
    assert_eq!(rs.feed(&[0x4A, 1, 1, 3, 9, 9]), None, "length past data");
    assert_eq!(
        rs.feed(&[0x4A, 1, 1, 2, 9, 8, 7]),
        Some(vec![9, 8]),
        "one fragment"
    );
}

#[test]
fn ack() {
    assert!(is_ack(&[0x4C, 0x0A, 3, 0], 3), "ack of 3");
    assert!(!is_ack(&[0x4C, 0x0A, 3], 4), "other index");
    assert!(!is_ack(&[0x4C, 0x0A], 3), "short");
}

#[test]
fn wrap_and_unwrap() {
    let frag = fragments(&[0; 512])[0];
    let msg = gip_wrap(&frag, 5, GIP_FLAGS);
    assert_eq!(
        msg[..8],
        [0x0F, 0x00, 5, 0x3C, 0xF0, 0, 0, 0],
        "header and prefix"
    );
    assert_eq!(msg.len(), 4 + 0x3C, "message length");
    let (cmd, _, seq, payload) = gip_unwrap(&msg).unwrap();
    assert_eq!((cmd, seq, &payload[4..]), (0x0F, 5, &frag[..]), "unwrap");
    let reply = [&[0xF1, 0, 0, 0][..], &frag].concat();
    assert_eq!(reply_fragment(&reply), Some(&frag[..]), "reply fragment");
    assert_eq!(reply_fragment(payload), None, "F0 is host-to-device");
    assert_eq!(gip_unwrap(&[0x0F, 0, 0, 0x80]), None, "two-byte length");
}
