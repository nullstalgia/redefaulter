use std::collections::BTreeMap;

use muda::{Submenu, SubmenuBuilder};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, MenuEvent, MenuItem};

use crate::{
    app::App,
    errors::AppResult,
    platform::{
        AudioNightmare, ConfigDevice, ConfigEntry, DeviceSet, Discovered, DiscoveredDevice,
        PlatformSettings,
    },
};

use super::{common_ids::*, DeviceSelectionType};

impl App {
    // this is cooked
    pub fn tray_platform_config_device_selection(
        &self,
        // device_set: DeviceSet<ConfigEntry>
    ) -> AppResult<Vec<Submenu>> {
        let mut submenus = Vec::new();

        // TODO options input should be determined by if profile or config
        let playback_id = format!("{CONFIG_DEFAULT_ID}-p");

        use wasapi::Direction::*;

        let possibly_known_device = self
            .endpoints
            .try_find_device(&Render, &self.settings.platform.default_devices.playback);

        let playback_device_checks = build_device_checks(
            &self.endpoints.playback_devices,
            CONFIG_DEFAULT_ID,
            &self.settings.platform.default_devices.playback,
            possibly_known_device,
            &DeviceSelectionType::ConfigDefault,
        );
        let item_refs = playback_device_checks
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect::<Vec<_>>();

        let playback_menu = SubmenuBuilder::new()
            .items(&item_refs)
            .text("Preferred Default Playback")
            .enabled(true)
            .build()?;

        submenus.push(playback_menu);

        if !self.settings.platform.unify_communications_devices {
            let possibly_known_device = self.endpoints.try_find_device(
                &Render,
                &self.settings.platform.default_devices.playback_comms,
            );
            let playback_device_checks = build_device_checks(
                &self.endpoints.playback_devices,
                CONFIG_DEFAULT_ID,
                &self.settings.platform.default_devices.playback_comms,
                possibly_known_device,
                &DeviceSelectionType::ConfigDefault,
            );
            let item_refs = playback_device_checks
                .iter()
                .map(|item| item as &dyn IsMenuItem)
                .collect::<Vec<_>>();

            let playback_menu = SubmenuBuilder::new()
                .items(&item_refs)
                .text("Preferred Default Playback (Comm.)")
                .enabled(true)
                .build()?;

            submenus.push(playback_menu);
        }

        let possibly_known_device = self
            .endpoints
            .try_find_device(&Capture, &self.settings.platform.default_devices.recording);

        let recording_device_checks = build_device_checks(
            &self.endpoints.recording_devices,
            CONFIG_DEFAULT_ID,
            &self.settings.platform.default_devices.recording,
            possibly_known_device,
            &DeviceSelectionType::ConfigDefault,
        );
        let item_refs = recording_device_checks
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect::<Vec<_>>();

        let recording_menu = SubmenuBuilder::new()
            .items(&item_refs)
            .text("Preferred Default Recording")
            .enabled(true)
            .build()?;

        submenus.push(recording_menu);

        if !self.settings.platform.unify_communications_devices {
            let possibly_known_device = self.endpoints.try_find_device(
                &Capture,
                &self.settings.platform.default_devices.recording_comms,
            );
            let recording_device_checks = build_device_checks(
                &self.endpoints.recording_devices,
                CONFIG_DEFAULT_ID,
                &self.settings.platform.default_devices.recording_comms,
                possibly_known_device,
                &DeviceSelectionType::ConfigDefault,
            );
            let item_refs = recording_device_checks
                .iter()
                .map(|item| item as &dyn IsMenuItem)
                .collect::<Vec<_>>();

            let recording_menu = SubmenuBuilder::new()
                .items(&item_refs)
                .text("Preferred Default Recording (Comm.)")
                .enabled(true)
                .build()?;

            submenus.push(recording_menu);
        }

        Ok(submenus)
    }
    pub fn tray_build_platform_device_selection(&self) {}
}

pub fn build_device_checks(
    devices: &BTreeMap<String, DiscoveredDevice>,
    prefix: &str,
    config_device: &ConfigDevice,
    discovered_device: Option<&DiscoveredDevice>,
    selection_type: &DeviceSelectionType,
) -> Vec<CheckMenuItem> {
    let mut items = Vec::new();

    use DeviceSelectionType::*;
    let none_text = match selection_type {
        ConfigDefault => "None",
        Profile => "No Override",
    };

    // Dunno if I want to keep it like this
    // or be prefix-none
    let none_id = format!("{prefix}");
    items.push(CheckMenuItem::with_id(
        &none_id,
        &none_text,
        true,
        config_device.is_empty(),
        None,
    ));

    for device in devices.values() {
        let item_id = format!("{prefix}-{}", device.guid);
        let chosen = if let Some(chosen) = discovered_device.as_ref() {
            *chosen.guid == device.guid
        } else {
            false
        };
        items.push(CheckMenuItem::with_id(
            &item_id,
            &device.human_name,
            true,
            chosen,
            None,
        ));
    }

    items
}
