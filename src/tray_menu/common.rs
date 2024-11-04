use std::{collections::BTreeMap, ffi::OsString};

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
        AudioNightmare, ConfigDevice, ConfigEntry, DeviceSet, Discovered, DiscoveredDevice,
        PlatformSettings,
    },
    profiles::{AppOverride, PROFILES_PATH},
};

pub mod common_ids {
    // Ids for root menu buttons, for all platforms
    pub const QUIT_ID: &str = "quit";
    pub const RELOAD_ID: &str = "reload";
    pub const REVEAL_ID: &str = "reveal";

    pub const CONFIG_DEFAULT_ID: &str = "config";

    pub const DEVICE_PREFIX: &str = "device";

    pub const IGNORE_ID: &str = "ignore";
}

pub const TOOLTIP_PREFIX: &str = "Redefaulter";

use common_ids::*;

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

        // We create the icon once the event loop is actually running
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

        // wretched de-evolution in the name of dynamic dispatch

        // let items: Vec<CheckMenuItem> = settings.build_check_menu_items();
        // let item_refs: Vec<&dyn IsMenuItem> =
        //     items.iter().map(|item| item as &dyn IsMenuItem).collect();
        // menu.prepend_items(&item_refs)?;

        // menu.append_items(
        //     &settings
        //         .build_check_menu_items()
        //         .iter()
        //         .map(|item| item as &dyn IsMenuItem)
        //         .collect::<Vec<_>>(),
        // )?;

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
            // Eh, muda also just calls append in a loop with the _items version
            for profile in self.active_profiles.keys() {
                let item = MenuItem::new(profile.to_string_lossy(), false, None);
                menu.append(&item)?;
            }
        }

        menu.append(&PredefinedMenuItem::separator())?;

        // Device selection for global default
        let menus = self.tray_platform_config_device_selection()?;

        for submenu in menus.into_iter() {
            menu.append(&submenu)?;
        }

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
            guid if id.starts_with(CONFIG_DEFAULT_ID) => {
                self.update_tray_menu()?;
            }
            _ => (),
        }
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

/// An enum to help with titling submenus.
pub enum DeviceSelectionType {
    /// This set of device selections is for the app's globally desired default
    ConfigDefault,
    /// This set of device selections is for changing a profile's set defaults
    Profile,
}
