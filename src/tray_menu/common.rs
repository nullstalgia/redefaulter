use std::{collections::BTreeMap, ffi::OsString};

use tao::event_loop::ControlFlow;
use tracing::*;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, SubmenuBuilder},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::{
    app::App,
    errors::AppResult,
    platform::PlatformSettings,
    profiles::{AppOverride, PROFILES_PATH},
};

pub mod common_ids {
    pub const QUIT_ID: &str = "quit";
    pub const RELOAD_ID: &str = "reload";
    pub const REVEAL_ID: &str = "reveal";
}
pub const TOOLTIP_PREFIX: &str = "Redefaulter";

use common_ids::*;

pub struct TrayHelper {
    handle: TrayIcon,
    root: Menu,
}

// TODO Consolidate menu root

impl TrayHelper {
    pub fn build() -> AppResult<Self> {
        let menu = Menu::new();

        let quit = MenuItem::with_id(QUIT_ID, "&Quit", true, None);
        let reload = MenuItem::with_id(RELOAD_ID, "&Reload Profiles", true, None);
        let reveal = MenuItem::with_id(REVEAL_ID, "Reveal Profiles &Folder", true, None);
        let loading = MenuItem::new(format!("Loading profiles..."), false, None);

        menu.append_items(&[
            &loading,
            &PredefinedMenuItem::separator(),
            &reload,
            &reveal,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        // drop(quit_i);

        // Add a copy to the struct if we start changing the icon?
        let initial_icon = Icon::from_resource_name("redefaulter", None)?;

        let initial_tooltip = format!("{} - Initializing", TOOLTIP_PREFIX);

        // We create the icon once the event loop is actually running
        // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
        let handle = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip(initial_tooltip)
            .with_icon(initial_icon)
            .build()?;

        Ok(Self { root: menu, handle })
    }
    pub fn update_menu(
        &mut self,
        total_profiles: usize,
        profiles: &BTreeMap<OsString, AppOverride>,
        settings: &PlatformSettings,
    ) -> AppResult<()> {
        let new_tooltip = format!("{} - {} profiles active", TOOLTIP_PREFIX, profiles.len());
        self.handle.set_tooltip(Some(new_tooltip))?;
        let new_menu = self.build_menu(total_profiles, profiles, settings)?;
        self.handle.set_menu(Some(Box::new(new_menu)));
        Ok(())
    }
    // Regenerate menu each time? or on click...
    // Right now it's on each profile change
    pub fn build_menu(
        &mut self,
        total_profiles: usize,
        profiles: &BTreeMap<OsString, AppOverride>,
        settings: &PlatformSettings,
    ) -> AppResult<Menu> {
        let menu = Menu::new();

        // settings section
        // submenu for each device role
        // hide communications role if unify enabled
        // section for editing active profiles

        for item in self.platform_settings(settings) {
            menu.append(item.as_ref())?;
        }

        menu.append(&PredefinedMenuItem::separator())?;

        if profiles.is_empty() {
            let text = format!("No Profiles Active ({total_profiles} loaded)");
            let item = MenuItem::new(text, false, None);
            menu.append(&item)?;
        } else {
            let item = MenuItem::new("Active Profiles:", false, None);
            menu.append(&item)?;
            // Eh, muda also just calls append in a loop with the _items version
            for profile in profiles.keys() {
                let item = MenuItem::new(profile.to_string_lossy(), false, None);
                menu.append(&item)?;
            }
        }

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

        Ok(menu)
    }
}

impl App {
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
                self.handle_platform_settings_menu_event(event);
            }
            _ => (),
        }
        Ok(())
    }
}
