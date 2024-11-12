#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

use crate::app::CustomEvent;

#[derive(Debug)]
pub enum FirstTimeChoice {
    UpdateCheckConsent(bool),
    UseCurrentDefaults,
    PlatformChoice(PlatformPrompts),
    SetupFinished,
}

impl From<FirstTimeChoice> for CustomEvent {
    fn from(value: FirstTimeChoice) -> Self {
        Self::FirstTimeChoice(value)
    }
}
