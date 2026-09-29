//! The use cases: the upgrade session and the flash over the [`GipLink`] port, and the
//! firmware download over the [`Source`] port.

pub mod fetch;
pub mod flash;
pub mod session;

pub use fetch::{Source, fetch, sha256_hex};
pub use flash::{choose_plan, flash, is_flash_bin, write_region};
pub use session::{
    DeviceError, GipLink, Log, Session, device_crc, open_session, random16, request, unscramble_any,
};
