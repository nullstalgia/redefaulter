use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_plain::derive_display_from_serialize;

use crate::platform::DeviceRole;

use super::common_ids::DEVICE_PREFIX;

/// An enum to help with titling submenus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceSelectionType<'a> {
    /// This set of device selections is for the app's globally desired default
    ConfigDefault,
    /// This set of device selections is for changing a profile's set defaults
    ///
    /// Presently only works with profiles with filenames that are valid UTF-8.
    Profile(&'a str),
}

#[derive(Debug)]
pub struct TrayDevice<'a> {
    pub destination: DeviceSelectionType<'a>,
    pub role: DeviceRole,
    /// If `None`, will clear the profile's entry for that role.
    ///
    /// Otherwise, replaces entry with device associated with the GUID
    pub guid: Option<&'a str>,
}

impl<'a> TrayDevice<'a> {
    pub fn new(destination: &DeviceSelectionType<'a>, role: &DeviceRole, guid: &'a str) -> Self {
        Self {
            destination: destination.to_owned(),
            role: role.to_owned(),
            guid: Some(guid),
        }
    }
    pub fn none(destination: &DeviceSelectionType<'a>, role: &DeviceRole) -> Self {
        Self {
            destination: destination.to_owned(),
            role: role.to_owned(),
            guid: None,
        }
    }
}

// Character is illegal in filenames, so should be safe to use.
const TRAY_ID_DELIMITER: char = '|';

impl<'de: 'a, 'a> Deserialize<'de> for TrayDevice<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf: &str = Deserialize::deserialize(deserializer)?;

        // Example input:
        // device||playback|1234567890
        // ^ a default-config entry uses a 0-length name, to avoid a potential collision with a file named "config"
        // device|00-notepad|playback
        // ^ unsetting device by omitting guid field and accompanying delimiter

        let parts: Vec<&str> = buf.split(TRAY_ID_DELIMITER).collect();
        if parts[0] != DEVICE_PREFIX {
            return Err(D::Error::custom(
                "Tried to deserialize a non-tray-id string",
            ));
        }

        match parts.len() {
            3 | 4 => {
                // Since I can't get the size of the split string without collecting it first
                let destination: DeviceSelectionType = {
                    let dest = parts[1];
                    if dest.is_empty() {
                        DeviceSelectionType::ConfigDefault
                    } else {
                        DeviceSelectionType::Profile(dest)
                    }
                };
                let role: DeviceRole = serde_plain::from_str(parts[2]).map_err(D::Error::custom)?;
                let guid = parts.get(3).copied();

                Ok(TrayDevice {
                    destination,
                    role,
                    guid,
                })
            }
            _ => Err(D::Error::custom(
                "Invalid number of components in TrayDevice deserialization",
            )),
        }
    }
}

impl Serialize for TrayDevice<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let guid = match &self.destination {
            DeviceSelectionType::ConfigDefault => "",
            DeviceSelectionType::Profile(name) => name,
        };
        let mut buffer = format!("{DEVICE_PREFIX}{TRAY_ID_DELIMITER}{guid}",);

        buffer.push(TRAY_ID_DELIMITER);
        buffer.push_str(&serde_plain::to_string(&self.role).map_err(serde::ser::Error::custom)?);

        if let Some(guid) = &self.guid {
            buffer.push(TRAY_ID_DELIMITER);
            buffer.push_str(guid);
        }

        serializer.serialize_str(&buffer)
    }
}

derive_display_from_serialize!(TrayDevice<'a>);
