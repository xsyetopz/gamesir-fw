//! The shared operations against the real G7 SE 6.40 image under `private/`; skips without it.
#![cfg(test)]

#[path = "common/fixture.rs"]
pub mod fixture;

use fixture::fixture;

extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use std::path::PathBuf;

use gsfw_core::ops::{Op, Sink, run};

/// An empty scratch directory path for this test process; the directory itself is not made.
fn fresh_dir(name: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    if out.exists() {
        std::fs::remove_dir_all(&out).unwrap();
    }
    out
}

fn capture(op: &Op) -> (bool, Vec<String>) {
    let lines = Rc::new(RefCell::new(Vec::new()));
    let sink: Sink = {
        let lines = Rc::clone(&lines);
        Rc::new(move |line: &str| lines.borrow_mut().push(line.to_owned()))
    };
    let ok = run(op, &sink).unwrap();
    let out = lines.borrow().clone();
    (ok, out)
}

#[test]
fn info_shows_entries_and_flash_head() {
    let Some(image) = fixture("flash-tool/bundle/JS_SL3101_V640_Key.fw") else {
        return;
    };
    let (ok, lines) = capture(&Op::Info { image });
    assert!(ok, "CRCs pass");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("flash.bin head: crc ok true")),
        "{lines:#?}"
    );
}

#[test]
fn dry_run_prints_ten_fragments_per_packet() {
    let Some(image) = fixture("flash-tool/bundle/JS_SL3101_V640_Key.fw") else {
        return;
    };
    let (ok, lines) = capture(&Op::DryRun { image });
    assert!(ok, "ran");
    assert_eq!(lines.len(), 22, "2 labels + 2 x 10 fragments");
    assert!(
        lines[1].starts_with("0f 00 01 3c f0 00 00 00 4a 0a 01"),
        "{}",
        lines[1]
    );
}

#[test]
fn app_bin_writes_the_file() {
    let Some(image) = fixture("flash-tool/bundle/JS_SL3101_V640_Key.fw") else {
        return;
    };
    let out = std::env::temp_dir().join(format!("gsfw-ops-test-{}.bin", std::process::id()));
    let (ok, lines) = capture(&Op::AppBin {
        image,
        out: out.clone(),
    });
    let len = std::fs::metadata(&out).unwrap().len();
    std::fs::remove_file(&out).unwrap();
    assert!(ok, "ran");
    assert_eq!(lines, [out.display().to_string()], "prints the path");
    assert_eq!(len, 0x2_C258, "app.bin length");
}

/// The oracle is `flash-tool/bundle/`, which was carved by hand from the same `.exe`.
#[test]
fn flash_tool_extract_matches_the_hand_carved_files() {
    let (Some(exe), Some(bundle)) = (
        fixture("flash-tool/flashtool-640.exe"),
        fixture("flash-tool/bundle"),
    ) else {
        return;
    };
    let out = fresh_dir("gsfw-flashtool-test");

    let (ok, lines) = capture(&Op::FlashToolExtract {
        exe,
        out: out.clone(),
    });

    assert!(ok, "ran");
    assert_eq!(lines.len(), 5, "four files and tail.bin: {lines:?}");
    for name in [
        "config.ini",
        "JS_SL3101_V640_Key.fw",
        "JS_SL3101_V640_No_Key.fw",
        "Window.png",
    ] {
        let carved = std::fs::read(out.join("data").join(name)).unwrap();
        let by_hand = std::fs::read(bundle.join(name)).unwrap();
        assert!(
            carved == by_hand,
            "{name} differs from the hand-carved copy"
        );
    }
    std::fs::remove_dir_all(&out).unwrap();
}

/// The catalog sums were computed with Python `hashlib` from the same `.exe` and from the
/// hand-carved `flash-tool/bundle/`.
#[test]
fn fetch_from_a_file_passes_the_catalog_sums() {
    let (Some(exe), Some(bundle)) = (
        fixture("flash-tool/flashtool-640.exe"),
        fixture("flash-tool/bundle"),
    ) else {
        return;
    };
    let out = fresh_dir("gsfw-fetch-test");

    let (ok, lines) = capture(&Op::Fetch {
        artifact: "g7se-6.40-flash-tool".to_owned(),
        out: out.clone(),
        from: Some(exe.clone()),
    });

    assert!(ok, "ran");
    assert_eq!(
        lines.len(),
        6,
        "the .exe, four files and tail.bin: {lines:?}"
    );
    let copy = std::fs::read(out.join("G7 SE-HE 6.40 Flash Tool - PID Protected.exe")).unwrap();
    assert!(copy == std::fs::read(&exe).unwrap(), "the .exe copy");
    let image = std::fs::read(out.join("data").join("JS_SL3101_V640_Key.fw")).unwrap();
    let by_hand = std::fs::read(bundle.join("JS_SL3101_V640_Key.fw")).unwrap();
    assert!(image == by_hand, "the image");
    std::fs::remove_dir_all(&out).unwrap();
}

/// Each catalog artifact in `private/flash-tool/cdn/` gives its images with the catalog sums.
/// Python `hashlib` computed the sums from the downloads and from the files that
/// `gsfw flash-tool-extract` wrote.
#[test]
fn each_downloaded_tool_passes_the_catalog_sums() {
    let Some(dir) = fixture("flash-tool/cdn") else {
        return;
    };
    let mut checked = 0_usize;
    for artifact in gsfw_core::catalog::builtin().unwrap() {
        let exe = dir.join(&artifact.file);
        if !exe.exists() {
            continue;
        }
        let out = fresh_dir(&format!("gsfw-fetch-{}", artifact.id));

        let (ok, lines) = capture(&Op::Fetch {
            artifact: artifact.id.clone(),
            out: out.clone(),
            from: Some(exe),
        });

        assert!(ok, "{}: {lines:?}", artifact.id);
        std::fs::remove_dir_all(&out).unwrap();
        checked = checked.saturating_add(1);
    }
    assert!(checked > 0, "no catalog file in {}", dir.display());
}
