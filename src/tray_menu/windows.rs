use muda::{MenuItem, Submenu, SubmenuBuilder};
use tray_icon::menu::IsMenuItem;
use wasapi::Direction;

use crate::{
    app::App,
    errors::AppResult,
    platform::{ConfigDevice, ConfigEntry, DeviceRole, DeviceSet},
    tray_menu::{build_device_checks, label_item, DeviceSelectionType},
};

impl App {
    // Helpful for diagnostics but looks kinda ugly due to how long the device names are,
    // so I'm just gonna keep it off by default.
    pub fn tray_platform_active_devices(&self) -> AppResult<Vec<Box<dyn IsMenuItem>>> {
        let mut devices: Vec<Box<dyn IsMenuItem>> = Vec::new();
        use DeviceRole::*;

        let header = label_item("Active Devices:");
        devices.push(Box::new(header));

        let build_device = |role: &DeviceRole| -> MenuItem {
            let device = self.current_defaults.get_role(role);
            let human_name = &device.human_name;
            // let human_name = if self.settings.behavior.always_save_generics {
            //     let config_device = self.endpoints.device_to_config_entry(device, true);
            //     config_device.human_name.clone()
            // } else {
            //     device.human_name.clone()
            // };
            let text = format!("{role}: {human_name}");
            label_item(text)
        };

        devices.push(Box::new(build_device(&Playback)));

        if !self.settings.devices.platform.unify_communications_devices {
            devices.push(Box::new(build_device(&PlaybackComms)));
        }

        devices.push(Box::new(build_device(&Recording)));

        if !self.settings.devices.platform.unify_communications_devices {
            devices.push(Box::new(build_device(&RecordingComms)));
        }

        Ok(devices)
    }
    pub fn tray_platform_device_selection(
        &self,
        destination: &DeviceSelectionType,
        device_set: &DeviceSet<ConfigEntry>,
    ) -> AppResult<Vec<Box<dyn IsMenuItem>>> {
        let mut submenus: Vec<Box<dyn IsMenuItem>> = Vec::new();

        use DeviceRole::*;

        submenus.push(Box::new(self.tray_build_platform_device_selection(
            destination,
            &Playback,
            &device_set.playback,
        )?));

        if !self.settings.devices.platform.unify_communications_devices {
            submenus.push(Box::new(self.tray_build_platform_device_selection(
                destination,
                &PlaybackComms,
                &device_set.playback_comms,
            )?));
        }

        submenus.push(Box::new(self.tray_build_platform_device_selection(
            destination,
            &Recording,
            &device_set.recording,
        )?));

        if !self.settings.devices.platform.unify_communications_devices {
            submenus.push(Box::new(self.tray_build_platform_device_selection(
                destination,
                &RecordingComms,
                &device_set.recording_comms,
            )?));
        }

        Ok(submenus)
    }
    pub fn tray_build_platform_device_selection(
        &self,
        destination: &DeviceSelectionType,
        role: &DeviceRole,
        current: &ConfigDevice,
    ) -> AppResult<Submenu> {
        use wasapi::Direction::*;

        let direction: Direction = role.into();

        let all_devices = match direction {
            Render => &self.endpoints.playback_devices,
            Capture => &self.endpoints.recording_devices,
        };

        let possibly_known_device = self.endpoints.try_find_device(
            &direction,
            current,
            self.settings.devices.fuzzy_match_names,
        );

        let playback_device_checks = build_device_checks(
            all_devices,
            destination,
            role,
            current,
            possibly_known_device,
        );
        let item_refs = playback_device_checks
            .iter()
            .map(|item| item.as_ref())
            .collect::<Vec<_>>();

        let text = match destination {
            DeviceSelectionType::ConfigDefault => format!("Preferred Default {role}"),
            DeviceSelectionType::Profile(_) => format!("Override Default {role}"),
        };

        let submenu = SubmenuBuilder::new()
            .items(&item_refs)
            .text(text)
            .enabled(true)
            .build()?;

        Ok(submenu)
    }
}
