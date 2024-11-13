use muda::{IsMenuItem, MenuItem, Submenu, SubmenuBuilder};

use crate::errors::AppResult;

use super::common_ids::*;

// impl App {

// }

pub fn tray_update_submenu(version: &str) -> AppResult<Submenu> {
    // let UpdateState::UpdateFound(version) = &self.update_state else {
    //     panic!()
    // };

    let menu_items: Vec<MenuItem> = vec![
        #[cfg(feature = "self-replace")]
        MenuItem::with_id(UPDATE_DOWNLOAD, "Download and Install", true, None),
        MenuItem::with_id(UPDATE_OPEN_REPO, "Open GitHub Repository", true, None),
        MenuItem::with_id(UPDATE_SKIP_VERSION, "Skip this Version", true, None),
        MenuItem::with_id(UPDATE_DISMISS, "Ask Again Later", true, None),
    ];
    let menu_item_refs = menu_items
        .iter()
        .map(|i| i as &dyn IsMenuItem)
        .collect::<Vec<_>>();
    let text = format!("Update Found! (v{version})");
    let submenu = SubmenuBuilder::new()
        .enabled(true)
        .text(text)
        .items(&menu_item_refs)
        .build()?;

    Ok(submenu)
}
