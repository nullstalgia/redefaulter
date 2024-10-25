use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

use color_eyre::eyre::Result;
use serde::{Deserialize, Deserializer, Serialize};
use takeable::Takeable;
use wasapi::*;
use windows::{
    core::PWSTR,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::*,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            STGM_READ,
        },
    },
};

use crate::{
    args::ListSubcommand,
    errors::{AppResult, RedefaulterError},
    profiles::AppOverride,
};

use device_notifications::{NotificationCallbacks, WindowsAudioNotification};
use policy_config::{IPolicyConfig, PolicyConfig};

use super::AudioDevice;

mod device_notifications;
mod device_ser;
mod policy_config;

pub struct AudioNightmare {
    /// Interface to query endpoints through
    device_enumerator: Takeable<IMMDeviceEnumerator>,
    /// Interface to change endpoints through
    policy_config: Takeable<IPolicyConfig>,
    ///
    device_callbacks: Takeable<NotificationCallbacks>,
    /// Notifications from Windows about updates to audio endpoints
    callback_rx: Receiver<WindowsAudioNotification>,
    /// Existing devices attached to the host
    playback_devices: BTreeMap<String, WindowsAudioDevice>,
    /// Existing devices attached to the host
    recording_devices: BTreeMap<String, WindowsAudioDevice>,
}
impl Drop for AudioNightmare {
    fn drop(&mut self) {
        // These need to get dropped first, otherwise the Uninit call will run while they're still in memory
        // and cause an ACCESS_VIOLATION when it tries
        self.policy_config.take();
        let device_enumerator = self.device_enumerator.take();
        let callbacks = self.device_callbacks.take();
        let _ = callbacks.unregister_to_enumerator(&device_enumerator);
        // https://github.com/microsoft/windows-rs/issues/1169#issuecomment-926877227
        // unsafe {
        //     CoUninitialize();
        // }
    }
}
impl AudioNightmare {
    pub fn build() -> AppResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }

        let policy_config: IPolicyConfig =
            unsafe { CoCreateInstance(&PolicyConfig, None, CLSCTX_ALL) }?;
        let device_enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;

        let (tx, rx) = mpsc::channel();

        let device_callbacks = NotificationCallbacks::new(&tx);

        let mut playback_devices = BTreeMap::new();
        let mut recording_devices = BTreeMap::new();

        let initial_playback = DeviceCollection::new(&Direction::Render)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?;

        for device in &initial_playback {
            let device = device.expect("Couldn't get device");
            let human_name = device
                .get_friendlyname()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?;
            let guid = device
                .get_id()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?;
            let listing = WindowsAudioDevice {
                human_name,
                guid: guid.clone(),
            };
            playback_devices.insert(guid, listing);
        }

        // println!("{playback_devices:#?}");

        let initial_recording = DeviceCollection::new(&Direction::Capture)
            .map_err(|_| RedefaulterError::FailedToGetInfo)?;

        for device in &initial_recording {
            let device = device.expect("Couldn't get device");
            let human_name = device
                .get_friendlyname()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?;
            // let device_name = device.get_interface_friendlyname()
            let guid = device
                .get_id()
                .map_err(|_| RedefaulterError::FailedToGetInfo)?;
            let listing = WindowsAudioDevice {
                human_name,
                guid: guid.clone(),
            };
            recording_devices.insert(guid, listing);
        }

        // println!("{recording_devices:#?}");

        device_callbacks.register_to_enumerator(&device_enumerator)?;

        Ok(Self {
            policy_config: Takeable::new(policy_config),
            device_enumerator: Takeable::new(device_enumerator),
            device_callbacks: Takeable::new(device_callbacks),
            callback_rx: rx,
            playback_devices,
            recording_devices,
        })
    }
    pub fn print_one_audio_event(&mut self) -> Result<()> {
        let notif = self.callback_rx.recv()?;
        println!("Notification: {:?}", notif);
        Ok(())
    }
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
    pub fn set_device_role(&mut self, device_id: &str, role: &Role) -> AppResult<()> {
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
    fn add_endpoint(&mut self, id: &str) {
        todo!()
    }
    fn remove_endpoint(&mut self, id: &str) {
        if self.playback_devices.remove(id).is_none() {
            self.recording_devices.remove(id);
        }
        todo!()
    }
    fn handle_endpoint_notification(&mut self, notif: WindowsAudioNotification) {
        use WindowsAudioNotification::*;
        match notif {
            DeviceAdded { id } => self.add_endpoint(&id),
            DeviceRemoved { id } => self.remove_endpoint(&id),
            DeviceStateChanged { id, state } => match state.0 {
                // https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-state-xxx-constants
                // ACTIVE
                0x1 => self.add_endpoint(&id),
                // DISABLED | NOTPRESENT | UNPLUGGED
                0x2 | 0x4 | 0x8 => self.remove_endpoint(&id),
                _ => panic!("Got unexpected state from DeviceStateChanged!"),
            },
            DefaultDeviceChanged { id, flow, role } => todo!(),
            PropertyValueChanged => unimplemented!(),
            VolumeChanged => unimplemented!(),
        }
    }
    /// Gets device by name, ignoring any numeric prefix added by Windows
    fn device_by_name_fuzzy(
        &self,
        direction: &Direction,
        name: &str,
    ) -> Option<&WindowsAudioDevice> {
        todo!()
    }
    fn device_by_guid(&self, direction: &Direction, guid: &str) -> Option<&WindowsAudioDevice> {
        match direction {
            Direction::Render => self.playback_devices.get(guid),
            Direction::Capture => self.recording_devices.get(guid),
        }
    }
}

// Maybe I need to have one for a detected device vs a desired device
// A desired device won't always be connected to the machine.
#[derive(Debug, Clone, Default)]
pub struct WindowsAudioDevice {
    // #[serde(skip)]
    // device_type: Direction,
    human_name: String,
    guid: String,
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
        // and JSON lets me output just the string as is.
        serde_json::to_string(self).expect("Failed to serialize profile")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

struct Config {
    unify_communications_devices: bool,
    desired_set: DeviceSet,
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
