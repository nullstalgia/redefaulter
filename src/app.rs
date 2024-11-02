use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::{
        mpsc::{self},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use takeable::Takeable;
use tao::event_loop::{ControlFlow, EventLoopProxy};
use tracing::*;
use tray_icon::menu::MenuEvent;

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, ConfigEntry, DeviceSet, Discovered},
    processes::{self, Process},
    profiles::{AppOverride, Profiles},
    settings::Settings,
    tray_menu::{common_ids::*, TrayHelper},
};

#[derive(Debug)]
pub enum CustomEvent {
    ProcessesChanged,
    AudioEndpointUpdate,
    AudioEndpointNotification(AudioEndpointNotification),
    ExitRequested,
}

pub struct App {
    pub endpoints: AudioNightmare,
    pub profiles: Profiles,
    pub process_watcher_handle: Takeable<JoinHandle<AppResult<()>>>,
    pub processes: Arc<DashMap<u32, Process>>,
    // TODO option for this to be
    // - on-launch devices
    // - config'd devices
    // - never taken into account
    pub config_defaults: DeviceSet<ConfigEntry>,
    current_defaults: DeviceSet<Discovered>,

    active_profiles: BTreeMap<OsString, AppOverride>,

    // Option instead of Takeable due to late initialization in EventLoop Init
    // Or possible non-initialization in the case of CLI commands
    pub tray_menu: Option<TrayHelper>,

    pub settings: Settings,
    pub config_path: PathBuf,
    // To prevent fighting with something else messing with devices
    // changes_within_few_seconds: usize,
    // last_change: Instant,
}

// TODO check for wrestling with other apps

impl App {
    pub fn build(event_proxy: EventLoopProxy<CustomEvent>) -> AppResult<Self> {
        let processes = Arc::new(DashMap::new());
        let (process_tx, process_rx) = mpsc::channel();
        let map_clone = Arc::clone(&processes);
        let proxy_clone = event_proxy.clone();

        let process_watcher_handle = thread::spawn(move || {
            processes::process_event_loop(map_clone, process_tx, proxy_clone)
        });

        let (initial_size, instance_already_exists) = process_rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => RedefaulterError::FailedToGetProcesses,
                mpsc::RecvTimeoutError::Disconnected => {
                    panic!("Process watcher was disconnected before sending!")
                }
            })?;

        if instance_already_exists {
            return Err(RedefaulterError::AlreadyExists);
        }

        assert_eq!(initial_size, processes.len());

        let exe_path = std::env::current_exe()?;
        let config_name = exe_path.with_extension("toml");
        let config_name = config_name
            .file_name()
            .expect("Failed to build config name");

        let config_path = PathBuf::from(config_name);

        let settings = Settings::load(&config_path, false)?;

        let endpoints = AudioNightmare::build(Some(event_proxy), Some(&settings.platform))?;

        let config_defaults = settings.platform.default_devices.clone();

        let current_defaults = endpoints.get_current_defaults()?;

        let active_profiles = BTreeMap::new();

        Ok(Self {
            endpoints,
            profiles: Profiles::build()?,
            processes,
            process_watcher_handle: Takeable::new(process_watcher_handle),
            config_defaults,
            current_defaults,
            active_profiles,
            settings,
            config_path,
            tray_menu: None,
        })
    }
    /// Run through all of the running processes and find which ones match the user's profiles
    pub fn determine_active_profiles(&self) -> BTreeMap<&OsString, &AppOverride> {
        let mut active_profiles = BTreeMap::new();
        let total_profiles = self.profiles.inner.len();
        for process in self.processes.iter() {
            if active_profiles.len() == total_profiles {
                break;
            }
            for (profile_name, profile) in self.profiles.inner.iter() {
                if active_profiles.contains_key(profile_name) {
                    continue;
                }
                if process.profile_matches(profile) {
                    active_profiles.insert(profile_name, profile);
                    break;
                }
            }
        }

        active_profiles
    }
    /// Given a list of profiles, will return the roles that need to be changed to fit the active profiles.
    ///
    /// Starting from the lowest priority, lays all of their desired devices
    /// on top of each other, discarding any devices that aren't connected to the system.
    pub fn get_damaged_devices(
        &self,
        active_profiles: &BTreeMap<OsString, AppOverride>,
    ) -> Option<DeviceSet<Discovered>> {
        let config_default_once = std::iter::once(&self.config_defaults);
        let profiles = active_profiles
            .values()
            .map(|p| &p.override_set)
            .chain(config_default_once)
            .rev();

        // TODO Consider a DeviceActions type with Options on the Strings
        let mut device_actions = DeviceSet::<Discovered>::default();

        for profile in profiles {
            self.endpoints
                .overlay_available_devices(&mut device_actions, profile);
        }

        // Don't set a device action for a role that's already properly set
        self.endpoints
            .discard_healthy(&mut device_actions, &self.current_defaults);

        if device_actions.is_empty() {
            None
        } else {
            Some(device_actions)
        }
    }
    /// Check running processes and update active profiles. Also sends new profiles to tray menu.
    ///
    /// Only need to call this when processes change
    // TODO find more graceful way to do the initial/force update
    pub fn update_active_profiles(&mut self, force_update: bool) -> AppResult<()> {
        let new_profiles = self.determine_active_profiles();
        let length_changed = new_profiles.len() != self.active_profiles.len();
        let profiles_changed = new_profiles
            .keys()
            .any(|n| !self.active_profiles.contains_key(*n));
        // Only update menu and local map when damaged
        if force_update || length_changed || profiles_changed {
            self.active_profiles = new_profiles
                .into_iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            if let Some(menu) = self.tray_menu.as_mut() {
                menu.update_menu(
                    self.profiles.len(),
                    &self.active_profiles,
                    &self.settings.platform,
                )?;
            }
        }
        Ok(())
    }
    /// Handle our defined `CustomEvent`s coming in from the platform and our tasks
    pub fn handle_custom_event(
        &mut self,
        event: CustomEvent,
        control_flow: &mut ControlFlow,
    ) -> AppResult<()> {
        use CustomEvent::*;
        match event {
            // Platform notification about endpoint status
            AudioEndpointNotification(notif) => {
                // Dispatch to our platform-specific handler
                self.endpoints.handle_endpoint_notification(notif)?;
                *control_flow = ControlFlow::Wait;
            }
            // Handler processed event, now we can react
            AudioEndpointUpdate => {
                // Changing default audio devices on Windows can trigger several "noisy" events back-to-back,
                // including when we set our desired devices' roles.
                // So instead of reacting to each event instantly (which would cause even more noise we'd react to),
                // we wait a moment for it to settle down.
                let delay = Instant::now() + Duration::from_secs(1);
                debug!("Audio update! Waiting to take action...");
                *control_flow = ControlFlow::WaitUntil(delay);
            }
            // A process has opened or closed
            ProcessesChanged => {
                self.update_active_profiles(false)?;
                self.change_devices_if_needed()?;
                *control_flow = ControlFlow::Wait;
            }
            ExitRequested => {
                *control_flow = ControlFlow::Exit;
            }
        }
        Ok(())
    }
    pub fn update_defaults(&mut self) -> AppResult<()> {
        debug!("Updating defaults!");
        self.current_defaults = self.endpoints.get_current_defaults()?;
        Ok(())
    }
    pub fn change_devices_if_needed(&mut self) -> AppResult<()> {
        if let Some(actions) = self.get_damaged_devices(&self.active_profiles) {
            self.endpoints.change_devices(actions)?;
            self.update_defaults()?;
        }
        Ok(())
    }
    pub fn back_to_default(&self) -> AppResult<()> {
        if let Some(actions) = self.get_damaged_devices(&BTreeMap::new()) {
            self.endpoints.change_devices(actions)?;
        }
        Ok(())
    }
    pub fn reload_profiles(&mut self) -> AppResult<()> {
        self.profiles.load_from_default_dir()?;
        self.update_active_profiles(false)?;
        self.change_devices_if_needed()?;
        Ok(())
    }
}
