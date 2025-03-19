use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use auto_launch::AutoLaunch;
use dashmap::DashMap;
use muda::MenuEventReceiver;
use takeable::Takeable;
use tao::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopProxy},
};
use tracing::*;
use tray_icon::{Icon, TrayIcon};

use crate::{
    errors::{AppResult, RedefaulterError},
    platform::{AudioEndpointNotification, AudioNightmare, DeviceSet, Discovered},
    popups::{
        first_time_popups, profile_exists_popup, settings_load_failed_popup, FirstTimeChoice,
    },
    processes::{self, LockFile},
    profiles::Profiles,
    settings::Settings,
    updates::{UpdateHandle, UpdateReply, UpdateState},
};

#[derive(Debug)]
pub enum CustomEvent {
    ProcessesChanged,
    AudioEndpointUpdate,
    AudioEndpointNotification(AudioEndpointNotification),
    UpdateReply(UpdateReply),
    FirstTimeChoice(FirstTimeChoice),
    NewProfile(PathBuf, bool),
    ReloadProfiles,
    ExitRequested,
}

pub type AppEventProxy = EventLoopProxy<CustomEvent>;

pub struct App {
    pub endpoints: AudioNightmare,
    pub profiles: Profiles,
    pub process_watcher_handle: Takeable<JoinHandle<AppResult<()>>>,
    // TODO move out of App?
    pub current_defaults: DeviceSet<Discovered>,

    // Option instead of Takeable due to late initialization in EventLoop Init
    // Or possible non-initialization in the case of CLI commands
    pub tray_menu: Option<TrayIcon>,
    pub normal_icon: Option<Icon>,
    pub update_icon: Option<Icon>,

    pub event_proxy: AppEventProxy,

    pub lock_file: Takeable<LockFile>,

    pub updates: Takeable<UpdateHandle>,
    pub update_state: UpdateState,

    pub auto_launch: Option<AutoLaunch>,

    // pub lock_file_path: PathBuf,
    pub settings: Settings,
    pub config_path: PathBuf,
    // To prevent fighting with something else messing with devices
    // changes_within_few_seconds: usize,
    // last_change: Instant,
}

// TODO check for wrestling with other apps

impl App {
    pub fn build(event_proxy: AppEventProxy) -> AppResult<Self> {
        let processes = Arc::new(DashMap::new());
        let (process_tx, process_rx) = mpsc::channel();
        let map_clone = Arc::clone(&processes);
        let proxy_clone = event_proxy.clone();

        let lock_file = LockFile::build()?;

        let process_watcher_handle = thread::spawn(move || {
            processes::process_event_loop(map_clone, process_tx, proxy_clone)
        });

        let initial_size = match process_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(size) => size,
            Err(RecvTimeoutError::Timeout) => {
                return Err(RedefaulterError::ProcessWatcherSetup("Timeout".to_string()));
            }
            Err(RecvTimeoutError::Disconnected) => {
                let result = process_watcher_handle.join();
                let output = format!("{result:?}");
                return Err(RedefaulterError::ProcessWatcherSetup(output));
            }
        };

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
                let new_err = RedefaulterError::SettingsLoad { human_span, reason };

                settings_load_failed_popup(new_err, lock_file);
            }
            Err(e) => {
                error!("Settings load failed: {e}");
                settings_load_failed_popup(e, lock_file);
            }
        };

        let endpoints =
            AudioNightmare::build(Some(event_proxy.clone()), Some(&settings.devices.platform))?;

        // let config_defaults = settings.platform.default_devices.clone();

        let current_defaults = endpoints.get_current_defaults()?;

        let mut profiles = Profiles::build(processes)?;

        if let Err(e) = profiles.load_from_default_dir() {
            crate::popups::profile_load_failed_popup(e, event_proxy.clone());
        };

        let updates = UpdateHandle::new(event_proxy.clone());

        let auto_launch = if let Some(path) = exe_path.to_str() {
            auto_launch::AutoLaunchBuilder::new()
                .set_app_name("redefaulter")
                .set_app_path(path)
                .build()
                .ok()
        } else {
            warn!("Couldn't convert exe path to Unicode for Auto Launch! {exe_path:?}");
            None
        };

        Ok(Self {
            endpoints,
            profiles,
            update_state: UpdateState::Idle,
            process_watcher_handle: Takeable::new(process_watcher_handle),
            // config_defaults,
            current_defaults,
            event_proxy,
            settings,
            config_path,
            lock_file: Takeable::new(lock_file),
            // lock_file_path,
            tray_menu: None,
            normal_icon: None,
            update_icon: None,
            updates: Takeable::new(updates),
            auto_launch,
        })
    }
    /// Given a list of profiles, will return the roles that need to be changed to fit the active profiles.
    ///
    /// Starting from the lowest priority, lays all of their desired devices
    /// on top of each other, discarding any devices that aren't connected to the system.
    ///
    /// Returns `None` if the resulting devices are the same as the current devices,
    /// or if the user has actions temporarily paused.
    pub fn get_damaged_devices(&self, only_config_default: bool) -> Option<DeviceSet<Discovered>> {
        // If the user has the pause override active, we shouldn't trigger
        // any device change actions.
        //
        // Noted side effect: If the user is closing the app and has actions paused,
        // we won't set devices back to their configured defaults.
        if self.profiles.temporary_override.is_paused() {
            return None;
        }

        let config_default_once = std::iter::once(&self.settings.devices.platform.default_devices);

        let profile_overrides = self
            .profiles
            .iter_active_override_sets()
            // Discard all active overrides if we're just shutting down
            // (There might be a nicer way to do this, but this is concise and doesn't have type mismatch issues)
            .filter(|_| !only_config_default);

        let active_overrides = config_default_once.chain(profile_overrides);

        // TODO Consider a DeviceActions type with Options on the Strings/Devices?
        let mut device_actions = self.current_defaults.clone();

        for profile in active_overrides {
            self.endpoints.overlay_available_devices(
                &mut device_actions,
                profile,
                self.settings.devices.fuzzy_match_names,
            );
        }

        // Clears device actions for roles that're already properly set
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
        let profiles_changed = self.profiles.update_active_profiles(force_update);
        if profiles_changed {
            self.update_tray_menu()?;
        }
        Ok(())
    }
    pub fn handle_tao_event(
        &mut self,
        event: Event<CustomEvent>,
        control_flow: &mut ControlFlow,
        menu_channel: &MenuEventReceiver,
    ) -> AppResult<()> {
        if self.process_watcher_handle.is_finished() {
            let result = self.process_watcher_handle.take().join();
            let output = format!("{result:?}");
            return Err(RedefaulterError::ProcessWatcher(output));
        }
        match event {
            // Note: If the user clicks on the icon before this event finishes,
            // the tray menu and icon will become stuck and uninteractable.
            // Might wanna open an issue about it later.
            Event::NewEvents(StartCause::Init) => {
                *control_flow = ControlFlow::Wait;
                self.tray_menu = Some(self.build_tray_late()?);
                self.update_active_profiles(true)?;
                self.change_devices_if_needed()?;
                if self.settings.updates.allow_checking_for_updates {
                    self.updates.query_latest();
                }
                if !self.settings.misc.first_time_setup_done {
                    first_time_popups(
                        self.current_defaults.clone(),
                        self.event_proxy.clone(),
                        self.auto_launch.is_some(),
                    );
                }
            }
            Event::UserEvent(event) => {
                // println!("user event: {event:?}");
                let t = Instant::now();
                self.handle_custom_event(event, control_flow)?;
                debug!("Event handling took {:?}", t.elapsed());
            }
            // Timeout for an audio device reaction finished waiting
            // (nothing else right now uses WaitUntil)
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                debug!("Done waiting for audio endpoint timeout!");
                self.update_defaults()?;
                self.change_devices_if_needed()?;
                self.update_tray_menu()?;
                *control_flow = ControlFlow::Wait;
            }
            Event::NewEvents(StartCause::WaitCancelled {
                requested_resume, ..
            }) => {
                // We had a wait time, but something else came in before we could finish waiting,
                // so just check now
                if requested_resume.is_some() {
                    self.update_defaults()?;
                    self.update_tray_menu()?;
                    *control_flow = ControlFlow::Wait;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            Event::LoopDestroyed => {
                debug!("Event loop destroyed!");
                self.kill_tray_menu();
                self.back_to_default()
                    .expect("Failed to return devices to default!");
                self.lock_file.take();
            }
            _ => (),
        }
        while let Ok(event) = menu_channel.try_recv() {
            debug!("Menu Event: {event:?}");
            let t = Instant::now();
            self.handle_tray_menu_event(event, control_flow)?;
            debug!("Tray event handling took {:?}", t.elapsed());
        }

        // if let Some(updates) = self.updates.as_ref() {
        //     if let Ok(reply) = updates.reply_rx.try_recv() {

        //     }
        // }

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
            FirstTimeChoice(choice) => {
                self.handle_first_time_choice(choice)?;
            }
            UpdateReply(reply) => {
                debug!("Update Event: {reply:?}");
                self.handle_update_reply(reply)?;
            }
            NewProfile(process_path, save_absolute_path) => {
                if let Err(e) = self.profiles.new_profile(process_path, save_absolute_path) {
                    profile_exists_popup(e);
                    return Ok(());
                };
                self.update_active_profiles(false)?;
                self.change_devices_if_needed()?;
            }
        }
        Ok(())
    }
    fn handle_first_time_choice(&mut self, choice: FirstTimeChoice) -> AppResult<()> {
        match choice {
            FirstTimeChoice::SetupFinished => {
                self.settings.misc.first_time_setup_done = true;
            }
            FirstTimeChoice::PlatformChoice(choice) => {
                self.handle_platform_first_time_choice(choice)?;
            }
            FirstTimeChoice::UseCurrentDefaults => {
                self.endpoints.copy_all_roles(
                    &mut self.settings.devices.platform.default_devices,
                    &self.current_defaults,
                    self.settings.devices.fuzzy_match_names,
                    self.settings.devices.save_guid,
                );
            }
            FirstTimeChoice::UpdateCheckConsent(consent) => {
                if consent {
                    self.settings.updates.allow_checking_for_updates = true;
                    self.updates.query_latest();
                } else {
                    self.settings.updates.allow_checking_for_updates = false;
                    self.updates.take();
                }
            }
            FirstTimeChoice::AutoLaunch(enabled) => {
                if let Err(e) = self.set_auto_launch(enabled) {
                    error!("{e}");
                } else {
                    self.update_tray_menu()?;
                }
            }
        }
        self.settings.save(&self.config_path)?;
        Ok(())
    }
    /// Query the OS for the current default endpoints.
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
    /// Meant to be run on shutdown (via error or user request) to attempt to set the default devices back
    /// to the global defaults defined in the config.
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
        self.update_tray_menu()?;
        Ok(())
    }
    pub fn set_auto_launch(&self, enabled: bool) -> AppResult<()> {
        if let Some(handle) = self.auto_launch.as_ref() {
            if enabled {
                handle.enable()?;
            } else {
                handle.disable()?;
            }
            Ok(())
        } else {
            Err(RedefaulterError::AutoLaunchInit)
        }
    }
    pub fn get_auto_launch_enabled(&self) -> AppResult<bool> {
        if let Some(handle) = self.auto_launch.as_ref() {
            Ok(handle.is_enabled()?)
        } else {
            Err(RedefaulterError::AutoLaunchInit)
        }
    }
}
