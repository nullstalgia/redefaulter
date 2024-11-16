use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use derivative::Derivative;
use fs_err::{self as fs};
use menu_macro::{MenuId, MenuToggle, TrayChecks};
use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use tracing::level_filters::LevelFilter;
use tracing::*;

use crate::errors::{AppResult, RedefaulterError};
use crate::platform::PlatformSettings;

// TODO Cleaner defaults.
// What I have now works and is predictable,
// but there's a lot of gross repetition.
// Especially with needing both:
// - #[serde_inline_default] for when a _field_ is missing,
//   - Since #[serde(default)] gets the default for the field's _type_, and *not* the parent struct's `Default::default()` value for it
// - #[derivative(Default)] for properly setting up `Default::default()` for when a _struct_ is missing.

#[serde_inline_default]
#[derive(Debug, Clone, Serialize, Deserialize, MenuToggle, MenuId, TrayChecks, Derivative)]
#[derivative(Default)]
pub struct DeviceSettings {
    /// Fuzzy Match Device Names
    ///
    /// When true, selecting a device in the tray menu will always
    /// save and match by the most generic version of that device's name.
    ///
    /// For example, on Windows, instead of saving `"Speakers (3- Gaming Headset)"`, it would save `"Speakers (Gaming Headset)"`
    #[serde_inline_default(true)]
    #[derivative(Default(value = "true"))]
    pub fuzzy_match_names: bool,
    /// Save Devices with GUID
    ///
    /// When true, selecting a device in the tray menu will also save it with the GUID.
    ///
    /// For example, on Windows, it would produce an output like:
    ///
    /// `"Speakers (Gaming Headset)~{0.0.0.00000000}.{aa-bb-cc-123-456}"`
    ///
    /// Safe to disable if you __don't__ plan to have multiple of the same device connected,
    /// otherwise `fuzzy_match_names` could cause some surprises.
    #[serde_inline_default(true)]
    #[derivative(Default(value = "true"))]
    pub save_guid: bool,
    /// Show Active Devices
    ///
    /// Just a toggle for showing the current default devices in the tray menu.
    #[serde(default)]
    pub show_active: bool,
    /// Platform-specific settings, including preferred default devices.
    #[menuid(skip)]
    #[serde(default)]
    #[serde(flatten)]
    pub platform: PlatformSettings,
}

#[serde_inline_default]
#[derive(Debug, Clone, Serialize, Deserialize, Derivative, MenuToggle, MenuId, TrayChecks)]
#[derivative(Default)]
pub struct ProfileSettings {
    /// Hide Inactive Profiles
    ///
    /// When true, won't show items for inactive profiles.
    ///
    /// Enabled by default to reduce visual clutter.
    #[serde_inline_default(true)]
    #[derivative(Default(value = "true"))]
    pub hide_inactive: bool,
}

#[serde_inline_default]
#[derive(Debug, Clone, Serialize, Deserialize, Derivative)]
#[derivative(Default)]
pub struct MiscSettings {
    #[serde_inline_default(String::from("debug"))]
    #[derivative(Default(value = "String::from(\"debug\")"))]
    pub log_level: String,
    #[serde(default)]
    pub first_time_setup_done: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, MenuToggle, MenuId, TrayChecks)]
pub struct AutoUpdateSettings {
    /// Check for Updates on Startup
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
    pub devices: DeviceSettings,
    #[serde(default)]
    pub profiles: ProfileSettings,
    #[serde(default)]
    pub misc: MiscSettings,
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
