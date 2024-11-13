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
    platform::{ConfigDevice, DeviceRole, DiscoveredDevice},
    popups::executable_file_picker,
    profiles::PROFILES_PATH,
    tray_menu::TrayDevice,
    updates::UpdateState,
};

pub mod common_ids {
    // Ids for root menu buttons, for all platforms
    pub const QUIT_ID: &str = "quit";
    pub const RELOAD_ID: &str = "reload";
    pub const REVEAL_ID: &str = "reveal";

    pub const NEW_SAVE_NAME: &str = "new-name";
    pub const NEW_SAVE_PATH: &str = "new-path";

    pub const AUTO_LAUNCH_ID: &str = "auto-launch";

    pub const DEVICE_PREFIX: &str = "device";

    pub const IGNORE_ID: &str = "ignore";

    pub const UPDATE_PREFIX: &str = "update";
    pub const UPDATE_DOWNLOAD: &str = "update-download";
    pub const UPDATE_OPEN_REPO: &str = "update-repo";
    pub const UPDATE_DISMISS: &str = "update-dismiss";
    pub const UPDATE_SKIP_VERSION: &str = "update-skip";
}

pub const TOOLTIP_PREFIX: &str = "Redefaulter";

use common_ids::*;

use super::{tray_update_submenu, DeviceSelectionType};

impl App {
    pub fn build_tray_late(&mut self) -> AppResult<TrayIcon> {
        let menu = Menu::new();

        let loading = MenuItem::new("Loading profiles...", false, None);

        menu.append(&loading)?;

        // drop(quit_i);

        self.normal_icon = Some(Icon::from_resource_name("redefaulter", None)?);
        self.update_icon = Some(Icon::from_resource_name("redefaulter-update", None)?);

        let initial_tooltip = format!("{} - Initializing", TOOLTIP_PREFIX);

        self.append_root(&menu)?;

        // We create the icon late (once the event loop is actually running)
        // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
        let handle = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(initial_tooltip)
            .with_icon(self.normal_icon.clone().unwrap())
            .build()?;

        Ok(handle)
    }
    pub fn kill_tray_menu(&mut self) -> Option<TrayIcon> {
        self.tray_menu.take()
    }
    pub fn update_tray_menu(&self) -> AppResult<()> {
        if let Some(handle) = self.tray_menu.as_ref() {
            let new_tooltip = format!(
                "{} - {} profiles active",
                TOOLTIP_PREFIX,
                self.profiles.active_len()
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

        match &self.update_state {
            UpdateState::Idle => (),
            UpdateState::Downloading => {
                let downloading = label_item("Downloading update...");
                menu.append(&downloading)?;
                menu.append(&PredefinedMenuItem::separator())?;
            }
            UpdateState::UpdateFound(version) => {
                let update_submenu = tray_update_submenu(version)?;
                menu.append(&update_submenu)?;
                menu.append(&PredefinedMenuItem::separator())?;
            }
        }

        if self.settings.behavior.show_active_devices {
            let active_devices = self.tray_platform_active_devices()?;
            let item_refs = active_devices
                .iter()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>();
            menu.append_items(&item_refs)?;

            menu.append(&PredefinedMenuItem::separator())?;
        }

        let total_profiles = self.profiles.len();
        let active_profiles = self.profiles.active_len();

        if total_profiles == 0 {
            let text = "No Profiles Loaded!";
            menu.append(&MenuItem::new(text, false, None))?;
        } else if !self.profiles.any_active() {
            let text = format!("No Profiles Active ({total_profiles} loaded)");
            menu.append(&MenuItem::new(text, false, None))?;
        } else {
            let text = format!("Active Profiles ({active_profiles}/{total_profiles}):");
            let item = MenuItem::new(text, false, None);
            menu.append(&item)?;
            // Generate submenus to edit active profiles
            for (profile_name, profile) in self.profiles.get_active_profiles() {
                let Some(profile_name_str) = profile_name.to_str() else {
                    let incomplete_item = SubmenuBuilder::new()
                        .enabled(true)
                        .text("Invalid UTF-8 Filename!")
                        .build()?;
                    menu.append(&incomplete_item)?;
                    continue;
                    // TODO: Opener::reveal the item?
                    // Except I can't put the filename in the ID without losing content....
                    // I could maybe represent *all* OsStrings destined to be
                    // sent into the menu_id's &str as hex bytes/base64 or something,
                    // but I'd rather just wait for someone to ask for it than spend a lot
                    // of time on it right now.
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

        self.append_root(&menu)?;

        Ok(menu)
    }
    fn build_tray_settings_submenu(&self) -> AppResult<Submenu> {
        // This a little cursed, but it's the best solution I can think of currently.
        // All of the menu methods that take in multiple items take in &[&dyn IsMenuItem]
        // So I have to store the built objects somewhere else to be able to return *only* references to the dyn type
        // or just rereference it here.
        // And I can just chain them without any intermediary variables, so, fine.

        let settings_text = format!("Settings - v{}", env!("CARGO_PKG_VERSION"));

        let mut extra_items: Vec<Box<dyn IsMenuItem>> = Vec::new();

        let checked = match self.get_auto_launch_enabled() {
            Ok(state) => state,
            Err(e) => {
                warn!("Error getting auto-launch state! Defaulting to false. {e}");
                false
            }
        };
        let auto_launch_item = CheckMenuItem::with_id(
            AUTO_LAUNCH_ID,
            "Open Redefaulter on Login",
            true,
            checked,
            None,
        );
        extra_items.push(Box::new(auto_launch_item));

        let extra_refs = extra_items.iter().map(|i| i.as_ref()).collect::<Vec<_>>();

        let submenu = SubmenuBuilder::new()
            .enabled(true)
            .text(settings_text)
            .items(
                &self
                    .settings
                    .updates
                    .build_check_menu_items()
                    .iter()
                    .map(|item| item as &dyn IsMenuItem)
                    .collect::<Vec<_>>(),
            )
            .items(&extra_refs)
            .items(
                &self
                    .settings
                    .behavior
                    .build_check_menu_items()
                    .iter()
                    .map(|item| item as &dyn IsMenuItem)
                    .collect::<Vec<_>>(),
            )
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
    /// Takes in a raw event from the tray menu, dispatching commands as requested.
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
                self.reload_profiles()?;
            }
            REVEAL_ID => {
                opener::reveal(PROFILES_PATH)?;
            }
            _ if id.starts_with(self.settings.platform.menu_id_root()) => {
                self.settings.platform.handle_menu_toggle_event(id)?;
                self.settings.save(&self.config_path)?;
                self.endpoints.update_config(&self.settings.platform);
                self.update_tray_menu()?;
                // debug!("{:#?}", self.settings.platform);
            }
            _ if id.starts_with(self.settings.behavior.menu_id_root()) => {
                self.settings.behavior.handle_menu_toggle_event(id)?;
                self.settings.save(&self.config_path)?;
                self.update_tray_menu()?;
                // debug!("{:#?}", self.settings.behavior);
            }
            IGNORE_ID => {
                // Rebuilding menu here since if the user clicked a CheckItem,
                // it would toggle visually but nothing would happen internally.
                self.update_tray_menu()?;
            }
            tray_device if id.starts_with(DEVICE_PREFIX) => {
                let tray_device = serde_plain::from_str::<TrayDevice>(tray_device)?;

                // println!("{tray_device:#?}");

                self.handle_tray_device_selection(tray_device)?;

                // println!("{:#?}", self.settings.platform.default_devices);

                self.update_tray_menu()?;
            }
            update_command if id.starts_with(UPDATE_PREFIX) => match update_command {
                UPDATE_DISMISS => {
                    _ = self.updates.take();
                    self.update_state = UpdateState::Idle;
                    if let Some(tray) = self.tray_menu.as_ref() {
                        tray.set_icon(self.normal_icon.clone())?;
                        self.update_tray_menu()?;
                    }
                }
                UPDATE_SKIP_VERSION => {
                    _ = self.updates.take();
                    let UpdateState::UpdateFound(version) = &self.update_state else {
                        panic!();
                    };
                    self.settings.updates.version_skipped = version.to_owned();
                    self.settings.save(&self.config_path)?;
                    self.update_state = UpdateState::Idle;
                    if let Some(tray) = self.tray_menu.as_ref() {
                        tray.set_icon(self.normal_icon.clone())?;
                        self.update_tray_menu()?;
                    }
                }
                UPDATE_OPEN_REPO => {
                    let url = format!("{}/releases", env!("CARGO_PKG_REPOSITORY"));
                    opener::open_browser(url)?;
                }
                UPDATE_DOWNLOAD => {
                    self.update_state = UpdateState::Downloading;
                    self.update_tray_menu()?;
                    self.updates.download_update();
                }
                _ => error!("Invalid update menu command!"),
            },
            NEW_SAVE_NAME => {
                executable_file_picker(self.event_proxy.clone(), false);
            }
            NEW_SAVE_PATH => {
                executable_file_picker(self.event_proxy.clone(), true);
            }
            AUTO_LAUNCH_ID => {
                let auto_launch_enabled = self.get_auto_launch_enabled()?;
                self.set_auto_launch(!auto_launch_enabled)?;
                self.update_tray_menu()?;
            }
            _ => (),
        }
        Ok(())
    }
    /// Takes in a deserialized device click event, modifies the specified profile, and saves the relevant file.
    fn handle_tray_device_selection(&mut self, tray_device: TrayDevice) -> AppResult<()> {
        let set_to_modify = match &tray_device.destination {
            DeviceSelectionType::ConfigDefault => {
                self.settings.platform.default_devices.borrow_mut()
            }
            DeviceSelectionType::Profile(profile) => self
                .profiles
                .get_mutable_profile(OsString::from(profile))
                .unwrap()
                .override_set
                .borrow_mut(),
        };

        match &tray_device.guid {
            Some(guid) => {
                self.endpoints.update_config_entry(
                    set_to_modify,
                    &tray_device.role,
                    guid,
                    self.settings.behavior.always_save_generics,
                )?;
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

    fn append_root(&self, menu: &Menu) -> AppResult<()> {
        let new_profile = SubmenuBuilder::new()
            .enabled(true)
            .text("New Profile...")
            .item(&MenuItem::with_id(
                NEW_SAVE_NAME,
                "...with Process Name",
                true,
                None,
            ))
            .item(&MenuItem::with_id(
                NEW_SAVE_PATH,
                "...with Full Path",
                true,
                None,
            ))
            .build()?;
        let reload = MenuItem::with_id(RELOAD_ID, "&Reload Profiles", true, None);
        let reveal = MenuItem::with_id(REVEAL_ID, "Reveal Profiles &Folder", true, None);
        let settings_submenu = self.build_tray_settings_submenu()?;
        let quit = MenuItem::with_id(QUIT_ID, "&Quit", true, None);

        menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &new_profile,
            &reload,
            &reveal,
            &PredefinedMenuItem::separator(),
            &settings_submenu,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;

        Ok(())
    }
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
        none_item.to_string(),
        none_text,
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
            tray_device.to_string(),
            device.to_string(),
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
        let derived_name = format!("(Not Found) {config_device}");
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

pub fn label_item<S: AsRef<str>>(text: S) -> MenuItem {
    MenuItem::with_id(IGNORE_ID, text.as_ref(), false, None)
}
