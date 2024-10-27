use serde::{Deserialize, Serialize};

use crate::platform::Config as PlatformConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub devices: PlatformConfig,
    // pub updates: AutoUpdateSettings
}

// #[derive(Debug, Clone, Default, Deserialize, Serialize)]
// pub struct AutoUpdateSettings {
//     pub update_check_prompt: bool,
//     pub allow_checking_for_updates: bool,
//     pub version_skipped: String,
// }
