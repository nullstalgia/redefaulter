#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

use crate::app::{AppEventProxy, CustomEvent};

#[derive(Debug)]
pub enum FirstTimeChoice {
    UpdateCheckConsent(bool),
    UseCurrentDefaults,
    PlatformChoice(PlatformPrompts),
    SetupFinished,
}

impl From<FirstTimeChoice> for CustomEvent {
    fn from(value: FirstTimeChoice) -> Self {
        Self::FirstTimeChoice(value)
    }
}

pub fn executable_file_picker(event_proxy: AppEventProxy, save_absolute_path: bool) {
    std::thread::spawn(move || {
        let dialog = rfd::FileDialog::new().set_title("Select path of executable to watch for:");

        #[cfg(windows)]
        let dialog = dialog.add_filter("Executable", &["exe"]);

        let dialog = dialog.add_filter("All Files", &["*"]);

        let chosen = dialog.pick_file();
        let Some(path) = chosen else {
            return;
        };
        event_proxy
            .send_event(CustomEvent::NewProfile(path, save_absolute_path))
            .unwrap();
    });
}
