#[cfg(target_os = "windows")]
mod windows;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
pub use windows::{
    device_notifications::WindowsAudioNotification as AudioEndpointNotification, AudioNightmare,
    Config, ConfigDevice, DeviceSet, DiscoveredDevice,
};

// I don't plan on doing this, but I'd rather over-engineer a little to prevent either myself
// or someone else some future pain.
// #[cfg(target_os = "linux")]
// mod unix;
// #[cfg(target_os = "linux")]
// pub use unix::AudioNightmare;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConfigEntry;
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Discovered;

pub trait AudioDevice {
    fn guid(&self) -> &str;
    fn human_name(&self) -> &str;
    fn profile_format(&self) -> String;
}
