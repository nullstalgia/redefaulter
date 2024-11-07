use std::{borrow::BorrowMut, collections::BTreeMap, ffi::OsString};

use muda::{CheckMenuItem, IsMenuItem, Submenu};
use tao::event_loop::ControlFlow;
use tracing::*;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, SubmenuBuilder},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::{
    app::App,
    errors::AppResult,
    platform::{
        AudioNightmare, ConfigDevice, ConfigEntry, DeviceRole, DeviceSet, Discovered,
        DiscoveredDevice, PlatformSettings,
    },
    profiles::{AppOverride, PROFILES_PATH},
    tray_menu::TrayDevice,
};

pub mod common_ids {
    // Ids for root menu buttons, for all platforms
    pub const QUIT_ID: &str = "quit";
    pub const RELOAD_ID: &str = "reload";
    pub const REVEAL_ID: &str = "reveal";

    pub const DEVICE_PREFIX: &str = "device";

    pub const IGNORE_ID: &str = "ignore";
}

pub const TOOLTIP_PREFIX: &str = "Redefaulter";

use common_ids::*;

use super::DeviceSelectionType;

impl App {
    pub fn build_tray_late(&self) -> AppResult<TrayIcon> {
        let menu = Menu::new();

        let loading = MenuItem::new(format!("Loading profiles..."), false, None);

        menu.append(&loading)?;

        // drop(quit_i);

        // Add a copy to the struct if we start changing the icon?
        let initial_icon = Icon::from_resource_name("redefaulter", None)?;

        let initial_tooltip = format!("{} - Initializing", TOOLTIP_PREFIX);

        append_root(&menu)?;

        // We create the icon late (once the event loop is actually running)
        // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
        let handle = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(initial_tooltip)
            .with_icon(initial_icon)
            .build()?;

        Ok(handle)
    }
    pub fn update_tray_menu(&self) -> AppResult<()> {
        if let Some(handle) = self.tray_menu.as_ref() {
            let new_tooltip = format!(
                "{} - {} profiles active",
                TOOLTIP_PREFIX,
                self.active_profiles.len()
            );
            handle.set_tooltip(Some(new_tooltip))?;
            let new_menu = self.build_tray_contents()?;
            handle.set_menu(Some(Box::new(new_menu)));
        }
        Ok(())
    }
    // Regenerate menu each time? or on click...
    // Right now it's on each profile change
    pub fn build_tray_contents(&self) -> AppResult<Menu> {
        let menu = Menu::new();

        // settings section
        // submenu for each device role
        // hide communications role if unify enabled
        // section for editing active profiles

        let settings_submenu = self.build_tray_settings_submenu()?;

        menu.append(&settings_submenu)?;

        menu.append(&PredefinedMenuItem::separator())?;

        let total_profiles = self.profiles.len();

        if total_profiles == 0 {
            let text = format!("No Profiles Loaded!");
            menu.append(&MenuItem::new(text, false, None))?;
        } else if self.active_profiles.is_empty() {
            let text = format!("No Profiles Active ({total_profiles} loaded)");
            menu.append(&MenuItem::new(text, false, None))?;
        } else {
            let item = MenuItem::new("Active Profiles:", false, None);
            menu.append(&item)?;
            // Generate submenus to edit active profiles
            for (profile_name, profile) in self.active_profiles.iter() {
                let Some(profile_name_str) = profile_name.to_str() else {
                    // let incomplete_item = SubmenuBuilder::new()
                    //     .enabled(true)
                    //     .item(&playback_submenu)
                    //     .text(profile_name_str)
                    //     .build()?;
                    // menu.append(&incomplete_item)?;
                    // TODO: Opener::reveal the item
                    // continue;
                    panic!();
                };
                let profile_submenus = self.tray_platform_device_selection(
                    &DeviceSelectionType::Profile(profile_name_str),
                    &profile.override_set,
                )?;
                let submenu_refs = profile_submenus
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<_>>();
                let item = SubmenuBuilder::new()
                    .enabled(true)
                    .items(&submenu_refs)
                    .text(profile_name_str)
                    .build()?;
                menu.append(&item)?;
            }
        }

        menu.append(&PredefinedMenuItem::separator())?;

        // Device selection for global default
        let profile_submenus = self.tray_platform_device_selection(
            &DeviceSelectionType::ConfigDefault,
            &self.settings.platform.default_devices,
        )?;
        let submenu_refs = profile_submenus
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>();
        menu.append_items(&submenu_refs)?;

        append_root(&menu)?;

        Ok(menu)
    }
    fn build_tray_settings_submenu(&self) -> AppResult<Submenu> {
        // This a little cursed, but it's the best solution I can think of currently.
        // All of the menu methods that take in multiple items take in &[&dyn IsMenuItem]
        // So I have to store the built objects somewhere else to be able to return *only* references to the dyn type
        // or just rereference it here.
        // And I can just chain them without any intermediary variables, so, fine.
        let submenu = SubmenuBuilder::new()
            .enabled(true)
            .text("Settings")
            .items(
                &self
                    .settings
                    .platform
                    .build_check_menu_items()
                    .iter()
                    .map(|item| item as &dyn IsMenuItem)
                    .collect::<Vec<_>>(),
            )
            .build()?;

        Ok(submenu)
    }
    pub fn handle_tray_menu_event(
        &mut self,
        event: MenuEvent,
        control_flow: &mut ControlFlow,
    ) -> AppResult<()> {
        let id = event.id.as_ref();
        match id {
            QUIT_ID => {
                *control_flow = ControlFlow::Exit;
            }
            RELOAD_ID => {
                // TODO Popup when failing to read a file?
                self.reload_profiles().unwrap();
            }
            REVEAL_ID => {
                opener::reveal(PROFILES_PATH)?;
            }
            _ if id.starts_with(self.settings.platform.menu_id_root()) => {
                self.settings.platform.handle_menu_toggle_event(id)?;
                self.settings.save(&self.config_path)?;
                self.endpoints.update_config(&self.settings.platform);
                // rebuild menu
                self.update_tray_menu()?;
                debug!("{:#?}", self.settings.platform);
            }
            IGNORE_ID => {
                self.update_tray_menu()?;
            }
            tray_device if id.starts_with(DEVICE_PREFIX) => {
                let tray_device = serde_plain::from_str::<TrayDevice>(tray_device)?;

                // println!("{tray_device:#?}");

                self.handle_tray_device_selection(tray_device)?;

                // println!("{:#?}", self.settings.platform.default_devices);

                self.update_tray_menu()?;
            }
            _ => (),
        }
        Ok(())
    }
    fn handle_tray_device_selection(&mut self, tray_device: TrayDevice) -> AppResult<()> {
        let set_to_modify = match &tray_device.destination {
            DeviceSelectionType::ConfigDefault => {
                self.settings.platform.default_devices.borrow_mut()
            }
            DeviceSelectionType::Profile(profile) => self
                .profiles
                .get_mutable_profile(profile)
                .unwrap()
                .override_set
                .borrow_mut(),
        };

        match &tray_device.guid {
            Some(guid) => {
                self.endpoints
                    .update_config_entry(set_to_modify, &tray_device.role, guid, true)?;
            }
            None => set_to_modify.clear_role(&tray_device.role),
        }

        match &tray_device.destination {
            DeviceSelectionType::ConfigDefault => {
                self.settings.save(&self.config_path)?;
            }
            DeviceSelectionType::Profile(profile) => {
                self.profiles.save_profile(profile)?;
            }
        }

        // println!("{:#?}", self.get_damaged_devices(&self.active_profiles));

        self.change_devices_if_needed()?;

        Ok(())
    }
}

fn append_root(menu: &Menu) -> AppResult<()> {
    let quit = MenuItem::with_id(QUIT_ID, "&Quit", true, None);
    let reload = MenuItem::with_id(RELOAD_ID, "&Reload Profiles", true, None);
    let reveal = MenuItem::with_id(REVEAL_ID, "Reveal Profiles &Folder", true, None);
    menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &reload,
        &reveal,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;

    Ok(())
}

pub fn build_device_checks(
    devices: &BTreeMap<String, DiscoveredDevice>,
    selection_type: &DeviceSelectionType,
    role: &DeviceRole,
    config_device: &ConfigDevice,
    discovered_device: Option<&DiscoveredDevice>,
) -> Vec<Box<dyn IsMenuItem>> {
    let mut items: Vec<Box<dyn IsMenuItem>> = Vec::new();

    use DeviceSelectionType::*;
    let none_text = match selection_type {
        ConfigDefault => "None",
        Profile(_) => "No Override",
    };

    // Dunno if I want to keep it like this
    // or be prefix-none
    let none_item = TrayDevice::none(selection_type, role);
    items.push(Box::new(CheckMenuItem::with_id(
        &none_item.to_string(),
        &none_text,
        true,
        config_device.is_empty(),
        None,
    )));

    items.push(Box::new(PredefinedMenuItem::separator()));

    let mut device_found = false;

    for device in devices.values() {
        let tray_device = TrayDevice::new(selection_type, role, &device.guid);
        let chosen = if let Some(chosen) = discovered_device.as_ref() {
            device_found = true;
            *chosen.guid == device.guid
        } else {
            false
        };
        items.push(Box::new(CheckMenuItem::with_id(
            &tray_device.to_string(),
            &device.to_string(),
            true,
            chosen,
            None,
        )));
    }

    // Checking if we have a device configured but wasn't in our list of known active devices
    if !config_device.is_empty() && !device_found {
        items.push(Box::new(PredefinedMenuItem::separator()) as Box<dyn IsMenuItem>);
        // Giving this an ignore id, since if someone clicks it
        // it unchecks the listing in the tray, when instead the user
        // should be clicking the None item to clear the config entry.
        let derived_name = format!("(Not Found) {}", config_device.to_string());
        items.push(Box::new(CheckMenuItem::with_id(
            IGNORE_ID,
            &derived_name,
            true,
            true,
            None,
        )));
    }

    items
}
