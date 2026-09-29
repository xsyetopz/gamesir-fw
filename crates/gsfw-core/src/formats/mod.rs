//! Firmware containers: `JieLi` `.ufw` and `.fw` images, the resources inside the Nexus DLL,
//! and the files inside a Flash Tool `.exe`.

pub mod cipher;
mod error;
pub mod flashtool;
pub mod nexus;
pub mod ufw;

pub use cipher::{crc16, enc, sfc};
pub use error::FormatError;
