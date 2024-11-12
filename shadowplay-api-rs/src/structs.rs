use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct MicrophonePresent {
    pub present: usize,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    #[serde(rename = "type")]
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub code: i32,
    #[serde(rename = "codeText")]
    #[serde(default)]
    pub code_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShadowPlayMicrophone {
    /// ShadowPlay's chosen index for this device
    #[serde(skip_serializing)]
    pub index: usize,
    #[serde(rename = "name")]
    #[serde(skip_serializing)]
    /// Human-friendly name of the audio device
    pub human_name: String,
    #[serde(rename = "id")]
    #[serde(skip_serializing)]
    /// Windows endpoint GUID for the audio device
    pub guid: String,
    /// Whether the device is muted, set at the Windows device level
    pub muted: bool,
    #[serde(rename = "volumePercent")]
    /// Volume percentage from 0 to 100, set at the Windows device level
    pub volume_percent: u8,
    /// Microphone boost percentage from 0 to 100, set at the Windows device level
    #[serde(rename = "boostPercent")]
    pub boost_percent: u8,
}
