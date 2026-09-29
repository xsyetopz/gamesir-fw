//! The firmware catalog: the downloads that `gsfw fetch` knows, and their SHA-256 sums.
//!
//! The catalog is `catalog/catalog.json`. The build puts it into the program. Each artifact is
//! a `GameSir` Flash Tool `.exe`. Its publisher URLs come first, then the community mirror URLs.
//! To obey a takedown, remove the mirror URL from the catalog.

use serde_json::Value;

use crate::formats::FormatError;

/// The catalog that the build puts into the program.
const BUILTIN: &str = include_str!("../catalog/catalog.json");

/// One firmware image inside an artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Path under the output directory, with `/` between the parts.
    pub path: String,
    /// SHA-256 of the image, 64 lowercase hex digits.
    pub sha256: String,
}

/// One download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    /// The name that `gsfw fetch` takes.
    pub id: String,
    /// The controller model.
    pub model: String,
    /// The firmware version.
    pub version: String,
    /// The file name for the output directory. It has no path separator.
    pub file: String,
    /// Length of the file in bytes. A download stops after this length.
    pub size: u64,
    /// SHA-256 of the file, 64 lowercase hex digits.
    pub sha256: String,
    /// The publisher's URLs. `gsfw fetch` tries these first.
    pub urls: Vec<String>,
    /// The community mirror URLs. `gsfw fetch` tries these when no publisher URL gives the file.
    pub mirror_urls: Vec<String>,
    /// The images that the file contains.
    pub images: Vec<Image>,
}

fn invalid(message: &str) -> FormatError {
    FormatError::Invalid(format!("catalog: {message}"))
}

/// The catalog in the program.
///
/// # Errors
/// [`FormatError::Invalid`] when `catalog/catalog.json` is not a valid catalog.
pub fn builtin() -> Result<Vec<Artifact>, FormatError> {
    parse(BUILTIN)
}

/// The artifacts of a catalog file.
///
/// # Errors
/// [`FormatError::Invalid`] for bad JSON, a missing field, a bad SHA-256, a file name with a
/// path separator, or two artifacts with the same id.
pub fn parse(text: &str) -> Result<Vec<Artifact>, FormatError> {
    let root: Value = serde_json::from_str(text).map_err(|err| invalid(&err.to_string()))?;
    let list = root
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("no `artifacts` list"))?;
    let artifacts = list.iter().map(artifact).collect::<Result<Vec<_>, _>>()?;
    for (n, one) in artifacts.iter().enumerate() {
        if artifacts
            .iter()
            .skip(n.saturating_add(1))
            .any(|other| other.id == one.id)
        {
            return Err(invalid(&format!("two artifacts have the id {}", one.id)));
        }
    }
    Ok(artifacts)
}

/// # Errors
/// [`FormatError::Invalid`] for a missing field or a bad value.
fn artifact(value: &Value) -> Result<Artifact, FormatError> {
    let id = text(value, "id", "artifact")?;
    let file = text(value, "file", &id)?;
    if file.is_empty() || file == "." || file == ".." || file.contains(['/', '\\', ':']) {
        return Err(invalid(&format!("{id}: `file` is not a plain file name")));
    }
    let images = value
        .get("images")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(&format!("{id}: no `images` list")))?
        .iter()
        .map(|image| {
            Ok(Image {
                path: text(image, "path", &id)?,
                sha256: sha256(image, &id)?,
            })
        })
        .collect::<Result<_, FormatError>>()?;
    Ok(Artifact {
        model: text(value, "model", &id)?,
        version: text(value, "version", &id)?,
        size: value
            .get("size")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid(&format!("{id}: no `size`")))?,
        sha256: sha256(value, &id)?,
        urls: texts(value, "urls", &id)?,
        mirror_urls: texts(value, "mirror_urls", &id)?,
        images,
        file,
        id,
    })
}

/// # Errors
/// [`FormatError::Invalid`] when `key` is not a string.
fn text(value: &Value, key: &str, owner: &str) -> Result<String, FormatError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid(&format!("{owner}: `{key}` is not a string")))
}

/// # Errors
/// [`FormatError::Invalid`] when `key` is not a list of strings.
fn texts(value: &Value, key: &str, owner: &str) -> Result<Vec<String>, FormatError> {
    let bad = || invalid(&format!("{owner}: `{key}` is not a list of strings"));
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(bad)?
        .iter()
        .map(|item| item.as_str().map(str::to_owned).ok_or_else(bad))
        .collect()
}

/// # Errors
/// [`FormatError::Invalid`] when `sha256` is not 64 lowercase hex digits.
fn sha256(value: &Value, owner: &str) -> Result<String, FormatError> {
    let sum = text(value, "sha256", owner)?;
    let hex = sum
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if sum.len() != 64 || !hex {
        return Err(invalid(&format!(
            "{owner}: `sha256` is not 64 lowercase hex digits"
        )));
    }
    Ok(sum)
}

#[cfg(test)]
mod tests;
