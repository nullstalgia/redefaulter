use std::{collections::BTreeMap, ffi::OsString};

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

        // settings section
        // submenu for each device
        // section for active profiles

        menu.append_items(&[
            // &PredefinedMenuItem::about(
            //     None,
            //     Some(AboutMetadata {
            //         name: Some("tao".to_string()),
            //         copyright: Some("Copyright tao".to_string()),
            //         ..Default::default()
            //     }),
            // ),
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
            .with_tooltip("Redefaulter - Initializing")
            .with_icon(initial_icon)
            .build()?;

        Ok(Self { root: menu, handle })
    }
    pub fn update_profiles(&mut self, profiles: &BTreeMap<OsString, AppOverride>) -> AppResult<()> {
        let new_tooltip = format!("{} - {} profiles active", TOOLTIP_PREFIX, profiles.len());
        self.handle.set_tooltip(Some(new_tooltip))?;
        Ok(())
    }
}
