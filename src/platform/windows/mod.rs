use std::{collections::BTreeMap, marker::PhantomData};

use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use takeable::Takeable;
use tao::event_loop::EventLoopProxy;
use tracing::{info, warn};
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
    app::CustomEvent,
    args::ListSubcommand,
    errors::{AppResult, RedefaulterError},
};

use device_notifications::{NotificationCallbacks, WindowsAudioNotification};
use policy_config::{IPolicyConfig, PolicyConfig};

use super::{AudioDevice, ConfigEntry, Discovered};

pub mod device_notifications;
mod device_ser;
mod policy_config;

pub type DiscoveredDevice = WindowsAudioDevice<Discovered>;
pub type ConfigDevice = WindowsAudioDevice<ConfigEntry>;

pub struct AudioNightmare {
    /// Interface to query endpoints through
    device_enumerator: Takeable<IMMDeviceEnumerator>,
    /// Interface to change endpoints through
    policy_config: Takeable<IPolicyConfig>,
    /// Client object for endpoint notifications from Windows
    device_callbacks: Option<NotificationCallbacks>,
    /// Channel for notifications for audio endpoint events
    // callback_rx: Receiver<WindowsAudioNotification>,
    /// Existing devices attached to the host
    playback_devices: BTreeMap<String, DiscoveredDevice>,
    /// Existing devices attached to the host
    recording_devices: BTreeMap<String, DiscoveredDevice>,
    /// Regex for fuzzy-matching devices with numeric prefixes
    regex: Regex,
    /// Used to tell `App` that something has changed
    event_proxy: Option<EventLoopProxy<CustomEvent>>,
    /// When `true`, *all* actions taken towards the Console/Multimedia Role
    /// will be applied to the Communications Role
    pub unify_communications_devices: bool,
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
        event_proxy: Option<EventLoopProxy<CustomEvent>>,
        config: Option<&PlatformConfig>,
    ) -> AppResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }

        let policy_config: IPolicyConfig =
            unsafe { CoCreateInstance(&PolicyConfig, None, CLSCTX_ALL) }?;
        let device_enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;

        // let (tx, rx) = mpsc::channel();

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

        // This regex matches an opening parenthesis '(', followed by one or more digits '\d+',
        // a dash '-', a space ' ', and captures the rest of the string '(.+?)' until the closing parenthesis.
        let regex = Regex::new(r"\(\d+- (.+?)\)").expect("Regex failed to build");

        let unify_communications_devices = if let Some(config) = config {
            config.unify_communications_devices
        } else {
            false
        };

        Ok(Self {
            policy_config: Takeable::new(policy_config),
            device_enumerator: Takeable::new(device_enumerator),
            device_callbacks,
            // callback_rx: rx,
            playback_devices,
            recording_devices,
            regex,
            unify_communications_devices,
            event_proxy,
        })
    }
    // pub fn print_one_audio_event(&mut self) -> Result<()> {
    //     let notif = self.callback_rx.recv()?;
    //     println!("Notification: {:?}", notif);
    //     Ok(())
    // }
    // pub fn event_loop(&mut self) -> Result<()> {
    //     Ok(())
    // }
    // pub fn set_device_test(&mut self) -> AppResult<()> {
    //     let id = "{0.0.0.00000000}.{1e9628d3-7e6c-4979-80f0-46122c6a8ab6}";
    //     let id = id.to_wide();
    //     for role in [eConsole, eMultimedia, eCommunications] {
    //         unsafe { self.policy_config.SetDefaultEndpoint(id.as_pwstr(), role) }?;
    //     }
    //     Ok(())
    // }
    pub fn set_device_role(&self, device_id: &str, role: &Role) -> AppResult<()> {
        let wide_id = device_id.to_wide();
        unsafe {
            self.policy_config
                .SetDefaultEndpoint(wide_id.as_pwstr(), role.to_owned().into())
        }?;
        Ok(())
    }
    pub fn print_devices(&self, categories: ListSubcommand) {
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
            for device in &self.playback_devices {
                println!("{}", device.1.profile_format());
            }
        }
        if recording {
            if playback {
                println!("----------");
            }
            println!("Recording devices: ");

            for device in &self.recording_devices {
                println!("{}", device.1.profile_format());
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
            Direction::Capture => {
                if let Some(old) = self.playback_devices.insert(device.guid.clone(), device) {
                    warn!("Playback device already existed? {old:?}");
                };
            }
            Direction::Render => {
                if let Some(old) = self.recording_devices.insert(device.guid.clone(), device) {
                    warn!("Recording device already existed? {old:?}");
                };
            }
        }

        Ok(())
    }
    fn remove_endpoint(&mut self, id: &str) -> AppResult<()> {
        if self.playback_devices.remove(id).is_none() {
            self.recording_devices.remove(id);
        }
        // todo!();
        Ok(())
    }
    pub fn handle_endpoint_notification(
        &mut self,
        notif: WindowsAudioNotification,
    ) -> AppResult<()> {
        use WindowsAudioNotification::*;
        match notif {
            DeviceAdded { id } => self.add_endpoint(&id, false)?,
            DeviceRemoved { id } => self.remove_endpoint(&id)?,
            DeviceStateChanged { id, state } => match state.0 {
                // https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-state-xxx-constants
                // ACTIVE
                0x1 => self.add_endpoint(&id, true)?,
                // DISABLED | NOTPRESENT | UNPLUGGED
                0x2 | 0x4 | 0x8 => self.remove_endpoint(&id)?,
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
    /// Gets device by name, ignoring any numeric prefix added by Windows
    pub fn device_by_name_fuzzy<'a>(
        &'a self,
        direction: &Direction,
        name: &str,
    ) -> Option<&'a DiscoveredDevice> {
        if name.is_empty() {
            return None;
        }
        let find = |map: &'a BTreeMap<String, DiscoveredDevice>| -> Option<&'a DiscoveredDevice> {
            for device in map.values() {
                let simplified_name = self.regex.replace(&device.human_name, "($1)");
                if name == device.human_name || name == simplified_name {
                    return Some(device);
                }
            }
            None
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

        Ok(DeviceSet {
            playback,
            playback_comms,
            recording,
            recording_comms,
        })
    }
    fn try_find_device(
        &self,
        direction: &Direction,
        needle: &ConfigDevice,
    ) -> Option<&DiscoveredDevice> {
        self.device_by_guid(direction, &needle.guid)
            .or_else(|| self.device_by_name_fuzzy(direction, &needle.human_name))
    }
    pub fn overlay_available_devices(
        &self,
        left: &mut DeviceSet<Discovered>,
        right: &DeviceSet<ConfigEntry>,
    ) {
        use wasapi::Direction::*;
        let update_device = |left: &mut DiscoveredDevice, right: &ConfigDevice| {
            if let Some(device) = self.try_find_device(&Render, right) {
                *left = device.clone();
            }
        };

        update_device(&mut left.playback, &right.playback);
        if self.unify_communications_devices {
            left.playback_comms = left.playback.clone();
        } else {
            update_device(&mut left.playback_comms, &right.playback_comms);
        }

        let update_device = |left: &mut DiscoveredDevice, right: &ConfigDevice| {
            if let Some(device) = self.try_find_device(&Capture, right) {
                *left = device.clone();
            }
        };

        update_device(&mut left.recording, &right.recording);
        if self.unify_communications_devices {
            left.recording_comms = left.recording.clone();
        } else {
            update_device(&mut left.recording_comms, &right.recording_comms);
        }
    }
    pub fn discard_healthy(&self, left: &mut DeviceSet<Discovered>, right: &DeviceSet<Discovered>) {
        let clear_if_matching = |l: &mut DiscoveredDevice, r: &DiscoveredDevice| {
            if l == r {
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
    pub fn change_config(&mut self, config: &PlatformConfig) {
        self.unify_communications_devices = config.unify_communications_devices;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsAudioDevice<State> {
    human_name: String,
    guid: String,
    // direction: Option<Direction>,
    _state: PhantomData<State>,
}

impl<State> WindowsAudioDevice<State> {
    pub fn clear(&mut self) {
        self.human_name.clear();
        self.guid.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.human_name.is_empty() && self.guid.is_empty()
    }
}

// impl WindowsAudioDevice<Discovered> {
//     pub fn direction(&self) -> Direction {
//         self.direction.unwrap()
//     }
// }

impl<State> AudioDevice for WindowsAudioDevice<State> {
    fn guid(&self) -> &str {
        self.guid.as_str()
    }
    fn human_name(&self) -> &str {
        self.human_name.as_str()
    }
    fn profile_format(&self) -> String {
        // So I can't use the toml serializer on the raw device since I think it expects a key/value,
        // but JSON lets me output just the string as is.
        serde_json::to_string(self).expect("Failed to serialize profile")
    }
}

impl TryFrom<wasapi::Device> for DiscoveredDevice {
    type Error = RedefaulterError;
    fn try_from(value: wasapi::Device) -> AppResult<Self> {
        Ok(DiscoveredDevice {
            human_name: value.get_friendlyname()?,
            guid: value.get_id()?,
            _state: PhantomData,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceSet<State> {
    #[serde(default)]
    playback: WindowsAudioDevice<State>,
    #[serde(default)]
    playback_comms: WindowsAudioDevice<State>,
    #[serde(default)]
    recording: WindowsAudioDevice<State>,
    #[serde(default)]
    recording_comms: WindowsAudioDevice<State>,
}

impl<State> DeviceSet<State> {
    pub fn is_empty(&self) -> bool {
        self.playback.is_empty()
            && self.playback_comms.is_empty()
            && self.recording.is_empty()
            && self.recording_comms.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub unify_communications_devices: bool,
    #[serde(rename = "default")]
    pub default_devices: DeviceSet<ConfigEntry>,
}

// struct AppContext {
//     config: Config,
//     overrides: Vec<AppOverride>,
//     desired_set: DeviceSet,
//     current_set: DeviceSet,
//     // To prevent fighting with something else messing with devices
//     changes_within_few_seconds: usize,
//     last_change: Instant,
// }

fn pwstr_eq(a: PWSTR, b: PWSTR) -> bool {
    let mut offset = 0;
    loop {
        let (chr_a, chr_b) = unsafe { (*a.0.add(offset), *b.0.add(offset)) };
        if chr_a != chr_b {
            return false;
        }
        if chr_a == 0 || chr_b == 0 {
            return true;
        }
        offset += 1;
    }
}

// Yoinked from https://gist.github.com/dgellow/fb85229ee8aeabf3844a5f3d38eb445d

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
        PWSTR(self.0.as_ptr() as *mut _)
    }
}
