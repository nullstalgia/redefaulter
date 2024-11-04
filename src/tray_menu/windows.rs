use std::collections::BTreeMap;

use muda::{Submenu, SubmenuBuilder};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, MenuEvent, MenuItem};

use crate::{
    app::App,
    errors::AppResult,
    platform::{AudioNightmare, DeviceSet, Discovered, DiscoveredDevice, PlatformSettings},
};

use super::{build_device_checks, common_ids::*, TrayHelper};

impl TrayHelper {
    pub fn platform_settings(&self, settings: &PlatformSettings) -> Vec<Box<dyn IsMenuItem>> {
        let mut items: Vec<Box<dyn IsMenuItem>> = Vec::new();

        let header = MenuItem::new("Settings:", false, None);

        items.push(Box::new(header));

        items
    }
    // this is cooked
    pub fn platform_config_device_selection(
        &self,
        endpoints: &AudioNightmare,
        current_defaults: &DeviceSet<Discovered>,
        settings: &PlatformSettings,
    ) -> AppResult<Vec<Submenu>> {
        let mut submenus = Vec::new();

        // TODO options input should be determined by if profile or config
        let playback_id = format!("{CONFIG_DEFAULT_ID}-p");
        let playback_device_checks = build_device_checks(
            &endpoints.playback_devices,
            CONFIG_DEFAULT_ID,
            Some(&current_defaults.playback.guid),
        );
        let item_refs = playback_device_checks
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect::<Vec<_>>();

        let playback_menu = SubmenuBuilder::new()
            .items(&item_refs)
            .text("Redefault Playback")
            .enabled(true)
            .build()?;

        submenus.push(playback_menu);

        if !settings.unify_communications_devices {
            let playback_device_checks = build_device_checks(
                &endpoints.playback_devices,
                CONFIG_DEFAULT_ID,
                Some(&current_defaults.playback_comms.guid),
            );
            let item_refs = playback_device_checks
                .iter()
                .map(|item| item as &dyn IsMenuItem)
                .collect::<Vec<_>>();

            let playback_menu = SubmenuBuilder::new()
                .items(&item_refs)
                .text("Redefault Playback (Comm.)")
                .enabled(true)
                .build()?;

            submenus.push(playback_menu);
        }

        let recording_device_checks = build_device_checks(
            &endpoints.recording_devices,
            CONFIG_DEFAULT_ID,
            Some(&current_defaults.recording.guid),
        );
        let item_refs = recording_device_checks
            .iter()
            .map(|item| item as &dyn IsMenuItem)
            .collect::<Vec<_>>();

        let recording_menu = SubmenuBuilder::new()
            .items(&item_refs)
            .text("Redefault Recording")
            .enabled(true)
            .build()?;

        submenus.push(recording_menu);

        if !settings.unify_communications_devices {
            let recording_device_checks = build_device_checks(
                &endpoints.recording_devices,
                CONFIG_DEFAULT_ID,
                Some(&current_defaults.recording_comms.guid),
            );
            let item_refs = recording_device_checks
                .iter()
                .map(|item| item as &dyn IsMenuItem)
                .collect::<Vec<_>>();

            let recording_menu = SubmenuBuilder::new()
                .items(&item_refs)
                .text("Redefault Recording (Comm.)")
                .enabled(true)
                .build()?;

            submenus.push(recording_menu);
        }

        Ok(submenus)
    }
}

impl App {}
