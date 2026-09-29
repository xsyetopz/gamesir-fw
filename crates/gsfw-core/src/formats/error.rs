//! The error type of the format parsers.

use core::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// A firmware or resource file could not be read, written or parsed.
#[derive(Debug)]
pub enum FormatError {
    /// Reading or writing `path` failed.
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The operating system error.
        source: io::Error,
    },
    /// The data does not have the expected layout.
    Invalid(String),
}

impl FormatError {
    /// Wraps an I/O error with the path it happened on.
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}
