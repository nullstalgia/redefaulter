use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

use color_eyre::eyre::Result;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use takeable::Takeable;
use tao::event_loop::EventLoopProxy;
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
    profiles::AppOverride,
};

use device_notifications::{NotificationCallbacks, WindowsAudioNotification};
use policy_config::{IPolicyConfig, PolicyConfig};

use super::AudioDevice;

pub mod device_notifications;
mod device_ser;
mod policy_config;

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
    playback_devices: BTreeMap<String, WindowsAudioDevice>,
    /// Existing devices attached to the host
    recording_devices: BTreeMap<String, WindowsAudioDevice>,
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
    pub fn build(event_proxy: Option<EventLoopProxy<CustomEvent>>) -> AppResult<Self> {
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

        let initial_playback = DeviceCollection::new(&Direction::Render)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?;

        for device in &initial_playback {
            let device: WindowsAudioDevice = device.expect("Couldn't get device").try_into()?;
            playback_devices.insert(device.guid.clone(), device);
        }

        // println!("{playback_devices:#?}");

        let initial_recording = DeviceCollection::new(&Direction::Capture)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?;

        for device in &initial_recording {
            let device: WindowsAudioDevice = device.expect("Couldn't get device").try_into()?;
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

        Ok(Self {
            policy_config: Takeable::new(policy_config),
            device_enumerator: Takeable::new(device_enumerator),
            device_callbacks,
            // callback_rx: rx,
            playback_devices,
            recording_devices,
            regex,
            unify_communications_devices: true,
            event_proxy,
        })
    }
    // pub fn print_one_audio_event(&mut self) -> Result<()> {
    //     let notif = self.callback_rx.recv()?;
    //     println!("Notification: {:?}", notif);
    //     Ok(())
    // }
    pub fn event_loop(&mut self) -> Result<()> {
        Ok(())
    }
    pub fn set_device_test(&mut self) -> AppResult<()> {
        let id = "{0.0.0.00000000}.{1e9628d3-7e6c-4979-80f0-46122c6a8ab6}";
        let id = id.to_wide();
        for role in [eConsole, eMultimedia, eCommunications] {
            unsafe { self.policy_config.SetDefaultEndpoint(id.as_pwstr(), role) }?;
        }
        Ok(())
    }
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
        let direction: Direction = unsafe { endpoint.GetDataFlow()? }
            .try_into()
            .expect("Invalid Enum?");
        println!("{direction:?}");
        let device: Device = Device::custom(device, direction);

        if !known_to_be_active {
            let state = device
                .get_state()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?;

            use DeviceState::*;
            match state {
                Active => (),
                Disabled | NotPresent | Unplugged => return Ok(()),
            }
        }

        let device: WindowsAudioDevice = device.try_into()?;

        match direction {
            Direction::Capture => {
                if let Some(old) = self.playback_devices.insert(device.guid.clone(), device) {
                    println!("Playback device already existed? {old:?}");
                };
            }
            Direction::Render => {
                if let Some(old) = self.recording_devices.insert(device.guid.clone(), device) {
                    println!("Recording device already existed? {old:?}");
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
    ) -> Option<&'a WindowsAudioDevice> {
        if name.is_empty() {
            return None;
        }
        let find =
            |map: &'a BTreeMap<String, WindowsAudioDevice>| -> Option<&'a WindowsAudioDevice> {
                for (_, device) in map {
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
    fn device_by_guid(&self, direction: &Direction, guid: &str) -> Option<&WindowsAudioDevice> {
        match direction {
            Direction::Render => self.playback_devices.get(guid),
            Direction::Capture => self.recording_devices.get(guid),
        }
    }
    pub fn get_current_defaults(&self) -> AppResult<DeviceSet> {
        use wasapi::Direction::*;
        use wasapi::Role::*;
        let playback: WindowsAudioDevice = get_default_device_for_role(&Render, &Console)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?
            .try_into()?;
        let playback_comms: WindowsAudioDevice =
            get_default_device_for_role(&Render, &Communications)
                .map_err(|_| RedefaulterError::FailedToGetInfo)?
                .try_into()?;
        let recording: WindowsAudioDevice = get_default_device_for_role(&Capture, &Console)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?
            .try_into()?;
        let recording_comms: WindowsAudioDevice =
            get_default_device_for_role(&Capture, &Communications)
                .map_err(|_| RedefaulterError::FailedToGetInfo)?
                .try_into()?;

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
        needle: &WindowsAudioDevice,
    ) -> Option<&WindowsAudioDevice> {
        self.device_by_guid(direction, &needle.guid)
            .or_else(|| self.device_by_name_fuzzy(direction, &needle.human_name))
    }
    pub fn overlay_available_devices(&self, left: &mut DeviceSet, right: &DeviceSet) {
        use wasapi::Direction::*;
        let update_device = |left: &mut WindowsAudioDevice, right: &WindowsAudioDevice| {
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

        update_device(&mut left.recording, &right.recording);
        if self.unify_communications_devices {
            left.recording_comms = left.recording.clone();
        } else {
            update_device(&mut left.recording_comms, &right.recording_comms);
        }
    }
    pub fn discard_healthy(&self, left: &mut DeviceSet, right: &DeviceSet) {
        let clear_if_matching = |l: &mut WindowsAudioDevice, r: &WindowsAudioDevice| {
            if l == r {
                l.clear();
            }
        };
        clear_if_matching(&mut left.playback, &right.playback);
        clear_if_matching(&mut left.playback_comms, &right.playback_comms);
        clear_if_matching(&mut left.recording, &right.recording);
        clear_if_matching(&mut left.recording_comms, &right.recording_comms);
    }
    // TODO take advantage of type system to make sure I can't put in a raw (from config)
    // DeviceSet, i only want to be able to supply a real known device from the platform impl's enumerations
    pub fn change_devices(&self, new_devices: DeviceSet) -> AppResult<()> {
        println!("change_devices: {new_devices:?}");
        use Role::*;
        let roles = [
            (new_devices.playback.guid, vec![Console, Multimedia]),
            (new_devices.playback_comms.guid, vec![Communications]),
            (new_devices.recording.guid, vec![Console, Multimedia]),
            (new_devices.recording_comms.guid, vec![Communications]),
        ];

        for (guid, roles) in roles.iter() {
            if !guid.is_empty() {
                for role in roles {
                    self.set_device_role(guid, role)?;
                }
            }
        }

        Ok(())
    }
}

// Maybe I need to have one for a detected device vs a desired device
// A desired device won't always be connected to the machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsAudioDevice {
    // #[serde(skip)]
    // device_type: Direction,
    human_name: String,
    guid: String,
}

impl WindowsAudioDevice {
    pub fn clear(&mut self) {
        self.human_name.clear();
        self.guid.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.human_name.is_empty() && self.guid.is_empty()
    }
}

impl AudioDevice for WindowsAudioDevice {
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

impl TryFrom<wasapi::Device> for WindowsAudioDevice {
    type Error = RedefaulterError;
    fn try_from(value: wasapi::Device) -> AppResult<Self> {
        Ok(WindowsAudioDevice {
            human_name: value
                .get_friendlyname()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?,
            guid: value
                .get_id()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeviceSet {
    // #[serde(with = "serde_windows_audio_device")]
    #[serde(default)]
    playback: WindowsAudioDevice,
    #[serde(default)]
    playback_comms: WindowsAudioDevice,
    #[serde(default)]
    recording: WindowsAudioDevice,
    #[serde(default)]
    recording_comms: WindowsAudioDevice,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub unify_communications_devices: bool,
    #[serde(rename = "default")]
    pub default_devices: DeviceSet,
}

struct AppContext {
    config: Config,
    overrides: Vec<AppOverride>,
    desired_set: DeviceSet,
    current_set: DeviceSet,
    // To prevent fighting with something else messing with devices
    changes_within_few_seconds: usize,
    last_change: Instant,
}

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
