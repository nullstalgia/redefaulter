//! # A Derive macro experiment
//!
//! I hadn't written any macros utilizing `Derive` until making `redefaulter`, and wanted to see if I could make
//! menus for use with [muda] in [tray-icon](https://docs.rs/tray-icon).
//!
//! ## Example usage:
//!
//! ```rust
//! use menu_macro::{MenuId, MenuToggle, TrayChecks};
//!
//! #[derive(Default)]
//! #[derive(MenuId)]
//! #[menuid(prefix = "hello_")]
//! pub struct Settings {
//!     /// Don't show generics
//!     hide_generics: bool,
//! }
//!
//! # fn main() -> Result<(), menu_macro::MenuMacroError> {
//! let settings = Settings::default();
//!
//! assert_eq!("hello_Settings", settings.menu_id_root());
//! assert_eq!("hello_Settings_hide_generics", settings.hide_generics_menu_id());
//!
//! # Ok(())
//! # }
//! ```

pub use menu_macro_impl::*;
mod errors;
pub use errors::*;

// pub fn add(left: u64, right: u64) -> u64 {
//     left + right
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn it_works() {
//         let result = add(2, 2);
//         assert_eq!(result, 4);
//     }
// }
