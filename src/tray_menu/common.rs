use tray_icon::{
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, SubmenuBuilder},
    Icon, TrayIcon, TrayIconBuilder,
};

use crate::errors::AppResult;

pub const QUIT_ID: &str = "quit";

pub struct TrayHelper {
    handle: TrayIcon,
    root: Menu,
}

impl TrayHelper {
    pub fn build() -> AppResult<Self> {
        let menu = Menu::new();

        let quit_i = MenuItem::with_id(QUIT_ID, "&Quit", true, None);

        let submenu = SubmenuBuilder::new().enabled(true).build()?;

        menu.append_items(&[
            &PredefinedMenuItem::about(
                None,
                Some(AboutMetadata {
                    name: Some("tao".to_string()),
                    copyright: Some("Copyright tao".to_string()),
                    ..Default::default()
                }),
            ),
            &PredefinedMenuItem::separator(),
            &quit_i,
        ])?;
        drop(quit_i);

        // Add a copy to the struct if we start changing the icon?
        let initial_icon = Icon::from_resource_name("redefaulter", None)?;

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
}
