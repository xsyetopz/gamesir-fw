//! Carve embedded resources (firmware `.ufw`, device JSON) out of the Nexus .NET Native DLL.
//!
//! .NET Native (UWP) drops the CLR metadata. The resources survive as a name table (UTF-8
//! name, then two packed varints: offset, length) and one contiguous blob section. Offsets are
//! relative to the first resource, which is `unsupported_devices.json`.

use std::fs;
use std::path::{Path, PathBuf};

use super::FormatError;
use crate::bytes::{find, index, tail, window};

/// Prefix of every resource name.
pub const PREFIX: &[u8] = b"HJC.GameSir.Nexus2_0.";

fn truncated() -> FormatError {
    FormatError::Invalid("resource table truncated".to_owned())
}

/// Native metadata packed uint at `data[at]`: the low bits of the first byte give the length.
/// Returns the value and the index after it.
///
/// # Errors
/// [`FormatError::Invalid`] when the data ends inside the varint.
pub fn varint(data: &[u8], at: usize) -> Result<(u32, usize), FormatError> {
    let byte = |n: usize| {
        data.get(at.saturating_add(n))
            .map(|&b| u32::from(b))
            .ok_or_else(truncated)
    };
    let x = byte(0)?;
    let (value, len) = if x & 1 == 0 {
        (x >> 1, 1)
    } else if x & 3 == 1 {
        ((x >> 2) | (byte(1)? << 6), 2)
    } else if x & 7 == 3 {
        ((x >> 3) | (byte(1)? << 5) | (byte(2)? << 13), 3)
    } else if x & 15 == 7 {
        (
            (x >> 4) | (byte(1)? << 4) | (byte(2)? << 12) | (byte(3)? << 20),
            4,
        )
    } else {
        let raw = window(data, at.saturating_add(1), 4);
        (
            u32::from_le_bytes(raw.try_into().map_err(|_| truncated())?),
            5,
        )
    };
    Ok((value, at.saturating_add(len)))
}

/// End of the name match `PREFIX [\x20-\x7e]+? \.(?:ufw|json)` whose prefix starts at `start`.
fn name_end(data: &[u8], start: usize) -> Option<usize> {
    let mut end = start.saturating_add(PREFIX.len());
    loop {
        if !data
            .get(end)
            .is_some_and(|b| b.is_ascii_graphic() || *b == b' ')
        {
            return None;
        }
        end = end.saturating_add(1);
        let rest = tail(data, end);
        for ext in [&b".ufw"[..], b".json"] {
            if rest.starts_with(ext) {
                return Some(end.saturating_add(ext.len()));
            }
        }
    }
}

/// `(name without prefix, offset, length)` of each resource-table record, in file order.
///
/// # Errors
///
/// [`FormatError::Invalid`] when a record's varints are truncated.
fn resources(data: &[u8]) -> Result<Vec<(String, u32, u32)>, FormatError> {
    let mut entries = Vec::new();
    let mut from = 0;
    while let Some(at) = find(tail(data, from), PREFIX).map(|i| from.saturating_add(i)) {
        let Some(end) = name_end(data, at) else {
            from = at.saturating_add(1);
            continue;
        };
        let (offset, next) = varint(data, end)?;
        let (length, _) = varint(data, next)?;
        let name = window(data, at, end.saturating_sub(at));
        let name = String::from_utf8_lossy(tail(name, PREFIX.len())).into_owned();
        entries.push((name, offset, length));
        from = end;
    }
    Ok(entries)
}

/// Target path of resource `name` (`a.b.c.ufw` becomes `out/a/b/c.ufw`).
fn target(out: &Path, name: &str) -> PathBuf {
    let parts: Vec<&str> = name.split('.').collect();
    let mut path = out.to_path_buf();
    if let [dirs @ .., stem, ext] = parts.as_slice() {
        for dir in dirs {
            path.push(dir);
        }
        path.push(format!("{stem}.{ext}"));
    }
    path
}

/// Writes every embedded resource of `dll` under `out`; returns `(length, path)` per resource.
///
/// # Errors
/// [`FormatError::Io`] on a read or write failure; [`FormatError::Invalid`] when no resource
/// names or no blob base are found.
pub fn carve(dll: &Path, out: &Path) -> Result<Vec<(usize, PathBuf)>, FormatError> {
    let data = fs::read(dll).map_err(|e| FormatError::io(dll, e))?;
    let entries = resources(&data)?;
    let first = entries
        .iter()
        .reduce(|best, e| if e.1 < best.1 { e } else { best })
        .ok_or_else(|| {
            FormatError::Invalid(format!(
                "no {}* resource names in {}",
                String::from_utf8_lossy(PREFIX),
                dll.display()
            ))
        })?;
    let base = data
        .iter()
        .enumerate()
        .filter(|&(_, &b)| b == b'[' || b == b'{')
        .map(|(at, _)| at)
        .find(|&at| {
            serde_json::from_slice::<serde_json::Value>(window(&data, at, index(first.2))).is_ok()
        })
        .ok_or_else(|| {
            FormatError::Invalid(format!("blob base not found (first resource {})", first.0))
        })?;
    let mut written = Vec::with_capacity(entries.len());
    for (name, offset, length) in &entries {
        let path = target(out, name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| FormatError::io(parent, e))?;
        }
        let body = window(&data, base.saturating_add(index(*offset)), index(*length));
        fs::write(&path, body).map_err(|e| FormatError::io(&path, e))?;
        written.push((index(*length), path));
    }
    Ok(written)
}
