use fs_err::{self as fs, File};
use std::{
    cell::LazyCell,
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::Write,
    os::windows::fs::FileTypeExt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppResult, RedefaulterError},
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

pub const WILDCARD_ANY_PROCESS: LazyCell<&Path> = LazyCell::new(|| Path::new("*"));

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
    pub fn get_mutable_profile(&mut self, profile_name: &str) -> Option<&mut AppOverride> {
        let profile_os_str = OsString::from(profile_name);
        self.inner.get_mut(&profile_os_str)
    }
    pub fn save_profile(&self, profile_name: &str) -> AppResult<()> {
        let profile_os_str = OsString::from(profile_name);
        let profile = self
            .inner
            .get(&profile_os_str)
            .ok_or(RedefaulterError::ProfileNotFound(profile_name.to_owned()))?;

        let profile_toml = toml::to_string(profile)?;
        let mut profile_path = PathBuf::from(PROFILES_PATH);
        profile_path.push(profile_name);
        profile_path.set_extension("toml");

        let mut file = File::create(profile_path)?;
        file.write_all(profile_toml.as_bytes())?;
        file.flush()?;
        file.sync_all()?;

        Ok(())
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
