use dashmap::DashMap;
use fs_err::{self as fs, File};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    io::Write,
    os::windows::fs::FileTypeExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{ConfigEntry, DeviceSet},
    processes::Process,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppOverride {
    #[serde(rename = "process")]
    pub process_path: PathBuf,
    #[serde(flatten)]
    pub override_set: DeviceSet<ConfigEntry>,
}

#[derive(Debug)]
pub enum TempOverride {
    None,
    PauseActions,
    PreferredDefaults,
    Override(OsString),
}

impl TempOverride {
    /// Returns `true` if the temporary override is currently set to pause actions.
    ///
    /// Returns `false` if no override is set, or if it's an actual profile.
    pub fn is_paused(&self) -> bool {
        matches!(&self, TempOverride::PauseActions)
    }
    /// Returns `true` if no override is set.
    ///
    /// Returns `false` if the override is set to a profile or to pause actions.
    pub fn is_none(&self) -> bool {
        matches!(&self, TempOverride::None)
    }
    /// Returns `true` if no override is set.
    ///
    /// Returns `false` if the override is set to a profile or to pause actions.
    pub fn is_preferred_defaults(&self) -> bool {
        matches!(&self, TempOverride::PreferredDefaults)
    }
    /// Returns a reference to the overridden profile if one is set
    ///
    /// Otherwise, returns `None`.
    pub fn get_profile(&self) -> Option<&OsString> {
        match self {
            Self::Override(profile) => Some(profile),
            _ => None,
        }
    }
    pub fn set_profile<S: Into<OsString>>(&mut self, profile: S) {
        *self = Self::Override(profile.into());
    }
    pub fn set_prefer_defaults(&mut self) {
        *self = Self::PreferredDefaults;
    }
    pub fn set_paused(&mut self) {
        *self = Self::PauseActions;
    }
    pub fn clear(&mut self) {
        *self = Self::None;
    }
}

#[derive(Debug)]
pub struct Profiles {
    pub temporary_override: TempOverride,

    inner: BTreeMap<OsString, AppOverride>,
    active: BTreeSet<OsString>,
    processes: Arc<DashMap<u32, Process>>,
}

pub const PROFILES_PATH: &str = "profiles";

impl Profiles {
    pub fn build(processes: Arc<DashMap<u32, Process>>) -> AppResult<Self> {
        let profiles = Self {
            inner: BTreeMap::new(),
            active: BTreeSet::new(),
            temporary_override: TempOverride::None,
            processes,
        };

        Ok(profiles)
    }
    /// Will replace all existing profiles if successful.
    ///
    /// If an error occurs, the previous profiles are retained.
    pub fn load_from_default_dir(&mut self) -> AppResult<()> {
        self.temporary_override = TempOverride::None;

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
    pub fn active_len(&self) -> usize {
        self.active.len()
    }
    // pub fn none_active(&self) -> bool {
    //     self.active.is_empty()
    // }
    pub fn get_mutable_profile<S: AsRef<OsStr>>(
        &mut self,
        profile_name: S,
    ) -> Option<&mut AppOverride> {
        self.inner.get_mut(profile_name.as_ref())
    }
    // pub fn get_profile(&self, profile_name: &str) -> Option<&AppOverride> {
    //     self.inner.get(OsStr::new(profile_name))
    // }
    pub fn save_profile<S: AsRef<OsStr>>(&self, profile_name: S) -> AppResult<()> {
        let profile = self.inner.get(profile_name.as_ref()).ok_or_else(|| {
            RedefaulterError::ProfileNotFound(profile_name.as_ref().to_os_string())
        })?;

        let profile_toml = toml::to_string(profile)?;
        let mut profile_path = PathBuf::from(PROFILES_PATH);
        profile_path.push(profile_name.as_ref());
        profile_path.set_extension("toml");

        let mut file = File::create(profile_path)?;
        file.write_all(profile_toml.as_bytes())?;
        file.flush()?;
        file.sync_all()?;

        Ok(())
    }
    pub fn new_profile(
        &mut self,
        process_path: PathBuf,
        save_absolute_path: bool,
    ) -> AppResult<()> {
        let Some(process_name) = process_path.file_name() else {
            return Err(RedefaulterError::ProfileEmptyProcessPath(
                process_path.into(),
            ));
        };
        let Some(file_stem) = process_path.file_stem() else {
            return Err(RedefaulterError::ProfileEmptyProcessPath(
                process_path.into(),
            ));
        };

        let new_profile_name = {
            let mut name = OsString::from("99-");
            name.push(file_stem);
            name
        };

        if self.inner.contains_key(&new_profile_name) {
            return Err(RedefaulterError::ProfileAlreadyExists(new_profile_name));
        }

        let output_path: PathBuf = if save_absolute_path {
            process_path
        } else {
            process_name.into()
        };

        let new_override = AppOverride {
            process_path: output_path,
            ..Default::default()
        };

        self.inner.insert(new_profile_name.clone(), new_override);

        self.save_profile(&new_profile_name)?;

        Ok(())
    }
    /// Check running processes and update active profiles.
    ///
    /// Returns `true` if there was a change in active profiles.
    ///
    /// Only need to call this when processes change, not audio endpoints.
    pub fn update_active_profiles(&mut self, force_update: bool) -> bool {
        let active_profiles = match &self.temporary_override {
            TempOverride::Override(temporary_override) => BTreeSet::from([temporary_override]),
            _ => determine_active_profiles(&self.inner, self.processes.as_ref()),
        };

        let new_profiles = active_profiles;
        let length_changed = new_profiles.len() != self.active.len();
        let profiles_changed = new_profiles.iter().any(|n| !self.active.contains(*n));
        // Only update menu and local map when damaged
        if force_update || length_changed || profiles_changed {
            self.active = new_profiles.into_iter().cloned().collect();

            true
        } else {
            false
        }
    }
    // Unwraps should be fine here, I want it to panic anyway if we try
    // to get a profile that doesn't exist anymore.
    pub fn iter_active_override_sets(
        &self,
    ) -> impl DoubleEndedIterator<Item = &DeviceSet<ConfigEntry>> {
        self.active
            .iter()
            .map(|p| &self.inner.get(p).unwrap().override_set)
    }
    pub fn iter_active_profiles(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&OsString, &AppOverride)> {
        self.active.iter().map(|p| (p, self.inner.get(p).unwrap()))
    }
    pub fn iter_inactive_profiles(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&OsString, &AppOverride)> {
        self.inner.iter().filter_map(|(k, v)| {
            if self.active.contains(k) {
                None
            } else {
                Some((k, v))
            }
        })
    }
    pub fn iter_all_profiles(&self) -> impl DoubleEndedIterator<Item = (&OsString, &AppOverride)> {
        self.inner.iter()
    }
}

#[inline]
fn determine_active_profiles<'a>(
    all_profiles: &'a BTreeMap<OsString, AppOverride>,
    running_processes: &'a DashMap<u32, Process>,
) -> BTreeSet<&'a OsString> {
    let mut active_profiles = BTreeSet::new();
    let total_profiles = all_profiles.len();
    // Checking for wildcard ("*"-only) profiles
    let wildcard_any_process = Path::new("*");
    for (profile_name, profile) in all_profiles.iter() {
        if profile.process_path == wildcard_any_process {
            active_profiles.insert(profile_name);
        }
    }

    for process in running_processes.iter() {
        if active_profiles.len() == total_profiles {
            break;
        }
        for (profile_name, profile) in all_profiles.iter() {
            if active_profiles.contains(profile_name) {
                continue;
            }
            if process.profile_matches(profile) {
                active_profiles.insert(profile_name);
                // Not breaking loop to allow other profiles
                // to match on the process
                // break;
            }
        }
    }
    active_profiles
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

/// Deserializes toml config into an [`AppOverride`]
fn try_load_profile(path: &Path) -> AppResult<(OsString, AppOverride)> {
    let file_name = path.file_stem().expect("File has no name?").to_owned();
    let profile: AppOverride = toml::from_str(&fs::read_to_string(path)?).map_err(|e| {
        let err_str = e.to_string();
        let human_span = err_str.lines().next().unwrap_or("").to_owned();
        let reason = e.message().to_owned();
        RedefaulterError::ProfileLoad {
            filename: file_name.clone(),
            human_span,
            reason,
        }
    })?;
    // Dead simple validation
    // Consider Keats/validator if I need more.
    if profile.process_path.as_os_str().is_empty() {
        return Err(RedefaulterError::ProfileEmptyProcessPath(file_name));
    }
    Ok((file_name, profile))
}
