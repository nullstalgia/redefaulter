use std::{
    path::PathBuf,
    sync::{
        mpsc::{self},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use tao::event_loop::{ControlFlow, EventLoopProxy};
use tracing::debug;

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, ConfigEntry, DeviceSet, Discovered},
    processes::{self, Process},
    profiles::{AppOverride, Profiles},
    settings::Config,
};

#[derive(Debug)]
pub enum CustomEvent {
    ProcessesChanged,
    AudioEndpointUpdate,
    AudioEndpointNotification(AudioEndpointNotification),
}

pub struct App {
    pub endpoints: AudioNightmare,
    pub profiles: Profiles,
    pub process_watcher_handle: JoinHandle<AppResult<()>>,
    pub processes: Arc<DashMap<u32, Process>>,
    // TODO option for this to be
    // - on-launch devices
    // - config'd devices
    // - never taken into account
    pub config_defaults: DeviceSet<ConfigEntry>,
    current_defaults: DeviceSet<Discovered>,

    active_profiles: Vec<AppOverride>,

    config: Config,
    config_path: PathBuf,
}

impl App {
    pub fn build(event_proxy: EventLoopProxy<CustomEvent>) -> AppResult<Self> {
        let processes = Arc::new(DashMap::new());
        let (process_tx, process_rx) = mpsc::channel();
        let map_clone = Arc::clone(&processes);
        let proxy_clone = event_proxy.clone();

        let process_watcher_handle = thread::spawn(move || {
            processes::process_event_loop(map_clone, process_tx, proxy_clone)
        });

        let initial_size =
            process_rx
                .recv_timeout(Duration::from_secs(3))
                .map_err(|e| match e {
                    mpsc::RecvTimeoutError::Timeout => RedefaulterError::FailedToGetProcesses,
                    mpsc::RecvTimeoutError::Disconnected => {
                        panic!("Process watcher was disconnected before sending!")
                    }
                })?;

        assert_eq!(initial_size, processes.len());

        let exe_path = std::env::current_exe()?;
        let config_name = exe_path.with_extension("toml");
        let config_name = config_name
            .file_name()
            .expect("Failed to build config name");
        let config_path = PathBuf::from(config_name);

        let config = Config::load(&config_path, false)?;

        let endpoints = AudioNightmare::build(Some(event_proxy), Some(&config.devices))?;

        let config_defaults = config.devices.default_devices.clone();

        let current_defaults = endpoints.get_current_defaults()?;

        let active_profiles = Vec::new();

        Ok(Self {
            endpoints,
            profiles: Profiles::build()?,
            processes,
            process_watcher_handle,
            config_defaults,
            current_defaults,
            active_profiles,
            config,
            config_path,
        })
    }
    /// Run through all of the running processes and find which ones match the user's profiles
    pub fn determine_active_profiles(&self) -> Vec<AppOverride> {
        // TODO make more memory efficient
        let mut remaining_profiles = self.profiles.inner.clone();
        let mut active_profiles = Vec::new();
        // let total_profiles = self.profiles.inner.len();
        for process in self.processes.iter() {
            if remaining_profiles.len() == 0 {
                break;
            }
            // TODO not check already matched profiles
            for (profile_name, profile) in self.profiles.inner.iter() {
                if process.profile_matches(&profile) {
                    if let Some((_, val)) = remaining_profiles.remove_entry(profile_name) {
                        active_profiles.push(val);
                    };
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
        active_profiles: Vec<AppOverride>,
    ) -> Option<DeviceSet<Discovered>> {
        let config_default_once = std::iter::once(self.config_defaults.clone().into());
        let profiles = active_profiles.into_iter().chain(config_default_once).rev();

        // TODO Consider a DeviceActions type with Options on the Strings
        let mut device_actions = DeviceSet::<Discovered>::default();

        for profile in profiles {
            self.endpoints
                .overlay_available_devices(&mut device_actions, &profile.override_set);
        }

        self.endpoints
            .discard_healthy(&mut device_actions, &self.current_defaults);

        if device_actions.is_empty() {
            None
        } else {
            Some(device_actions)
        }
    }
    fn update_active_profiles(&mut self) {
        self.active_profiles = self.determine_active_profiles();
    }
    pub fn generate_device_actions(&self) -> Option<DeviceSet<Discovered>> {
        self.get_damaged_devices(self.active_profiles.clone())
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
                // Changing default audio devices on windows can trigger several "noisy" events back-to-back,
                // including when we send our own desired devices.
                // So instead of instantly reacting to each one, we wait a moment for it to settle down.
                let delay = Instant::now() + Duration::from_secs(1);
                debug!("Audio update! Waiting to take action...");
                *control_flow = ControlFlow::WaitUntil(delay);
            }
            // A process has opened or closed
            ProcessesChanged => {
                // Only need to call this when processes change
                self.update_active_profiles();
                self.change_devices_if_needed()?;
                *control_flow = ControlFlow::Wait;
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
        if let Some(actions) = self.generate_device_actions() {
            self.endpoints.change_devices(actions)?;
            self.update_defaults()?;
        }
        Ok(())
    }
}
