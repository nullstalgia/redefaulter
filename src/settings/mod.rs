use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use derivative::Derivative;
use fs_err::{self as fs};
use menu_macro::{MenuId, MenuToggle, TrayChecks};
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing::*;

use crate::errors::AppResult;
use crate::platform::PlatformSettings;

// TODO Proper defaults.

#[derive(Debug, Clone, Serialize, Deserialize, MenuToggle, MenuId, TrayChecks, Derivative)]
#[derivative(Default)]
pub struct BehaviorSettings {
    /// Always Save Generics
    ///
    /// When true, selecting a device in the tray menu will always save
    /// the most generic version of that device.
    ///
    /// For example, on Windows, instead of saving "Speakers (3- Gaming Headset)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
    ///
    /// It would save "Speakers (Gaming Headset)", ignoring the Windows-appended numeric identifier, and not
    /// saving the GUID.
    #[derivative(Default(value = "true"))]
    #[serde(default)]
    pub always_save_generics: bool,
    /// Show Active Devices
    ///
    /// Just a toggle for showing the current default devices in the tray menu.
    #[serde(default)]
    pub show_active_devices: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Derivative)]
#[derivative(Default)]
pub struct MiscSettings {
    #[derivative(Default(value = "String::from(\"debug\")"))]
    pub log_level: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub behavior: BehaviorSettings,
    #[serde(default)]
    pub misc: MiscSettings,
    #[serde(rename = "devices")]
    #[serde(default)]
    pub platform: PlatformSettings,
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
        let mut file = fs::File::open(path)?;
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
        let mut file = fs::File::create(config_path)?;
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
