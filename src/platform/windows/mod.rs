use std::collections::BTreeMap;

use devices::WindowsAudioDevice;
use menu_macro::*;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use shadowplay::ShadowPlayHandle;
use takeable::Takeable;
use tracing::*;
use wasapi::*;
use windows::{
    core::PWSTR,
    Win32::{
        Media::Audio::*,
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
    },
};
use windows_core::Interface;

use crate::{
    app::{AppEventProxy, CustomEvent},
    args::ListSubcommand,
    errors::{AppResult, RedefaulterError},
};

use device_notifications::{NotificationCallbacks, WindowsAudioNotification};
use policy_config::{IPolicyConfig, PolicyConfig};

use super::{ConfigEntry, Discovered};

pub mod device_notifications;
pub mod devices;
pub use devices::{ConfigDevice, DeviceRole, DeviceSet, DiscoveredDevice};

mod device_ser;
mod policy_config;
mod shadowplay;

pub struct AudioNightmare {
    /// Interface to query endpoints through
    device_enumerator: Takeable<IMMDeviceEnumerator>,
    /// Interface to change endpoints through
    policy_config: Takeable<IPolicyConfig>,
    /// Client object for endpoint notifications from Windows
    device_callbacks: Option<NotificationCallbacks>,
    /// Existing devices attached to the host
    pub playback_devices: BTreeMap<String, DiscoveredDevice>,
    /// Existing devices attached to the host
    pub recording_devices: BTreeMap<String, DiscoveredDevice>,
    /// Regex to help with fuzzy-matching against devices with numeric prefixes
    regex_windows_numeric_prefix: Regex,
    /// Used to tell `App` that something has changed
    event_proxy: Option<AppEventProxy>,
    /// When `true`, *all* actions taken towards the Console/Multimedia Role
    /// will be applied to the Communications Role
    pub unify_communications_devices: bool,
    /// When present, will be used to attempt to keep the ShadowPlay recorded device
    /// the same as the Default `Recording` device.
    shadowplay: Option<ShadowPlayHandle>,
}
impl Drop for AudioNightmare {
    fn drop(&mut self) {
        // These need to get dropped first, otherwise the Uninit call will run while they're still in memory
        // and cause an ACCESS_VIOLATION when it tries
        self.policy_config.take();
        if let Some(callbacks) = self.device_callbacks.take() {
            let device_enumerator = self.device_enumerator.take();
            let _ = callbacks.unregister_to_enumerator(&device_enumerator);
        }
        // https://github.com/microsoft/windows-rs/issues/1169#issuecomment-926877227
        // unsafe {
        //     CoUninitialize();
        // }
    }
}
impl AudioNightmare {
    pub fn build(
        event_proxy: Option<AppEventProxy>,
        config: Option<&PlatformSettings>,
    ) -> AppResult<Self> {
        let default = PlatformSettings::default();
        let config = config.unwrap_or(&default);

        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }

        let policy_config: IPolicyConfig =
            unsafe { CoCreateInstance(&PolicyConfig, None, CLSCTX_ALL) }?;
        let device_enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;

        let mut playback_devices = BTreeMap::new();
        let mut recording_devices = BTreeMap::new();

        let initial_playback = DeviceCollection::new(&Direction::Render)?;

        for device in &initial_playback {
            let device: DiscoveredDevice = device.expect("Couldn't get device").try_into()?;
            playback_devices.insert(device.guid.clone(), device);
        }

        // println!("{playback_devices:#?}");

        let initial_recording = DeviceCollection::new(&Direction::Capture)?;

        for device in &initial_recording {
            let device: DiscoveredDevice = device.expect("Couldn't get device").try_into()?;
            recording_devices.insert(device.guid.clone(), device);
        }

        // println!("{recording_devices:#?}");

        let mut device_callbacks = None;

        if let Some(proxy) = event_proxy.as_ref() {
            let client = NotificationCallbacks::new(proxy.clone());
            client.register_to_enumerator(&device_enumerator)?;
            device_callbacks = Some(client);
        }

        // This regex matches numeric prefix patterns in the form of ' (#- ',
        // which are used as prefixes by Windows for differentiating device instances.
        // The regex's goal is to match to these prefixes so they
        // can be ignored/removed during fuzzy device matching/saving.
        let regex_windows_numeric_prefix = Regex::new(r" \(\d+- ").expect("Regex failed to build");

        let unify_communications_devices = config.unify_communications_devices;

        let shadowplay = if config.shadowplay_support {
            match ShadowPlayHandle::build() {
                Ok(handle) => Some(handle),
                Err(e) => {
                    error!("{e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            policy_config: Takeable::new(policy_config),
            device_enumerator: Takeable::new(device_enumerator),
            device_callbacks,
            // callback_rx: rx,
            playback_devices,
            recording_devices,
            regex_windows_numeric_prefix,
            event_proxy,
            unify_communications_devices,
            shadowplay,
        })
    }
    pub fn set_device_role(&self, device_id: &str, role: &Role) -> AppResult<()> {
        let wide_id = device_id.to_wide();
        unsafe {
            self.policy_config
                .SetDefaultEndpoint(wide_id.as_pwstr(), role.to_owned().into())
        }?;
        Ok(())
    }
    pub fn print_devices(&self, categories: &ListSubcommand) {
        let (playback, recording) = {
            // If neither specified, do both
            if !categories.playback && !categories.recording {
                (true, true)
            } else {
                (categories.playback, categories.recording)
            }
        };
        if categories.profile_format {
            self.print_profile_format(playback, recording);
        } else {
            self.print_human_readable(playback, recording);
        }
    }
    fn print_profile_format(&self, playback: bool, recording: bool) {
        if playback {
            println!("Playback devices: ");
            for device in self.playback_devices.values() {
                println!(
                    "{}",
                    serde_json::to_string(device).expect("Failed to serialize profile")
                );
            }
        }
        if recording {
            if playback {
                println!("----------");
            }
            println!("Recording devices: ");

            for device in self.recording_devices.values() {
                println!(
                    "{}",
                    serde_json::to_string(device).expect("Failed to serialize profile")
                );
            }
        }
    }
    fn print_human_readable(&self, playback: bool, recording: bool) {
        let max_len = self
            .playback_devices
            .iter()
            .chain(self.recording_devices.iter())
            .map(|device| device.1.human_name.len())
            .max()
            .unwrap_or(0);

        if playback {
            println!("Playback devices: ");
            for device in &self.playback_devices {
                println!(
                    "{:<width$} - {}",
                    device.1.human_name,
                    device.1.guid,
                    width = max_len
                );
            }
        }
        if recording {
            if playback {
                println!("----------");
            }
            println!("Recording devices: ");

            for device in &self.recording_devices {
                println!(
                    "{:<width$} - {}",
                    device.1.human_name,
                    device.1.guid,
                    width = max_len
                );
            }
        }
    }
    fn add_endpoint(&mut self, id: &str, known_to_be_active: bool) -> AppResult<()> {
        let id = String::from(id).to_wide();
        let device: IMMDevice = unsafe { self.device_enumerator.GetDevice(id.as_pwstr())? };
        let endpoint: IMMEndpoint = device.cast()?;
        let direction: Direction = unsafe { endpoint.GetDataFlow()? }.try_into()?;
        info!("New {direction:?} device!");
        let device: Device = Device::custom(device, direction);

        if !known_to_be_active {
            let state = device.get_state()?;

            use DeviceState::*;
            match state {
                Active => (),
                Disabled | NotPresent | Unplugged => return Ok(()),
            }
        }

        let device: DiscoveredDevice = device.try_into()?;

        match direction {
            Direction::Render => {
                if let Some(old) = self.playback_devices.insert(device.guid.clone(), device) {
                    warn!("Playback device already existed? {old:?}");
                };
            }
            Direction::Capture => {
                if let Some(old) = self.recording_devices.insert(device.guid.clone(), device) {
                    warn!("Recording device already existed? {old:?}");
                };
            }
        }

        Ok(())
    }
    fn remove_endpoint(&mut self, id: &str) {
        if self.playback_devices.remove(id).is_none() {
            self.recording_devices.remove(id);
        }
    }
    pub fn handle_endpoint_notification(
        &mut self,
        notif: WindowsAudioNotification,
    ) -> AppResult<()> {
        use WindowsAudioNotification::*;
        debug!("{notif:?}");
        match notif {
            DeviceAdded { id } => self.add_endpoint(&id, false)?,
            DeviceRemoved { id } => self.remove_endpoint(&id),
            DeviceStateChanged { id, state } => match state.0 {
                // https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-state-xxx-constants
                // ACTIVE
                0x1 => self.add_endpoint(&id, true)?,
                // DISABLED | NOTPRESENT | UNPLUGGED
                0x2 | 0x4 | 0x8 => self.remove_endpoint(&id),
                _ => panic!("Got unexpected state from DeviceStateChanged!"),
            },
            DefaultDeviceChanged { .. } => (),
        }
        if let Some(proxy) = self.event_proxy.as_ref() {
            proxy
                .send_event(CustomEvent::AudioEndpointUpdate)
                .map_err(|_| RedefaulterError::EventLoopClosed)?;
        }
        Ok(())
    }
    /// Gets device by name, strict matching
    fn device_by_name<'a>(
        &'a self,
        direction: &Direction,
        name: &str,
    ) -> Option<&'a DiscoveredDevice> {
        if name.is_empty() {
            return None;
        }
        let find = |map: &'a BTreeMap<String, DiscoveredDevice>| -> Option<&'a DiscoveredDevice> {
            map.values().find(|d| d.human_name == name)
        };
        match direction {
            Direction::Render => find(&self.playback_devices),
            Direction::Capture => find(&self.recording_devices),
        }
    }
    /// Gets device by name fuzzily.
    ///
    /// Searches in the specified set of devices,
    /// returning the first device that matches,
    /// ignoring numeric prefixes in desired device name or discovered device names.
    fn device_by_name_fuzzy<'a>(
        &'a self,
        direction: &Direction,
        name: &str,
    ) -> Option<&'a DiscoveredDevice> {
        if name.is_empty() {
            return None;
        }
        let desired_normalized = self.regex_windows_numeric_prefix.replace(name, " (");
        let find = |map: &'a BTreeMap<String, DiscoveredDevice>| -> Option<&'a DiscoveredDevice> {
            map.values().find(|d| {
                desired_normalized
                    == self
                        .regex_windows_numeric_prefix
                        .replace(&d.human_name, " (")
            })
        };
        match direction {
            Direction::Render => find(&self.playback_devices),
            Direction::Capture => find(&self.recording_devices),
        }
    }
    fn device_by_guid(&self, direction: &Direction, guid: &str) -> Option<&DiscoveredDevice> {
        match direction {
            Direction::Render => self.playback_devices.get(guid),
            Direction::Capture => self.recording_devices.get(guid),
        }
    }
    pub fn get_role_default(&self, role: &DeviceRole) -> AppResult<DiscoveredDevice> {
        let target_role: Role = role.into();
        let target_direction: Direction = role.into();
        let default_device: DiscoveredDevice =
            get_default_device_for_role(&target_direction, &target_role)?.try_into()?;
        Ok(default_device)
    }
    // Bit of a slow operation, queries Windows for all four roles individually.
    pub fn get_current_defaults(&self) -> AppResult<DeviceSet<Discovered>> {
        use wasapi::Direction::*;
        use wasapi::Role::*;
        let playback: DiscoveredDevice =
            get_default_device_for_role(&Render, &Console)?.try_into()?;
        let playback_comms: DiscoveredDevice =
            get_default_device_for_role(&Render, &Communications)?.try_into()?;
        let recording: DiscoveredDevice =
            get_default_device_for_role(&Capture, &Console)?.try_into()?;
        let recording_comms: DiscoveredDevice =
            get_default_device_for_role(&Capture, &Communications)?.try_into()?;

        // Tacking on ShadowPlay's action here, since this runs not too frequently
        // (mainly only when devices change),
        // but enough to not lag behind.
        // Plus we just got the most recent Recording device, which is the one we want.
        if let Some(shadowplay) = self.shadowplay.as_ref() {
            shadowplay.microphone_change(&recording.guid);
        }

        Ok(DeviceSet {
            playback,
            playback_comms,
            recording,
            recording_comms,
        })
    }
    /// Tries to find device by GUID first, and then by name
    pub fn try_find_device(
        &self,
        direction: &Direction,
        needle: &ConfigDevice,
        fuzzy_match_names: bool,
    ) -> Option<&DiscoveredDevice> {
        self.device_by_guid(direction, &needle.guid).or_else(|| {
            if fuzzy_match_names {
                self.device_by_name_fuzzy(direction, &needle.human_name)
            } else {
                self.device_by_name(direction, &needle.human_name)
            }
        })
    }
    /// Given an input of desired devices from an active profile,
    /// search our lists of known connected and active devices,
    /// and "overlay" the devices we were able to find on top
    /// of the given action set.
    pub fn overlay_available_devices(
        &self,
        actions: &mut DeviceSet<Discovered>,
        desired: &DeviceSet<ConfigEntry>,
        fuzzy_match_names: bool,
    ) {
        use wasapi::Direction::*;
        let update_device =
            |direction: &Direction, actions: &mut DiscoveredDevice, desired: &ConfigDevice| {
                if let Some(device) = self.try_find_device(direction, desired, fuzzy_match_names) {
                    *actions = device.clone();
                }
            };

        update_device(&Render, &mut actions.playback, &desired.playback);
        if self.unify_communications_devices {
            actions.playback_comms = actions.playback.clone();
        } else {
            update_device(
                &Render,
                &mut actions.playback_comms,
                &desired.playback_comms,
            );
        }

        update_device(&Capture, &mut actions.recording, &desired.recording);
        if self.unify_communications_devices {
            actions.recording_comms = actions.recording.clone();
        } else {
            update_device(
                &Capture,
                &mut actions.recording_comms,
                &desired.recording_comms,
            );
        }
    }
    /// Used after laying all active profiles on top of one another to remove any redundant actions.
    pub fn discard_healthy(&self, left: &mut DeviceSet<Discovered>, right: &DeviceSet<Discovered>) {
        let clear_if_matching = |l: &mut DiscoveredDevice, r: &DiscoveredDevice| {
            if l.guid == r.guid {
                l.clear();
            }
        };
        clear_if_matching(&mut left.playback, &right.playback);
        clear_if_matching(&mut left.playback_comms, &right.playback_comms);
        clear_if_matching(&mut left.recording, &right.recording);
        clear_if_matching(&mut left.recording_comms, &right.recording_comms);
    }
    pub fn change_devices(&self, new_devices: DeviceSet<Discovered>) -> AppResult<()> {
        use Role::*;
        let roles = [
            (new_devices.playback, vec![Console, Multimedia]),
            (new_devices.playback_comms, vec![Communications]),
            (new_devices.recording, vec![Console, Multimedia]),
            (new_devices.recording_comms, vec![Communications]),
        ];

        for (device, roles) in roles.iter() {
            if !device.guid.is_empty() {
                info!("Setting {} -> {roles:?}", device.human_name);
                for role in roles {
                    self.set_device_role(&device.guid, role)?;
                }
            }
        }

        Ok(())
    }
    /// Update the Platform handler with the given config
    pub fn update_config(&mut self, config: &PlatformSettings) {
        self.unify_communications_devices = config.unify_communications_devices;

        if config.shadowplay_support {
            self.shadowplay = match ShadowPlayHandle::build() {
                Ok(handle) => {
                    if let Ok(recording) = self.get_role_default(&DeviceRole::Recording) {
                        handle.microphone_change(&recording.guid);
                        Some(handle)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    error!("{e}");
                    None
                }
            };
        } else {
            self.shadowplay = None;
        }
    }
    // Could probably replace this with some generic iterator over the enum variants for `DeviceRole`...
    pub fn copy_all_roles(
        &self,
        dest: &mut DeviceSet<ConfigEntry>,
        source: &DeviceSet<Discovered>,
        save_fuzzy_name: bool,
        save_guid: bool,
    ) {
        use DeviceRole::*;
        let roles = [Playback, PlaybackComms, Recording, RecordingComms];

        for role in roles {
            let real_device = source.get_role(&role);
            let config_device =
                self.device_to_config_entry(real_device, save_fuzzy_name, save_guid);
            dest.update_role(&role, config_device);
        }
    }
    pub fn update_config_entry(
        &self,
        entry: &mut DeviceSet<ConfigEntry>,
        role: &DeviceRole,
        guid: &str,
        save_fuzzy_name: bool,
        save_guid: bool,
    ) -> AppResult<()> {
        let real_device = self
            .device_by_guid(&role.into(), guid)
            .ok_or_else(|| RedefaulterError::DeviceNotFound(guid.to_string()))?;

        let new_device = self.device_to_config_entry(real_device, save_fuzzy_name, save_guid);
        entry.update_role(role, new_device);

        Ok(())
    }
    // I would prefer this to be a method of the struct,
    // but I don't want to rebuild the regex every invocation.
    // I could possibly make it a static var, but eh.
    pub fn device_to_config_entry(
        &self,
        discovered: &WindowsAudioDevice<Discovered>,
        save_fuzzy_name: bool,
        save_guid: bool,
    ) -> WindowsAudioDevice<ConfigEntry> {
        let human_name = if save_fuzzy_name {
            self.regex_windows_numeric_prefix
                .replace(&discovered.human_name, " (")
                .to_string()
        } else {
            discovered.human_name.to_owned()
        };

        let guid = if save_guid {
            discovered.guid.to_owned()
        } else {
            String::new()
        };

        WindowsAudioDevice::new(human_name, guid)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, MenuToggle, MenuId, TrayChecks)]
pub struct PlatformSettings {
    /// Unify Communications Devices
    ///
    /// When true, all communications entries are ignored. Any higher priority profile entries that change only communications device will be ignored.
    #[menuid(rename = "unify")]
    #[serde(default)]
    pub unify_communications_devices: bool,
    /// ShadowPlay Support (Experimental)
    ///
    /// When true, will also attempt to update the chosen recording device for NVIDIA's ShadowPlay feature
    #[menuid(rename = "shadow")]
    #[serde(default)]
    pub shadowplay_support: bool,
    #[menuid(skip)]
    #[serde(default)]
    #[serde(rename = "default")]
    pub default_devices: DeviceSet<ConfigEntry>,
}

// Yoinked from https://gist.github.com/dgellow/fb85229ee8aeabf3844a5f3d38eb445d

// TODO Maybe replace with OsStrExt,
// since I think encode_wide returns a compatible reference,
// to help avoid allocating each time we want to talk to Windows
#[derive(Default)]
pub struct WideString(pub Vec<u16>);

pub trait ToWide {
    fn to_wide(&self) -> WideString;
}

impl ToWide for &str {
    fn to_wide(&self) -> WideString {
        let mut result: Vec<u16> = self.encode_utf16().collect();
        result.push(0);
        WideString(result)
    }
}

impl ToWide for String {
    fn to_wide(&self) -> WideString {
        let mut result: Vec<u16> = self.encode_utf16().collect();
        result.push(0);
        WideString(result)
    }
}

impl WideString {
    pub fn as_pwstr(&self) -> PWSTR {
        PWSTR(self.0.as_ptr().cast_mut())
    }
}
