use std::collections::BTreeMap;

use devices::WindowsAudioDevice;
use menu_macro::*;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use takeable::Takeable;
use tao::event_loop::EventLoopProxy;
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
    app::CustomEvent,
    args::ListSubcommand,
    errors::{AppResult, RedefaulterError},
};

use device_notifications::{NotificationCallbacks, WindowsAudioNotification};
use policy_config::{IPolicyConfig, PolicyConfig};

use super::{AudioDevice, ConfigEntry, Discovered};

pub mod device_notifications;
pub mod devices;
pub use devices::{ConfigDevice, DeviceRole, DeviceSet, DiscoveredDevice};

mod device_ser;
mod policy_config;

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
    /// Regex for fuzzy-matching devices with numeric prefixes
    regex_finding: Regex,
    /// Regex for removing numeric prefixes from devices to allow for fuzzy-matching later
    regex_replacing: Regex,
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
        config: Option<&PlatformSettings>,
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
        let regex_finding = Regex::new(r"\(\d+- (.+?)\)").expect("Regex failed to build");

        let regex_replacing = Regex::new(r"\((\d+)-\s*(.+?)\)").expect("Regex failed to build");

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
            regex_finding,
            regex_replacing,
            unify_communications_devices,
            event_proxy,
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
    fn remove_endpoint(&mut self, id: &str) -> AppResult<()> {
        if self.playback_devices.remove(id).is_none() {
            self.recording_devices.remove(id);
        }
        Ok(())
    }
    pub fn handle_endpoint_notification(
        &mut self,
        notif: WindowsAudioNotification,
    ) -> AppResult<()> {
        use WindowsAudioNotification::*;
        debug!("{notif:?}");
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
    /// Gets device by name,
    /// if no numeric prefix (e.g. `3- `) is supplied in the name,
    /// will return first device that matches regardless of prefix
    ///
    /// If one is supplied, will match for that name specifically
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
                let simplified_name = self.regex_finding.replace(&device.human_name, "($1)");
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
    ) -> Option<&DiscoveredDevice> {
        self.device_by_guid(direction, &needle.guid)
            .or_else(|| self.device_by_name_fuzzy(direction, &needle.human_name))
    }
    /// Given an input of desired devices from an active profile,
    /// search our lists of known connected and active devices,
    /// and "overlay" the devices we were able to find on top
    /// of the given action set.
    pub fn overlay_available_devices(
        &self,
        actions: &mut DeviceSet<Discovered>,
        desired: &DeviceSet<ConfigEntry>,
    ) {
        use wasapi::Direction::*;
        let update_device =
            |direction: &Direction, actions: &mut DiscoveredDevice, desired: &ConfigDevice| {
                if let Some(device) = self.try_find_device(direction, desired) {
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
    }
    pub fn update_config_entry(
        &self,
        entry: &mut DeviceSet<ConfigEntry>,
        role: &DeviceRole,
        guid: &str,
        make_fuzzy_name: bool,
    ) -> AppResult<()> {
        let real_device = self
            .device_by_guid(&role.into(), guid)
            .ok_or_else(|| RedefaulterError::DeviceNotFound(guid.to_string()))?;

        let new_device = self.device_to_config_entry(real_device, make_fuzzy_name);
        entry.update_role(role, new_device);

        Ok(())
    }
    // I would prefer this to be a method of the struct,
    // but I don't want to rebuild the regex every invocation.
    // I could possibly make it a static var, but eh.
    pub fn device_to_config_entry(
        &self,
        discovered: &WindowsAudioDevice<Discovered>,
        make_fuzzy_name: bool,
    ) -> WindowsAudioDevice<ConfigEntry> {
        let (human_name, guid) = {
            if make_fuzzy_name {
                let fuzzy_name = self
                    .regex_replacing
                    .replace_all(&discovered.human_name, "($2)");

                (fuzzy_name.to_string(), "".to_string())
            } else {
                (discovered.human_name.to_owned(), discovered.guid.to_owned())
            }
        };

        let config_device: WindowsAudioDevice<ConfigEntry> =
            WindowsAudioDevice::new(human_name, guid);

        config_device
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, MenuToggle, MenuId, TrayChecks)]
pub struct PlatformSettings {
    /// Unify Communications Devices
    ///
    /// When true, all communications entries are ignored. Any higher priority profile entries that change only communications device will be ignored.
    ///
    /// TODO: Make this work on its own when there's not a given set of devices?
    #[menuid(rename = "unify")]
    #[serde(default)]
    pub unify_communications_devices: bool,
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
        PWSTR(self.0.as_ptr() as *mut u16)
    }
}
