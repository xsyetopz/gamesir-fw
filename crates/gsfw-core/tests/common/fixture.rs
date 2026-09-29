//! Test files under `private/`, which the repository does not hold.

use std::path::{Path, PathBuf};

/// The path of `rel` under `private/`, or `None` (with a note) when it is missing.
#[must_use]
pub fn fixture(rel: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private")
        .join(rel);
    if path.exists() {
        Some(path)
    } else {
        eprintln!("skipped: {} is missing", path.display());
        None
    }
}
