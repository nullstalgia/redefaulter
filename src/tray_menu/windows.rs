use std::collections::BTreeMap;

use muda::{PredefinedMenuItem, Submenu, SubmenuBuilder};
use tray_icon::menu::{CheckMenuItem, IsMenuItem, MenuEvent, MenuItem};

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
            &DeviceSelectionType::ConfigDefault,
            &DeviceRole::Playback,
            &self.settings.platform.default_devices.playback,
            possibly_known_device,
        );
        let item_refs = playback_device_checks
            .iter()
            .map(|item| item.as_ref())
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
                &DeviceSelectionType::ConfigDefault,
                &DeviceRole::PlaybackComms,
                &self.settings.platform.default_devices.playback_comms,
                possibly_known_device,
            );
            let item_refs = playback_device_checks
                .iter()
                .map(|item| item.as_ref())
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
            &DeviceSelectionType::ConfigDefault,
            &DeviceRole::Recording,
            &self.settings.platform.default_devices.recording,
            possibly_known_device,
        );
        let item_refs = recording_device_checks
            .iter()
            .map(|item| item.as_ref())
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
                &DeviceSelectionType::ConfigDefault,
                &DeviceRole::RecordingComms,
                &self.settings.platform.default_devices.recording_comms,
                possibly_known_device,
            );
            let item_refs = recording_device_checks
                .iter()
                .map(|item| item.as_ref())
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
