use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use fs_err::File;
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing::*;

use crate::errors::AppResult;
use crate::platform::PlatformConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BehaviorSettings {
    #[serde(default)]
    pub always_save_generics: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MiscSettings {
    #[serde(default)]
    pub log_level: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub redefaulter: BehaviorSettings,
    #[serde(default)]
    pub misc: MiscSettings,
    #[serde(default)]
    pub devices: PlatformConfig,
    // pub updates: AutoUpdateSettings
}

impl Settings {
    pub fn load(path: &Path, required: bool) -> AppResult<Self> {
        if !path.exists() && !required {
            let default = Settings::default();
            default.save(path)?;
            return Ok(default);
        } else if !path.exists() && required {
            // TODO Make an actual error
            panic!();
        }
        let mut file = File::open(path)?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        drop(file);
        let config: Settings = toml::from_str(&buffer)?;
        config.save(path)?;
        Ok(config)
    }
    pub fn save(&self, config_path: &Path) -> AppResult<()> {
        // TODO Look into toml_edit's options
        let toml_config = toml::to_string(self)?;
        info!("Serialized config length: {}", toml_config.len());
        let mut file = File::create(config_path)?;
        file.write_all(toml_config.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }
    pub fn get_log_level(&self) -> LevelFilter {
        LevelFilter::from_str(&self.misc.log_level).unwrap_or(LevelFilter::DEBUG)
    }
}

// #[derive(Debug, Clone, Default, Deserialize, Serialize)]
// pub struct AutoUpdateSettings {
//     pub update_check_prompt: bool,
//     pub allow_checking_for_updates: bool,
//     pub version_skipped: String,
// }
