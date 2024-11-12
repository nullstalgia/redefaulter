use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use derivative::Derivative;
use fs_err::{self as fs};
use menu_macro::{MenuId, MenuToggle, TrayChecks};
use serde::{Deserialize, Serialize};
use tracing::level_filters::LevelFilter;
use tracing::*;

use crate::errors::{AppResult, RedefaulterError};
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
    #[serde(default)]
    pub log_level: String,
    #[serde(default)]
    pub first_time_setup_done: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, MenuToggle, MenuId, TrayChecks)]
pub struct AutoUpdateSettings {
    /// Check for updates on startup
    ///
    /// When true, allows the app to check for updates a single time when it launches.
    #[serde(default)]
    pub allow_checking_for_updates: bool,
    #[serde(default)]
    #[menuid(skip)]
    pub version_skipped: String,
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
    #[serde(default)]
    pub updates: AutoUpdateSettings,
}

impl Settings {
    pub fn load(path: &Path, required: bool) -> AppResult<Self> {
        if !path.exists() && !required {
            let default = Settings::default();
            default.save(path)?;
            return Ok(default);
        } else if !path.exists() && required {
            return Err(RedefaulterError::RequiredSettingsMissing);
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
