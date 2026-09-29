use std::fs;
use std::path::PathBuf;

use super::{Source, check_images, fetch, obtain, sha256_hex};
use crate::catalog::Artifact;
use crate::catalog::Image;
use crate::formats::flashtool::tests::{BODY, good_exe};

/// SHA-256 of `abc` (FIPS 180-2, appendix B.1).
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// Answers from a fixed table and records each URL it was asked for.
struct Table {
    answers: Vec<(&'static str, Result<&'static [u8], &'static str>)>,
    asked: Vec<String>,
}

impl Source for Table {
    fn get(&mut self, url: &str, _limit: u64) -> Result<Vec<u8>, String> {
        self.asked.push(url.to_owned());
        let answer = self.answers.iter().find(|(known, _)| *known == url);
        match answer {
            Some((_, Ok(body))) => Ok(body.to_vec()),
            Some((_, Err(why))) => Err((*why).to_owned()),
            None => Err("404".to_owned()),
        }
    }
}

fn table(answers: Vec<(&'static str, Result<&'static [u8], &'static str>)>) -> Table {
    Table {
        answers,
        asked: Vec::new(),
    }
}

fn artifact() -> Artifact {
    Artifact {
        id: "abc".to_owned(),
        model: "M".to_owned(),
        version: "1".to_owned(),
        file: "abc.exe".to_owned(),
        size: 3,
        sha256: ABC.to_owned(),
        urls: vec!["https://p/1".to_owned(), "https://p/2".to_owned()],
        mirror_urls: vec!["https://m/1".to_owned()],
        images: Vec::new(),
    }
}

/// A fresh directory for one test.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gsfw-fetch-{test}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn sha256_matches_the_standard_vector() {
    assert_eq!(sha256_hex(b"abc"), ABC);
}

#[test]
fn the_first_publisher_url_wins_and_the_mirror_is_not_asked() {
    let mut source = table(vec![
        ("https://p/1", Ok(b"abc")),
        ("https://m/1", Ok(b"abc")),
    ]);

    let data = obtain(&artifact(), None, &mut source, &mut |_| {}).unwrap();

    assert_eq!(data, b"abc", "body");
    assert_eq!(source.asked, ["https://p/1"], "only the first URL");
}

#[test]
fn the_mirror_is_asked_when_no_publisher_url_gives_the_file() {
    let mut source = table(vec![
        ("https://p/1", Err("connection refused")),
        ("https://p/2", Ok(b"abd")),
        ("https://m/1", Ok(b"abc")),
    ]);

    let data = obtain(&artifact(), None, &mut source, &mut |_| {}).unwrap();

    assert_eq!(data, b"abc", "the mirror's body");
    assert_eq!(
        source.asked,
        ["https://p/1", "https://p/2", "https://m/1"],
        "publisher URLs in order, then the mirror"
    );
}

#[test]
fn a_tampered_download_is_never_used() {
    let dir = scratch("tampered");
    let mut source = table(vec![
        ("https://p/1", Ok(b"abd")),
        ("https://p/2", Ok(b"ab")),
        ("https://m/1", Ok(b"abcd")),
    ]);

    let result = fetch(
        &artifact(),
        None,
        &mut source,
        &dir.join("out"),
        &mut |_| {},
    );

    assert!(result.is_err(), "no body has the SHA-256");
    assert!(!dir.join("out").exists(), "nothing written");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_given_file_is_checked_and_no_url_is_asked() {
    let dir = scratch("from");
    fs::write(dir.join("abc.exe"), b"abd").unwrap();
    let mut source = table(vec![("https://p/1", Ok(b"abc"))]);

    let result = fetch(
        &artifact(),
        Some(&dir.join("abc.exe")),
        &mut source,
        &dir.join("out"),
        &mut |_| {},
    );

    assert!(result.is_err(), "the file has a different SHA-256");
    assert!(source.asked.is_empty(), "no URL asked");
    assert!(!dir.join("out").exists(), "nothing written");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_artifact_without_urls_needs_a_file() {
    let bare = Artifact {
        urls: Vec::new(),
        mirror_urls: Vec::new(),
        ..artifact()
    };
    let mut source = table(Vec::new());

    let err = obtain(&bare, None, &mut source, &mut |_| {}).unwrap_err();

    assert!(err.to_string().contains("--from"), "{err}");
}

#[test]
fn each_image_must_have_its_catalog_sha256() {
    let dir = scratch("images");
    fs::create_dir_all(dir.join("data")).unwrap();
    let with_image = Artifact {
        images: vec![Image {
            path: "data/x.fw".to_owned(),
            sha256: ABC.to_owned(),
        }],
        ..artifact()
    };

    fs::write(dir.join("data").join("x.fw"), b"abc").unwrap();
    assert!(check_images(&with_image, &dir).is_ok(), "same SHA-256");
    fs::write(dir.join("data").join("x.fw"), b"abd").unwrap();
    assert!(check_images(&with_image, &dir).is_err(), "other SHA-256");
    fs::remove_file(dir.join("data").join("x.fw")).unwrap();
    assert!(check_images(&with_image, &dir).is_err(), "missing image");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fetch_writes_the_artifact_and_its_files_and_checks_the_images() {
    let dir = scratch("carve");
    let exe = good_exe("data\\x.fw");
    fs::write(dir.join("tool.exe"), &exe).unwrap();
    let with_image = |sum: &str| Artifact {
        size: u64::try_from(exe.len()).unwrap(),
        sha256: sha256_hex(&exe),
        images: vec![Image {
            path: "data/x.fw".to_owned(),
            sha256: sum.to_owned(),
        }],
        ..artifact()
    };
    let from = Some(dir.join("tool.exe"));
    let mut source = table(Vec::new());
    let mut fetch_into = |out: &str, sum: &str| {
        fetch(
            &with_image(sum),
            from.as_deref(),
            &mut source,
            &dir.join(out),
            &mut |_| {},
        )
    };

    let written = fetch_into("good", &sha256_hex(BODY)).unwrap();
    let bad = fetch_into("bad", ABC);

    assert_eq!(written.len(), 2, "the .exe and one file: {written:?}");
    assert_eq!(
        fs::read(dir.join("good").join("abc.exe")).unwrap(),
        exe,
        "the .exe"
    );
    assert_eq!(
        fs::read(dir.join("good").join("data").join("x.fw")).unwrap(),
        BODY,
        "the file"
    );
    assert!(bad.is_err(), "the image has another SHA-256");
    fs::remove_dir_all(&dir).unwrap();
}
