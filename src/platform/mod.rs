#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{
    device_notifications::WindowsAudioNotification as AudioEndpointNotification, AudioNightmare,
    ConfigDevice, DeviceRole, DeviceSet, DiscoveredDevice, PlatformSettings,
};

use serde::{Deserialize, Serialize};

// I don't plan on doing this, but I'd rather over-engineer a little to prevent either myself
// or someone else some future pain.
// #[cfg(target_os = "linux")]
// mod unix;
// #[cfg(target_os = "linux")]
// pub use unix::AudioNightmare;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
/// A device tagged with this could be unreachable, and thus
/// needs to be checked before setting any role to it.
pub struct ConfigEntry;
#[derive(Debug, Clone)]
/// A device tagged with this is known to be connected and available
pub struct Discovered;

// pub trait AudioDevice {
//     fn guid(&self) -> &str;
//     fn human_name(&self) -> &str;
//     fn profile_format(&self) -> String;
// }
