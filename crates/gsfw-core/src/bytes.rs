//! Byte helpers shared by the parsers: Python slice semantics and little-endian fields.

/// `data[start:start + len]` as Python slices it: clamped to the data, empty past its end.
#[must_use]
pub fn window(data: &[u8], start: usize, len: usize) -> &[u8] {
    let end = start.saturating_add(len).min(data.len());
    data.get(start..end).unwrap_or_default()
}

/// `data[start:]` as Python slices it.
#[must_use]
pub fn tail(data: &[u8], start: usize) -> &[u8] {
    data.get(start..).unwrap_or_default()
}

/// A `u32` file offset or size as an index; saturates where `usize` is narrower.
#[must_use]
pub fn index(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The low 16 bits of `value` (Python `value & 0xFFFF`).
#[must_use]
pub const fn low16(value: usize) -> u16 {
    let [lo, hi, ..] = value.to_le_bytes();
    u16::from_le_bytes([lo, hi])
}

/// Little-endian `u16` at `at`, if the data holds it.
pub fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    tail(data, at)
        .first_chunk()
        .copied()
        .map(u16::from_le_bytes)
}

/// Little-endian `u32` at `at`, if the data holds it.
pub fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    tail(data, at)
        .first_chunk()
        .copied()
        .map(u32::from_le_bytes)
}

/// Little-endian `i32` at `at`, if the data holds it.
pub fn i32_at(data: &[u8], at: usize) -> Option<i32> {
    tail(data, at)
        .first_chunk()
        .copied()
        .map(i32::from_le_bytes)
}

/// Copies `src` into `dst` at `at`; bytes past the end of `dst` are dropped.
pub fn put(dst: &mut [u8], at: usize, src: &[u8]) {
    let end = at.saturating_add(src.len()).min(dst.len());
    if let Some(slot) = dst.get_mut(at..end) {
        slot.copy_from_slice(window(src, 0, slot.len()));
    }
}

/// The bytes before the first NUL, decoded like Python's `decode("ascii", "replace")`.
#[must_use]
pub fn ascii_name(raw: &[u8]) -> String {
    raw.split(|&b| b == 0)
        .next()
        .unwrap_or_default()
        .iter()
        .map(|&b| {
            if b.is_ascii() {
                char::from(b)
            } else {
                '\u{fffd}'
            }
        })
        .collect()
}

/// Lowercase hex, bytes joined by `sep` (Python `bytes.hex(sep)`).
#[must_use]
pub fn hex(data: &[u8], sep: &str) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(sep)
}

/// First index of `needle` in `data` (Python `bytes.find`; an empty needle is found at 0).
#[must_use]
pub fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    data.windows(needle.len()).position(|w| w == needle)
}
