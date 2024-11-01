use fs_err as fs;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    os::windows::fs::FileTypeExt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::AppResult,
    platform::{ConfigEntry, DeviceSet},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppOverride {
    pub process_path: PathBuf,
    #[serde(flatten)]
    pub override_set: DeviceSet<ConfigEntry>,
}

#[derive(Debug)]
pub struct Profiles {
    pub inner: BTreeMap<OsString, AppOverride>,
}

pub const PROFILES_PATH: &str = "profiles";

impl Profiles {
    pub fn build() -> AppResult<Self> {
        let dir = PathBuf::from(PROFILES_PATH);
        let mut profiles = Profiles {
            inner: BTreeMap::new(),
        };

        if dir.exists() {
            profiles.load_from_default_dir()?;
        } else {
            fs::create_dir(dir)?;
        }

        Ok(profiles)
    }
    /// Will replace all existing profiles if successful.
    ///
    /// If an error occurs, the previous profiles are retained.
    pub fn load_from_default_dir(&mut self) -> AppResult<()> {
        let dir = PathBuf::from(PROFILES_PATH);
        if !dir.exists() {
            self.inner.clear();
            fs::create_dir(dir)?;
            return Ok(());
        }
        let mut dir = fs::read_dir(dir)?;
        let mut new_map = BTreeMap::new();
        while let Some(Ok(file)) = dir.next() {
            // Ignore any non .toml's
            if file.path().extension() != Some("toml".as_ref()) {
                continue;
            }
            // Ignore any other directories
            if file.file_type()?.is_dir() || file.file_type()?.is_symlink_dir() {
                continue;
            }
            let (key, value) = try_load_profile(&file.path())?;
            new_map.insert(key, value);
        }

        self.inner = new_map;
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl From<DeviceSet<ConfigEntry>> for AppOverride {
    // Used to build a "profile" for the app's config file's defaults
    fn from(value: DeviceSet<ConfigEntry>) -> Self {
        Self {
            process_path: PathBuf::new(),
            override_set: value,
        }
    }
}

/// Deserializes toml config into an [AppOverride]
fn try_load_profile(path: &Path) -> AppResult<(OsString, AppOverride)> {
    let file_name = path.file_stem().expect("File has no name?").to_owned();
    let profile: AppOverride = toml::from_str(&fs::read_to_string(path)?)?;
    Ok((file_name, profile))
}
