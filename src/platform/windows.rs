use std::{path::PathBuf, time::Instant};

use color_eyre::eyre::Result;
use windows::{
    core::PWSTR,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Media::Audio::{
            eCapture, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            STGM_READ,
        },
    },
};
pub struct AudioNightmare {}
impl AudioNightmare {
    pub fn init(&self) -> Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(())
    }
    pub fn deinit(&self) -> Result<()> {
        unsafe {
            CoUninitialize();
        }
        Ok(())
    }
    pub fn event_loop(&mut self) -> Result<()> {
        Ok(())
    }
    pub fn print_devices(&self) -> Result<()> {
        let audio_endpoint_enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
        let devices =
            unsafe { audio_endpoint_enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }?;
        let count = unsafe { devices.GetCount() }? as usize;
        for i in 0..count {
            let device = unsafe { devices.Item(i as u32)? };
            println!("{device:#?}");

            let device_id = unsafe { device.GetId() }?;
            let mut device_name_buffer = unsafe {
                device
                    .OpenPropertyStore(STGM_READ)?
                    .GetValue(&PKEY_Device_FriendlyName)?
            }
            .to_string()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();
            let device_name = PWSTR(device_name_buffer.as_mut_ptr());
            println!("{}", unsafe { device_name.display() });
            // let device_is_default = match default_endpoint_id {
            //     Some(id) => pwstr_eq(device_id, id),
            //     _ => false,
            // };
            // let mut menu_audio_endpoints: Vec<PWSTR> = Vec::new();
            // let mut found = false;
            // for j in i..menu_audio_endpoints.len() {
            //     // if pwstr_eq(device_id, menu_audio_endpoints[i]) {
            //     //     found = true;
            //     //     for _ in 0..(j - i) {
            //     //         unsafe {
            //     //             CoTaskMemFree(Some(menu_audio_endpoints.remove(i).0 as *const c_void));
            //     //             RemoveMenu(menu, i as u32, MF_BYPOSITION)?;
            //     //         }
            //     //     }
            //     //     unsafe {
            //     //         SetMenuItemInfoW(
            //     //             menu,
            //     //             i as u32,
            //     //             true,
            //     //             &MENUITEMINFOW {
            //     //                 cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            //     //                 fMask: MIIM_ID | MIIM_STATE | MIIM_STRING,
            //     //                 fState: if device_is_default {
            //     //                     MFS_CHECKED
            //     //                 } else {
            //     //                     MFS_UNCHECKED
            //     //                 },
            //     //                 wID: i as u32,
            //     //                 dwTypeData: device_name,
            //     //                 ..Default::default()
            //     //             },
            //     //         )?;
            //     //     }
            //     //     break;
            //     // }
            // }
        }
        Ok(())
    }
}

enum DeviceType {
    Playback,
    Recording,
}

// Maybe I need to have one for a detected device vs a desired device
// A desired device won't always be connected to the machine.
struct WindowsAudioDevice {
    device_type: DeviceType,
    human_name: String,
    guid: String,
}

struct DeviceSet {
    playback: WindowsAudioDevice,
    playback_comms: WindowsAudioDevice,
    recording: WindowsAudioDevice,
    recording_comms: WindowsAudioDevice,
}

struct Config {
    unify_communications_devices: bool,
    desired_set: DeviceSet,
}

struct AppOverride {
    priority: usize,
    process_path: PathBuf,
    override_set: DeviceSet,
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
