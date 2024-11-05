use std::ffi::OsString;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_plain::derive_display_from_serialize;

use crate::platform::DeviceRole;

use super::common_ids::DEVICE_PREFIX;

/// An enum to help with titling submenus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceSelectionType {
    /// This set of device selections is for the app's globally desired default
    ConfigDefault,
    /// This set of device selections is for changing a profile's set defaults
    ///
    /// Presently only works with profiles with filenames that are valid UTF-8.
    Profile(String),
}

#[derive(Debug)]
pub struct TrayDevice {
    pub destination: DeviceSelectionType,
    pub role: DeviceRole,
    pub guid: Option<String>,
}

impl TrayDevice {
    pub fn new(destination: &DeviceSelectionType, role: &DeviceRole, guid: &str) -> Self {
        Self {
            destination: destination.to_owned(),
            role: role.to_owned(),
            guid: Some(guid.to_string()),
        }
    }
    pub fn none(destination: &DeviceSelectionType, role: &DeviceRole) -> Self {
        Self {
            destination: destination.to_owned(),
            role: role.to_owned(),
            guid: None,
        }
    }
}

// Character is illegal in filenames, so should be safe to use.
const TRAY_ID_DELIMITER: char = '|';

impl<'de> Deserialize<'de> for TrayDevice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = String::deserialize(deserializer)?;

        // Example input:
        // device~~playback~1234567890
        // ^ a default-config entry uses a 0-length name, to avoid a potential collision with a file named "config"
        // device~00-notepad~playback
        // ^ unsetting device by omitting guid field and accompanying delimiter

        let parts: Vec<&str> = buf.split(TRAY_ID_DELIMITER).collect();

        match parts.len() {
            3 | 4 => {
                if parts[0] != DEVICE_PREFIX {
                    return Err(D::Error::custom(
                        "Tried to deserialize a non-tray-id string",
                    ));
                }
                // Since I can't get the size of the split string without collecting it first
                // let parts_iter = parts.into_iter();
                let destination: DeviceSelectionType = {
                    let dest = parts[1];
                    if dest.is_empty() {
                        DeviceSelectionType::ConfigDefault
                    } else {
                        DeviceSelectionType::Profile(dest.to_owned())
                    }
                };
                let role: DeviceRole = serde_plain::from_str(parts[2]).map_err(D::Error::custom)?;
                let guid = parts.get(3).map(|s| s.to_string());

                Ok(TrayDevice {
                    destination,
                    role,
                    guid,
                })
            }
            _ => {
                return Err(D::Error::custom(
                    "Invalid number of components in TrayDevice deserialization",
                ))
            }
        }
    }
}

impl Serialize for TrayDevice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut serialized = format!(
            "{DEVICE_PREFIX}{TRAY_ID_DELIMITER}{}",
            match &self.destination {
                DeviceSelectionType::ConfigDefault => "",
                DeviceSelectionType::Profile(name) => name,
            }
        );

        serialized.push(TRAY_ID_DELIMITER);
        serialized
            .push_str(&serde_plain::to_string(&self.role).map_err(serde::ser::Error::custom)?);

        if let Some(guid) = &self.guid {
            serialized.push(TRAY_ID_DELIMITER);
            serialized.push_str(guid);
        }

        serializer.serialize_str(&serialized)
    }
}

derive_display_from_serialize!(TrayDevice);
