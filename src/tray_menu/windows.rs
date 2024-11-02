use tray_icon::menu::{CheckMenuItem, IsMenuItem, MenuEvent, MenuItem};

use crate::{app::App, platform::PlatformSettings};

use super::TrayHelper;

impl TrayHelper {
    pub fn platform_settings(&self, settings: &PlatformSettings) -> Vec<Box<dyn IsMenuItem>> {
        let mut items: Vec<Box<dyn IsMenuItem>> = Vec::new();

        let header = MenuItem::new("Settings:", false, None);

        items.push(Box::new(header));

        items
    }
}

impl App {}
