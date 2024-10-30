use std::{collections::BTreeMap, ffi::OsString};

use tracing::debug;
use tray_icon::{
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, SubmenuBuilder},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::{errors::AppResult, profiles::AppOverride};

pub const QUIT_ID: &str = "quit";

pub const RELOAD_ID: &str = "reload";

pub const TOOLTIP_PREFIX: &str = "Redefaulter";

pub struct TrayHelper {
    handle: TrayIcon,
    root: Menu,
}

impl TrayHelper {
    pub fn build() -> AppResult<Self> {
        let menu = Menu::new();

        let quit = MenuItem::with_id(QUIT_ID, "&Quit", true, None);
        let reload = MenuItem::with_id(RELOAD_ID, "&Reload Profiles", true, None);

        menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &reload,
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
            // TODO Show number of actives profiles in tooltip
            .with_tooltip(initial_tooltip)
            .with_icon(initial_icon)
            .build()?;

        Ok(Self { root: menu, handle })
    }
    pub fn update_profiles(
        &mut self,
        total_profiles: usize,
        profiles: &BTreeMap<OsString, AppOverride>,
    ) -> AppResult<()> {
        let new_tooltip = format!("{} - {} profiles active", TOOLTIP_PREFIX, profiles.len());
        self.handle.set_tooltip(Some(new_tooltip))?;
        let new_menu = self.build_menu(total_profiles, profiles)?;
        self.handle.set_menu(Some(Box::new(new_menu)));
        Ok(())
    }
    pub fn build_menu(
        &mut self,
        total_profiles: usize,
        profiles: &BTreeMap<OsString, AppOverride>,
    ) -> AppResult<Menu> {
        let menu = Menu::new();

        // settings section
        // submenu for each device
        // section for active profiles

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
        menu.append_items(&[
            &PredefinedMenuItem::separator(),
            &reload,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;

        Ok(menu)
    }
}
