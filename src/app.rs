use std::{
    collections::BTreeMap,
    ffi::OsString,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use dashmap::DashMap;
use tao::event_loop::{ControlFlow, EventLoopProxy};

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, ConfigEntry, DeviceSet, Discovered},
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
    pub default_set: DeviceSet<Discovered>,
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
    pub fn get_damaged_devices(&self, active_profiles: Vec<AppOverride>) -> DeviceSet<Discovered> {
        // let config_default_once = std::iter::once(self.default_set.clone().into());
        // let profiles = active_profiles.into_iter().chain(config_default_once).rev();
        let profiles = active_profiles.into_iter().rev();

        let compare_against_me = self.endpoints.get_current_defaults().unwrap();

        // TODO Consider a DeviceActions type with Options on the Strings
        let mut device_actions = DeviceSet::<Discovered>::default();

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
    pub fn test(&self) -> DeviceSet<Discovered> {
        let need_to_change = self.get_damaged_devices(self.active_profiles.clone());

        need_to_change
    }
    // fn change_devices(&self, new_devices: DeviceSet<Discovered>) -> AppResult<()> {
    //     Ok(())
    // }
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
                *control_flow = ControlFlow::WaitUntil(delay);
            }
            // A process has opened or closed
            ProcessesChanged => {
                self.update_active_profiles();
                //trigger changes
                self.change_devices_if_needed()?;
                *control_flow = ControlFlow::Wait;
            }
        }
        Ok(())
    }
    pub fn change_devices_if_needed(&self) -> AppResult<()> {
        self.endpoints.change_devices(self.test())?;
        Ok(())
    }
}
