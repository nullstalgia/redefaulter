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
use takeable::Takeable;
use tao::event_loop::{ControlFlow, EventLoopProxy};
use tracing::*;
use tray_icon::TrayIcon;

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, DeviceSet, Discovered},
    popups::settings_load_failed_popup,
    processes,
    profiles::Profiles,
    settings::Settings,
};

#[derive(Debug)]
pub enum CustomEvent {
    ProcessesChanged,
    AudioEndpointUpdate,
    AudioEndpointNotification(AudioEndpointNotification),
    ReloadProfiles,
    ExitRequested,
}

pub struct App {
    pub endpoints: AudioNightmare,
    pub profiles: Profiles,
    pub process_watcher_handle: Takeable<JoinHandle<AppResult<()>>>,
    // TODO option for this to be
    // - on-launch devices
    // - config'd devices
    // - never taken into account
    // pub config_defaults: DeviceSet<ConfigEntry>,
    // TODO move out of App?
    pub current_defaults: DeviceSet<Discovered>,

    // Option instead of Takeable due to late initialization in EventLoop Init
    // Or possible non-initialization in the case of CLI commands
    pub tray_menu: Option<TrayIcon>,

    pub event_proxy: EventLoopProxy<CustomEvent>,

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
            return Err(RedefaulterError::AlreadyRunning);
        }

        assert_eq!(initial_size, processes.len());

        let exe_path = std::env::current_exe()?;
        let config_name = exe_path.with_extension("toml");
        let config_name = config_name
            .file_name()
            .expect("Failed to build config name");

        let config_path = PathBuf::from(config_name);

        let settings = match Settings::load(&config_path, false) {
            Ok(settings) => settings,
            Err(RedefaulterError::TomlDe(e)) => {
                error!("Settings load failed: {e}");
                // TODO move human_span formatting into thiserror fmt attr?
                let err_str = e.to_string();
                // Only grabbing the top line since it has the human-readable line and column information
                // (the error's span method is in *bytes*, not lines and columns)
                let human_span = err_str.lines().next().unwrap_or("").to_owned();
                let reason = e.message().to_owned();
                let new_err = RedefaulterError::FailedSettingsLoad { human_span, reason };

                settings_load_failed_popup(new_err);
            }
            Err(e) => {
                error!("Settings load failed: {e}");
                settings_load_failed_popup(e);
            }
        };

        let endpoints = AudioNightmare::build(Some(event_proxy.clone()), Some(&settings.platform))?;

        // let config_defaults = settings.platform.default_devices.clone();

        let current_defaults = endpoints.get_current_defaults()?;

        let mut profiles = Profiles::build(processes)?;

        if let Err(e) = profiles.load_from_default_dir() {
            crate::popups::profile_load_failed_popup(e, event_proxy.clone());
        };

        Ok(Self {
            endpoints,
            profiles,
            process_watcher_handle: Takeable::new(process_watcher_handle),
            // config_defaults,
            current_defaults,
            event_proxy,
            settings,
            config_path,
            tray_menu: None,
        })
    }
    /// Given a list of profiles, will return the roles that need to be changed to fit the active profiles.
    ///
    /// Starting from the lowest priority, lays all of their desired devices
    /// on top of each other, discarding any devices that aren't connected to the system.
    ///
    /// Returns None if the resulting devices are the same as the current de.
    pub fn get_damaged_devices(&self, only_config_default: bool) -> Option<DeviceSet<Discovered>> {
        let config_default_once = std::iter::once(&self.settings.platform.default_devices);

        let active_overrides = self
            .profiles
            .get_active_override_sets()
            // Discard all active overrides if we're just shutting down
            // (There might be a nicer way to do this, but this is concise and doesn't have type mismatch issues)
            .filter(|_| !only_config_default)
            .chain(config_default_once)
            .rev();

        // TODO Consider a DeviceActions type with Options on the Strings
        let mut device_actions = DeviceSet::<Discovered>::default();

        for profile in active_overrides {
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
    // TODO find more graceful way to do the initial/force update
    pub fn update_active_profiles(&mut self, force_update: bool) -> AppResult<()> {
        let profiles_changed = self.profiles.update_active_profiles(force_update)?;
        if profiles_changed {
            self.update_tray_menu()?;
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
            ReloadProfiles => {
                debug!("Reload Profiles event recieved!");
                self.reload_profiles()?;
            }
        }
        Ok(())
    }
    // pub fn event_proxy(&self) -> EventLoopProxy<CustomEvent> {
    //     self.event_proxy.clone()
    // }
    pub fn update_defaults(&mut self) -> AppResult<()> {
        debug!("Updating defaults!");
        self.current_defaults = self.endpoints.get_current_defaults()?;
        Ok(())
    }
    pub fn change_devices_if_needed(&mut self) -> AppResult<()> {
        if let Some(actions) = self.get_damaged_devices(false) {
            self.endpoints.change_devices(actions)?;
            self.update_defaults()?;
        }
        Ok(())
    }
    pub fn back_to_default(&self) -> AppResult<()> {
        if let Some(actions) = self.get_damaged_devices(true) {
            self.endpoints.change_devices(actions)?;
        }
        Ok(())
    }
    /// If deserializing a profile fails, the previous profiles are kept as-is in memory.
    pub fn reload_profiles(&mut self) -> AppResult<()> {
        if let Err(e) = self.profiles.load_from_default_dir() {
            crate::popups::profile_load_failed_popup(e, self.event_proxy.clone());
            return Ok(());
        };
        self.update_active_profiles(false)?;
        self.change_devices_if_needed()?;
        Ok(())
    }
}
