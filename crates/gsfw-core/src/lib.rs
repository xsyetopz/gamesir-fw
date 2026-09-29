//! `GameSir` firmware formats, the firmware catalog, the `JieLi` GIP upgrade protocol, the flash
//! and download use cases, the USB and HTTP adapters, and the operations of the CLI and the GUI.

extern crate alloc;

pub mod app;
pub mod bytes;
pub mod catalog;
pub mod formats;
pub mod jieli;
pub mod net;
pub mod ops;
pub mod usb;
