use std::{
    collections::BTreeMap,
    ffi::OsString,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use dashmap::DashMap;
use tao::event_loop::EventLoopProxy;

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, DeviceSet},
    processes::{self, Process},
    profiles::{AppOverride, Profiles},
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
    pub process_rx: Receiver<usize>,
    // TODO option for this to be
    // - on-launch devices
    // - config'd devices
    // - never taken into account
    pub default_set: DeviceSet,
    active_profiles: Vec<AppOverride>,
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

        let endpoints = AudioNightmare::build(Some(event_proxy))?;

        // TODO later this should be the defaults set by the user in the config
        let default_set = endpoints.get_current_defaults()?;

        let active_profiles = Vec::new();

        Ok(Self {
            endpoints,
            profiles: Profiles::build()?,
            processes,
            process_watcher_handle,
            process_rx,
            default_set,
            active_profiles,
        })
    }
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
    pub fn get_damaged_devices(&self, active_profiles: Vec<AppOverride>) -> DeviceSet {
        let config_default_once = std::iter::once(self.default_set.clone().into());
        let profiles = active_profiles.into_iter().chain(config_default_once).rev();

        let compare_against_me = self.default_set.clone();

        // TODO Consider a DeviceActions type with Options on the Strings
        let mut device_actions = DeviceSet::default();

        for profile in profiles {
            self.endpoints
                .overlay_available_devices(&mut device_actions, &profile.override_set);
        }

        self.endpoints
            .discard_healthy(&mut device_actions, &compare_against_me);

        device_actions
    }
    fn update_active_profiles(&mut self) {
        self.active_profiles = self.determine_active_profiles();
    }
    pub fn test(&self) -> DeviceSet {
        let need_to_change = self.get_damaged_devices(self.active_profiles.clone());

        need_to_change
    }
    fn change_devices(&self, new_devices: DeviceSet) -> AppResult<()> {
        Ok(())
    }
    pub fn handle_custom_event(&mut self, event: CustomEvent) -> AppResult<()> {
        use CustomEvent::*;
        match event {
            AudioEndpointNotification(notif) => {
                self.endpoints.handle_endpoint_notification(notif)?
            }
            AudioEndpointUpdate => {
                //trigger changes
                self.endpoints.change_devices(self.test())?;
            }
            ProcessesChanged => {
                self.update_active_profiles();
                //trigger changes
                self.endpoints.change_devices(self.test())?;
            }
        }
        Ok(())
    }
}
