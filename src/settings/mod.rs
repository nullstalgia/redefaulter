use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use fs_err::File;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::errors::AppResult;
use crate::platform::PlatformConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub devices: PlatformConfig,
    // pub updates: AutoUpdateSettings
}

impl Config {
    pub fn load(path: &Path, required: bool) -> AppResult<Self> {
        if !path.exists() && !required {
            let default = Config::default();
            default.save(&path)?;
            return Ok(default);
        } else if !path.exists() && required {
            // TODO Make an actual error
            panic!();
        }
        let mut file = File::open(&path)?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        drop(file);
        let config: Config = toml::from_str(&buffer)?;
        config.save(&path)?;
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
}

// #[derive(Debug, Clone, Default, Deserialize, Serialize)]
// pub struct AutoUpdateSettings {
//     pub update_check_prompt: bool,
//     pub allow_checking_for_updates: bool,
//     pub version_skipped: String,
// }
