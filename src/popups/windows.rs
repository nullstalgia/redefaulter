use std::thread;
use win_msgbox::{Okay, RetryCancel, YesNo};

use crate::{
    app::{AppEventProxy, CustomEvent},
    errors::RedefaulterError,
    processes::LockFile,
};

// TODO First time setup:
// Update check popup
// Set current devices as desired
// Unify devices popup
// Make a profile popup

pub fn profile_load_failed_popup(error: RedefaulterError, event_proxy: AppEventProxy) {
    thread::spawn(move || {
        let response = win_msgbox::error::<RetryCancel>(&format!(
            "{error}\n\nPlease fix the profile and try again."
        ))
        .title("Redefaulter Error")
        .show()
        .expect("Couldn't show error popup!");

        match response {
            RetryCancel::Retry => event_proxy.send_event(CustomEvent::ReloadProfiles).unwrap(),
            RetryCancel::Cancel => (),
        }
    });
}

pub fn settings_load_failed_popup(error: RedefaulterError, lock_file: LockFile) -> ! {
    win_msgbox::error::<Okay>(&format!(
        "{error}\n\nPlease fix the settings file and try again."
    ))
    .title("Redefaulter Error")
    .show()
    .expect("Couldn't show error popup!");

    drop(lock_file);

    std::process::exit(libc::EXIT_FAILURE);
}

pub fn fatal_error_popup(error: RedefaulterError, lock_file: Option<LockFile>) -> ! {
    win_msgbox::error::<Okay>(&format!(
        "Fatal error!\n{error}\n\nCheck the logs for more info, and consider submitting them in an issue.\nShutting down."
    ))
    .title("Fatal Redefaulter Error!")
    .show()
    .expect("Couldn't show error popup!");

    if let Some(lock_file) = lock_file {
        drop(lock_file);
    }

    std::process::exit(libc::EXIT_FAILURE);
}

pub fn allow_update_check_popup(event_proxy: AppEventProxy) {
    thread::spawn(move || {
        let response = win_msgbox::information::<YesNo>(
            "Allow Redefaulter to check for updates once during startup?",
        )
        .title("Redefaulter update check")
        .show()
        .expect("Couldn't show update check popup");

        let response = match response {
            YesNo::Yes => CustomEvent::UpdateCheckConsent(true),
            YesNo::No => CustomEvent::UpdateCheckConsent(false),
        };

        event_proxy.send_event(response).unwrap();
    });
}

pub fn start_new_version_popup() {
    win_msgbox::information::<Okay>("Update complete! Ready to launch new version!")
        .title("Redefaulter update success!")
        .show()
        .expect("Couldn't show update complete popup");
}
