//! The error type of the protocol layer.

use core::fmt;

/// A packet failed its checks, or an argument or image does not fit the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// A reply failed a descramble or layout check (Python `BadPacket`).
    BadPacket(String),
    /// An argument or image does not fit the protocol (Python `ValueError`).
    Invalid(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPacket(message) | Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for ProtocolError {}
