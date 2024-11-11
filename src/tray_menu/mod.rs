#[cfg(target_os = "windows")]
mod windows;
// #[cfg(target_os = "windows")]
// pub use windows::*;

mod common;
pub use common::*;
mod device_ser;
pub use device_ser::*;
mod updates;
pub use updates::*;
