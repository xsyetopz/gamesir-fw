use super::{builtin, parse};

/// SHA-256 of `abc` (FIPS 180-2, appendix B.1).
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn catalog(id2: &str, file: &str, sum: &str) -> String {
    format!(
        r#"{{"artifacts": [
            {{"id": "a", "model": "M", "version": "1", "file": "a.exe", "size": 3,
              "sha256": "{ABC}", "urls": ["https://p/a"], "mirror_urls": [],
              "images": [{{"path": "data/x.fw", "sha256": "{ABC}"}}]}},
            {{"id": "{id2}", "model": "M", "version": "2", "file": "{file}", "size": 3,
              "sha256": "{sum}", "urls": [], "mirror_urls": ["https://m/b"], "images": []}}
        ]}}"#
    )
}

#[test]
fn a_valid_catalog_gives_every_field() {
    let artifacts = parse(&catalog("b", "b.exe", ABC)).unwrap();

    assert_eq!(artifacts.len(), 2, "two artifacts");
    let [first, second] = artifacts.as_slice() else {
        panic!("two artifacts");
    };
    assert_eq!(
        (first.id.as_str(), first.file.as_str(), first.size),
        ("a", "a.exe", 3),
        "first"
    );
    assert_eq!(first.urls, ["https://p/a"], "publisher URLs");
    assert_eq!(second.mirror_urls, ["https://m/b"], "mirror URLs");
    assert_eq!(
        first.images.first().map(|i| i.path.as_str()),
        Some("data/x.fw"),
        "image"
    );
}

#[test]
fn two_artifacts_with_one_id_are_refused() {
    assert!(parse(&catalog("a", "b.exe", ABC)).is_err());
}

#[test]
fn a_file_name_with_a_path_is_refused() {
    // The Rust text `d\\\\b.exe` gives the JSON text `d\\b.exe`, which is the name `d\b.exe`.
    for file in ["", "..", "../b.exe", "d/b.exe", "d\\\\b.exe", "C:b.exe"] {
        assert!(parse(&catalog("b", file, ABC)).is_err(), "{file:?}");
    }
}

#[test]
fn a_bad_sha256_is_refused() {
    let upper = ABC.to_uppercase();
    let short = &ABC[1..];
    let long = format!("{ABC}0");
    let not_hex = ABC.replacen('b', "g", 1);
    for sum in [upper.as_str(), short, long.as_str(), not_hex.as_str()] {
        assert!(parse(&catalog("b", "b.exe", sum)).is_err(), "{sum}");
    }
}

#[test]
fn the_builtin_catalog_is_valid() {
    let artifacts = builtin().unwrap();
    assert!(!artifacts.is_empty(), "at least one artifact");
}
