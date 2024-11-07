use std::collections::BTreeMap;

use muda::{PredefinedMenuItem, Submenu, SubmenuBuilder};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, MenuEvent, MenuItem};
use wasapi::Direction;

use crate::{
    app::App,
    errors::{AppResult, RedefaulterError},
    platform::{
        AudioNightmare, ConfigDevice, ConfigEntry, DeviceRole, DeviceSet, Discovered,
        DiscoveredDevice, PlatformSettings,
    },
    tray_menu::{build_device_checks, DeviceSelectionType, TrayDevice},
};

use super::common_ids::*;

impl App {
    pub fn tray_platform_device_selection(
        &self,
        // profile_name_str: &str,
        destination: &DeviceSelectionType,
        device_set: &DeviceSet<ConfigEntry>,
    ) -> AppResult<Vec<Box<dyn IsMenuItem>>> {
        let mut submenus: Vec<Box<dyn IsMenuItem>> = Vec::new();

        use DeviceRole::*;
        use DeviceSelectionType::*;

        submenus.push(Box::new(self.tray_build_platform_device_selection(
            destination,
            &Playback,
            &device_set.playback,
        )?));

        if !self.settings.platform.unify_communications_devices {
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

        if !self.settings.platform.unify_communications_devices {
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

        let possibly_known_device = self.endpoints.try_find_device(&direction, current);

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
