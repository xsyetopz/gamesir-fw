//! Tests of the Flash Tool carver, and the test Flash Tool that the fetch tests also use.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::{carve, entries, inflate, overlay, target};

/// CRC-16/XMODEM check value of `123456789` (the catalogue of parametrised CRC algorithms).
const CHECK_CRC: u16 = 0x31c3;
/// The one file of the test archive.
pub const BODY: &[u8] = b"123456789";

/// An archive with one file, `BODY`, named `name`, at 0x400.
fn archive(name: &str, crc: u16, size: u32) -> Vec<u8> {
    let mut data = vec![0_u8; 0x400];
    data[..4].copy_from_slice(&[0x2d, 0x43, 0xed, 0xcc]);
    data[4..8].copy_from_slice(&1_u32.to_le_bytes());
    data[0x20..0x22].copy_from_slice(&[0xfe, 0]);
    data[0x22..0x24].copy_from_slice(&crc.to_le_bytes());
    data[0x24..0x28].copy_from_slice(&0x400_u32.to_le_bytes());
    data[0x28..0x2c].copy_from_slice(&size.to_le_bytes());
    data[0x30..][..name.len()].copy_from_slice(name.as_bytes());
    data.extend_from_slice(BODY);
    let total = u32::try_from(data.len()).unwrap();
    data[8..12].copy_from_slice(&total.to_le_bytes());
    data
}

/// A PE file with one 0x200-byte section at 0x200, then the overlay for `archive`: `prefix`,
/// the length `declared` (big-endian), and the zlib stream.
fn exe(prefix: &[u8], archive: &[u8], declared: u32) -> Vec<u8> {
    let mut data = vec![0_u8; 0x400];
    data[..2].copy_from_slice(b"MZ");
    data[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    data[0x80..0x84].copy_from_slice(b"PE\0\0");
    data[0x86..0x88].copy_from_slice(&1_u16.to_le_bytes());
    data[0x94..0x96].copy_from_slice(&0xe0_u16.to_le_bytes());
    let section = 0x80 + 24 + 0xe0;
    data[section + 16..section + 20].copy_from_slice(&0x200_u32.to_le_bytes());
    data[section + 20..section + 24].copy_from_slice(&0x200_u32.to_le_bytes());
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib.write_all(archive).unwrap();
    data.extend_from_slice(prefix);
    data.extend_from_slice(&declared.to_be_bytes());
    data.extend_from_slice(&zlib.finish().unwrap());
    data
}

/// A Flash Tool `.exe` with one file, `BODY`, named `name`.
///
/// # Panics
/// When `name` does not fit in the test archive (0x3d0 bytes).
#[must_use]
pub fn good_exe(name: &str) -> Vec<u8> {
    let archive = archive(name, CHECK_CRC, 9);
    exe(&[0xf8], &archive, u32::try_from(archive.len()).unwrap())
}

/// A fresh directory for one test; `carve` writes below it.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gsfw-flashtool-{test}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn carve_writes_each_file_under_its_stored_path() {
    let dir = scratch("carve");
    fs::write(dir.join("tool.exe"), good_exe("data\\config.ini")).unwrap();

    let written = carve(&dir.join("tool.exe"), &dir.join("out")).unwrap();

    let path = dir.join("out").join("data").join("config.ini");
    assert_eq!(written, vec![(9, path.clone())], "one file reported");
    assert_eq!(fs::read(&path).unwrap(), BODY, "file content");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_crc_mismatch_writes_nothing() {
    let dir = scratch("crc");
    let archive = archive("data\\config.ini", CHECK_CRC ^ 1, 9);
    let tool = exe(&[0xf8], &archive, u32::try_from(archive.len()).unwrap());
    fs::write(dir.join("tool.exe"), tool).unwrap();

    let result = carve(&dir.join("tool.exe"), &dir.join("out"));

    assert!(result.is_err(), "CRC mismatch refused");
    assert!(!dir.join("out").exists(), "nothing written");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_name_that_leaves_the_output_directory_writes_nothing() {
    let dir = scratch("traversal");
    fs::write(dir.join("tool.exe"), good_exe("..\\evil.ini")).unwrap();

    let result = carve(&dir.join("tool.exe"), &dir.join("out"));

    assert!(result.is_err(), "`..` refused");
    assert!(!dir.join("evil.ini").exists(), "nothing written outside");
    assert!(!dir.join("out").exists(), "nothing written inside");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn names_must_be_relative() {
    let out = PathBuf::from("out");
    for name in ["", "\\data\\x", "C:\\x", "data\\..\\x", "data\\\\x"] {
        assert!(target(&out, name).is_err(), "{name:?} refused");
    }
    assert_eq!(
        target(&out, "data/x.fw").unwrap(),
        out.join("data").join("x.fw"),
        "forward slash accepted"
    );
}

#[test]
fn an_entry_past_the_archive_end_is_refused() {
    let data = archive("data\\x", CHECK_CRC, 10);
    assert!(entries(&data).is_err(), "size one byte past the end");
    let data = archive("data\\x", CHECK_CRC, 9);
    assert_eq!(entries(&data).unwrap().len(), 1, "exact end accepted");
}

#[test]
fn the_first_four_archive_bytes_can_have_any_value() {
    // The nine Flash Tools in `notes/facts.md` have nine different values here.
    let mut data = archive("data\\x", CHECK_CRC, 9);
    data[..4].copy_from_slice(&[0x5b, 0x56, 0x5b, 0x74]);
    assert_eq!(entries(&data).unwrap().len(), 1, "one entry");
}

#[test]
fn the_declared_length_must_match_the_stream() {
    let data = archive("data\\x", CHECK_CRC, 9);
    let len = u32::try_from(data.len()).unwrap();
    let ok = exe(&[0xf8], &data, len);
    assert_eq!(
        inflate(overlay(&ok).unwrap()).unwrap(),
        data,
        "exact length"
    );
    for declared in [len - 1, len + 1] {
        let tool = exe(&[0xf8], &data, declared);
        assert!(
            inflate(overlay(&tool).unwrap()).is_err(),
            "declared {declared}, stream {len}"
        );
    }
}

#[test]
fn a_file_that_is_not_pe_has_no_overlay() {
    assert!(overlay(b"PK\x03\x04 not an exe").is_err(), "zip refused");
    let tool = good_exe("x");
    let sections_only = &tool[..0x400];
    assert!(
        overlay(sections_only).is_err(),
        "no data after the sections"
    );
}

#[test]
fn the_stream_is_found_after_any_prefix() {
    let data = archive("data\\x", CHECK_CRC, 9);
    let len = u32::try_from(data.len()).unwrap();
    // Lengths of the prefixes in the nine Flash Tools of `notes/facts.md` include 0, 1 and 14.
    let prefixes: [&[u8]; 3] = [
        &[],
        &[0xf8],
        &[
            0x9d, 0xdb, 0xdf, 0xc2, 0x73, 0xfb, 0x39, 0x98, 0xf0, 0x50, 0x24, 0x07, 0xda, 0xbd,
        ],
    ];
    for prefix in prefixes {
        let tool = exe(prefix, &data, len);
        assert_eq!(
            inflate(overlay(&tool).unwrap()).unwrap(),
            data,
            "prefix of {} bytes",
            prefix.len()
        );
    }
}

#[test]
fn a_zlib_header_in_the_prefix_that_does_not_inflate_is_skipped() {
    let data = archive("data\\x", CHECK_CRC, 9);
    let len = u32::try_from(data.len()).unwrap();
    // A length, then a valid zlib header (RFC 1950), then bytes that are not deflate data.
    let mut decoy = len.to_be_bytes().to_vec();
    decoy.extend_from_slice(&[0x78, 0xda, 0xff, 0xff, 0xff]);

    let tool = exe(&decoy, &data, len);

    assert_eq!(
        inflate(overlay(&tool).unwrap()).unwrap(),
        data,
        "real stream"
    );
}

#[test]
fn the_header_length_must_match_the_archive() {
    let mut data = archive("data\\x", CHECK_CRC, 9);
    data.push(0);
    assert!(
        entries(&data).is_err(),
        "one byte more than the header says"
    );
}
