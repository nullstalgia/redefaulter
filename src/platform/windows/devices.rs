use std::{fmt::Display, marker::PhantomData};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{ConfigEntry, Discovered},
};

pub type DiscoveredDevice = WindowsAudioDevice<Discovered>;
pub type ConfigDevice = WindowsAudioDevice<ConfigEntry>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsAudioDevice<State> {
    pub human_name: String,
    pub guid: String,
    // direction: Option<Direction>,
    _state: PhantomData<State>,
}

impl<State> WindowsAudioDevice<State> {
    pub fn new(human_name: String, guid: String) -> Self {
        Self {
            human_name,
            guid,
            _state: PhantomData,
        }
    }
    pub fn clear(&mut self) {
        self.human_name.clear();
        self.guid.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.human_name.is_empty() && self.guid.is_empty()
    }
}

// impl WindowsAudioDevice<Discovered> {
//     fn as_generic(&self) -> WindowsAudioDevice<ConfigEntry> {
//         let generic_name = self.human_name
//         WindowsAudioDevice {

//         }
//     }
// }

// impl WindowsAudioDevice<Discovered> {
//     pub fn direction(&self) -> Direction {
//         self.direction.unwrap()
//     }
// }

// impl<State> AudioDevice for WindowsAudioDevice<State> {
//     fn guid(&self) -> &str {
//         self.guid.as_str()
//     }
//     fn human_name(&self) -> &str {
//         self.human_name.as_str()
//     }
//     fn profile_format(&self) -> String {
//         // So I can't use the toml serializer on the raw device since I think it expects a key/value,
//         // but JSON lets me output just the string as is.
//         serde_json::to_string(self).expect("Failed to serialize profile")
//     }
// }

impl TryFrom<wasapi::Device> for DiscoveredDevice {
    type Error = RedefaulterError;
    fn try_from(value: wasapi::Device) -> AppResult<Self> {
        Ok(DiscoveredDevice {
            human_name: value.get_friendlyname()?,
            guid: value.get_id()?,
            _state: PhantomData,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceSet<State> {
    #[serde(default)]
    pub playback: WindowsAudioDevice<State>,
    #[serde(default)]
    pub playback_comms: WindowsAudioDevice<State>,
    #[serde(default)]
    pub recording: WindowsAudioDevice<State>,
    #[serde(default)]
    pub recording_comms: WindowsAudioDevice<State>,
}

impl<State> DeviceSet<State> {
    pub fn update_role(&mut self, role: &DeviceRole, new_device: WindowsAudioDevice<State>) {
        use DeviceRole::*;
        match role {
            Playback => self.playback = new_device,
            PlaybackComms => self.playback_comms = new_device,
            Recording => self.recording = new_device,
            RecordingComms => self.recording_comms = new_device,
        }
    }
    pub fn clear_role(&mut self, role: &DeviceRole) {
        use DeviceRole::*;
        match role {
            Playback => self.playback.clear(),
            PlaybackComms => self.playback_comms.clear(),
            Recording => self.recording.clear(),
            RecordingComms => self.recording_comms.clear(),
        }
    }
    pub fn get_role(&self, role: &DeviceRole) -> &WindowsAudioDevice<State> {
        use DeviceRole::*;
        match role {
            Playback => &self.playback,
            PlaybackComms => &self.playback_comms,
            Recording => &self.recording,
            RecordingComms => &self.recording_comms,
        }
    }
    // pub fn get_mut_role(&mut self, role: &DeviceRole) -> &mut WindowsAudioDevice<State> {
    //     use DeviceRole::*;
    //     match role {
    //         Playback => &mut self.playback,
    //         PlaybackComms => &mut self.playback_comms,
    //         Recording => &mut self.recording,
    //         RecordingComms => &mut self.recording_comms,
    //     }
    // }
}

// A lot of this feels Derive-able.
// If so, could lower amount of platform-specific code that just copies stuff from platform specific structs?

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceRole {
    Playback,
    PlaybackComms,
    Recording,
    RecordingComms,
}

impl Display for DeviceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let role_str = match self {
            Self::Playback => "Playback",
            Self::PlaybackComms => "Playback Comm.",
            Self::Recording => "Recording",
            Self::RecordingComms => "Recording Comm.",
        };
        write!(f, "{role_str}")
    }
}

impl From<&DeviceRole> for wasapi::Direction {
    fn from(value: &DeviceRole) -> Self {
        match value {
            DeviceRole::Playback | DeviceRole::PlaybackComms => Self::Render,
            DeviceRole::Recording | DeviceRole::RecordingComms => Self::Capture,
        }
    }
}

impl From<DeviceRole> for wasapi::Direction {
    fn from(value: DeviceRole) -> Self {
        Self::from(&value)
    }
}

impl From<&DeviceRole> for wasapi::Role {
    fn from(value: &DeviceRole) -> Self {
        match value {
            DeviceRole::Playback | DeviceRole::Recording => Self::Console,
            DeviceRole::PlaybackComms | DeviceRole::RecordingComms => Self::Communications,
        }
    }
}

impl From<DeviceRole> for wasapi::Role {
    fn from(value: DeviceRole) -> Self {
        Self::from(&value)
    }
}

impl<State> Display for WindowsAudioDevice<State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.guid.is_empty(), self.human_name.is_empty()) {
            // If the name's populated, just use that
            (_, false) => write!(f, "{}", self.human_name),
            // Only GUID populated
            (false, true) => write!(f, "By GUID: \"{}\"", self.guid),
            // Neither populated?
            (true, true) => write!(f, "Empty device?"),
        }
    }
}

impl<State> DeviceSet<State> {
    pub fn is_empty(&self) -> bool {
        self.playback.is_empty()
            && self.playback_comms.is_empty()
            && self.recording.is_empty()
            && self.recording_comms.is_empty()
    }
}
