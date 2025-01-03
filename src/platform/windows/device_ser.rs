use serde::{Deserialize, Deserializer, Serialize};

use super::WindowsAudioDevice;

const DEVICE_DELIMITER: char = '~';

impl<'de, State> Deserialize<'de> for WindowsAudioDevice<State> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = String::deserialize(deserializer)?;

        // Example input:
        // Speakers (Device)~{x}.{y}

        let parts: Vec<&str> = buf.split(DEVICE_DELIMITER).collect();

        let (human_name, guid) = match parts.len() {
            2 => (String::from(parts[0]), String::from(parts[1])),
            1 => {
                if parts[0].starts_with(DEVICE_DELIMITER) || parts[0].starts_with('{') {
                    (String::new(), String::from(parts[0]))
                } else {
                    (String::from(parts[0]), String::new())
                }
            }
            _ => (String::new(), String::new()),
        };

        Ok(Self::new(human_name, guid))
    }
}

impl<State> Serialize for WindowsAudioDevice<State> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut buffer = String::new();
        if !self.human_name.is_empty() {
            buffer.push_str(&self.human_name);
        }
        if !self.guid.is_empty() {
            buffer.push(DEVICE_DELIMITER);
            buffer.push_str(&self.guid);
        }
        serializer.serialize_str(&buffer)
    }
}
