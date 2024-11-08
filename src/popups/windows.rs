use std::thread;
use tao::event_loop::EventLoopProxy;
use win_msgbox::{Okay, RetryCancel};

use crate::{app::CustomEvent, errors::RedefaulterError};

pub fn profile_load_failed_popup(
    error: RedefaulterError,
    event_proxy: EventLoopProxy<CustomEvent>,
) {
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

pub fn settings_load_failed_popup(error: RedefaulterError) -> ! {
    win_msgbox::error::<Okay>(&format!(
        "{error}\n\nPlease fix the settings file and try again."
    ))
    .title("Redefaulter Error")
    .show()
    .expect("Couldn't show error popup!");

    std::process::exit(libc::EXIT_FAILURE);
}

pub fn fatal_error_popup(error: RedefaulterError) -> ! {
    win_msgbox::error::<Okay>(&format!(
        "Fatal error!\n{error}\n\nCheck the logs for more info, and consider submitting them in an issue.\nShutting down."
    ))
    .title("Fatal Redefaulter Error!")
    .show()
    .expect("Couldn't show error popup!");

    std::process::exit(libc::EXIT_FAILURE);
}
