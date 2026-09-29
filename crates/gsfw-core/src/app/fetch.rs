//! Get a catalog artifact, check its SHA-256, and write its images, over the [`Source`] port.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::bytes::hex;
use crate::catalog::Artifact;
use crate::formats::{FormatError, flashtool};

/// Gives the body at a URL.
pub trait Source {
    /// The body at `url`. The body is at most `limit` bytes.
    ///
    /// # Errors
    /// The message when the request fails, the server gives an error status, or the body is
    /// longer than `limit`.
    fn get(&mut self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}

/// Lowercase hex SHA-256 of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data), "")
}

/// # Errors
/// The message when the SHA-256 of `data` is not the artifact's.
fn checked(artifact: &Artifact, data: Vec<u8>) -> Result<Vec<u8>, String> {
    let sum = sha256_hex(&data);
    if sum != artifact.sha256 {
        return Err(format!(
            "SHA-256 {sum}, the catalog gives {}",
            artifact.sha256
        ));
    }
    Ok(data)
}

/// The artifact's bytes, from `from` when given, else from the first URL that gives the correct
/// SHA-256: the publisher URLs first, then the mirror URLs.
///
/// # Errors
/// [`FormatError`] when `from` cannot be read or has a different SHA-256, or no URL gives the
/// artifact.
fn obtain(
    artifact: &Artifact,
    from: Option<&Path>,
    source: &mut dyn Source,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, FormatError> {
    if let Some(file) = from {
        let data = fs::read(file).map_err(|err| FormatError::io(file, err))?;
        return checked(artifact, data)
            .map_err(|why| FormatError::Invalid(format!("{}: {why}", file.display())));
    }
    for url in artifact.urls.iter().chain(&artifact.mirror_urls) {
        match source
            .get(url, artifact.size)
            .and_then(|data| checked(artifact, data))
        {
            Ok(data) => {
                log(&format!("{url}: SHA-256 ok"));
                return Ok(data);
            }
            Err(why) => log(&format!("{url}: {why}")),
        }
    }
    let why = if artifact.urls.is_empty() && artifact.mirror_urls.is_empty() {
        "the catalog has no URL for it. Download it and give the file with --from FILE"
    } else {
        "no URL gave a file with the SHA-256 of the catalog"
    };
    Err(FormatError::Invalid(format!("{}: {why}", artifact.id)))
}

/// Gets `artifact` and writes it into `out`, then writes its files under `out` and checks the
/// SHA-256 of each catalog image. Returns `(length, path)` for each file written.
///
/// Nothing is written when no source gives the artifact with the SHA-256 of the catalog.
///
/// # Errors
/// [`FormatError`] when no source gives the artifact, a file cannot be written, or an image
/// has a different SHA-256.
pub fn fetch(
    artifact: &Artifact,
    from: Option<&Path>,
    source: &mut dyn Source,
    out: &Path,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<(usize, PathBuf)>, FormatError> {
    let data = obtain(artifact, from, source, log)?;
    let path = flashtool::target(out, &artifact.file)?;
    fs::create_dir_all(out).map_err(|err| FormatError::io(out, err))?;
    fs::write(&path, &data).map_err(|err| FormatError::io(&path, err))?;
    let mut written = vec![(data.len(), path.clone())];
    written.extend(flashtool::carve(&path, out)?);
    check_images(artifact, out)?;
    Ok(written)
}

/// Checks the SHA-256 of each catalog image under `out`.
///
/// # Errors
/// [`FormatError`] when an image cannot be read or has a different SHA-256.
fn check_images(artifact: &Artifact, out: &Path) -> Result<(), FormatError> {
    for image in &artifact.images {
        let file = flashtool::target(out, &image.path)?;
        let body = fs::read(&file).map_err(|err| FormatError::io(&file, err))?;
        let sum = sha256_hex(&body);
        if sum != image.sha256 {
            return Err(FormatError::Invalid(format!(
                "{}: SHA-256 {sum}, the catalog gives {}",
                file.display(),
                image.sha256
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
