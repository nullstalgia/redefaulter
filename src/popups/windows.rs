use std::thread;
use win_msgbox::{Okay, RetryCancel, YesNo, YesNoCancel};

use crate::{
    app::{App, AppEventProxy, CustomEvent},
    errors::{AppResult, RedefaulterError},
    platform::{DeviceRole, DeviceSet, Discovered},
    processes::LockFile,
};

use super::FirstTimeChoice;

#[derive(Debug)]
pub enum PlatformPrompts {
    UnifyCommunications(bool),
}

impl App {
    pub fn handle_platform_first_time_choice(&mut self, choice: PlatformPrompts) -> AppResult<()> {
        match choice {
            PlatformPrompts::UnifyCommunications(unify) => {
                self.settings.platform.unify_communications_devices = unify;
                if unify {
                    self.settings
                        .platform
                        .default_devices
                        .clear_role(&DeviceRole::PlaybackComms);
                    self.settings
                        .platform
                        .default_devices
                        .clear_role(&DeviceRole::RecordingComms);
                }
                self.change_devices_if_needed()?;
                self.update_tray_menu()?;
            }
        }
        Ok(())
    }
}

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

pub fn profile_exists_popup(error: RedefaulterError) {
    thread::spawn(move || {
        win_msgbox::error::<Okay>(&format!("Error creating profile!\n{error}"))
            .title("Redefaulter Error")
            .show()
            .expect("Couldn't show error popup!");
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
#[cfg(feature = "self-replace")]
pub fn start_new_version_popup() {
    win_msgbox::information::<Okay>("Update complete! Ready to launch new version!")
        .title("Redefaulter update success!")
        .show()
        .expect("Couldn't show update complete popup");
}

pub fn first_time_popups(
    current_defaults: DeviceSet<Discovered>,
    event_proxy: AppEventProxy,
    auto_launch_init: bool,
) {
    thread::spawn(move || {
        first_time_impl(current_defaults, &event_proxy, auto_launch_init);
        event_proxy
            .send_event(CustomEvent::FirstTimeChoice(FirstTimeChoice::SetupFinished))
            .unwrap();
    });
}

// Lazy way of doing this, should maybe be part of the set methods?
fn format_devices(devices: &DeviceSet<Discovered>, unify_example: bool) -> String {
    let mut buffer = String::new();

    let device_string = |role: &DeviceRole| -> String {
        let device = devices.get_role(role);
        let human_name = &device.human_name;
        format!("{role}: {human_name}\n")
    };

    use DeviceRole::*;

    if !unify_example {
        buffer.push_str(&device_string(&Playback));
        buffer.push_str(&device_string(&PlaybackComms));
        buffer.push_str(&device_string(&Recording));
        buffer.push_str(&device_string(&RecordingComms));
    } else {
        buffer.push_str(&device_string(&Playback));
        buffer.push_str(&device_string(&PlaybackComms));
        buffer.push_str("\nwould be considered as just\n\n");
        buffer.push_str(&device_string(&Playback));
    }
    buffer
}

fn first_time_impl(
    current_defaults: DeviceSet<Discovered>,
    event_proxy: &AppEventProxy,
    auto_launch_init: bool,
) {
    let all_devices = format_devices(&current_defaults, false);
    let unified_devices = format_devices(&current_defaults, true);

    let current_device_prompt =
        format!("Set your current devices as your preferred defaults?\n\n{all_devices}",);

    let unify_comms_prompt = format!(
        r#"Would you like to enable "Unify Communications Devices"?

Playback and Recording Communication devices always be forced to follow the Default Playback or Recording device.

(In app override profiles, Communications devices will be ignored.)


{unified_devices}"#,
    );

    type Mapper = fn(YesNoCancel) -> Option<FirstTimeChoice>;

    let mut prompts: Vec<(String, Mapper)> = vec![
        (
            "Allow Redefaulter to check for updates once during startup?".to_string(),
            |c| match c {
                YesNoCancel::Yes => Some(FirstTimeChoice::UpdateCheckConsent(true)),
                YesNoCancel::No => Some(FirstTimeChoice::UpdateCheckConsent(false)),
                _ => None,
            },
        ),
        (current_device_prompt, |c| match c {
            YesNoCancel::Yes => Some(FirstTimeChoice::UseCurrentDefaults),
            _ => None,
        }),
        (unify_comms_prompt, |c| match c {
            YesNoCancel::Yes => Some(FirstTimeChoice::PlatformChoice(
                PlatformPrompts::UnifyCommunications(true),
            )),
            YesNoCancel::No => Some(FirstTimeChoice::PlatformChoice(
                PlatformPrompts::UnifyCommunications(false),
            )),
            _ => None,
        }),
    ];

    if auto_launch_init {
        let auto_launch_prompt: Mapper = |c| match c {
            YesNoCancel::Yes => Some(FirstTimeChoice::AutoLaunch(true)),
            YesNoCancel::No => Some(FirstTimeChoice::AutoLaunch(false)),
            _ => None,
        };
        prompts.insert(
            0,
            (
                "Auto Launch Redefaulter on Login?".to_string(),
                auto_launch_prompt,
            ),
        );
    }

    let prompts_count = prompts.len();
    let text = format!(
        r#"Thanks for using Redefaulter! Most controls reside in the System Tray icon.

Would you like to perform first time setup for Redefaulter?

Only {prompts_count} quick questions, and you can Cancel at any time."#,
    );

    let title = format!("Redefaulter setup 0/{prompts_count}");
    let response = win_msgbox::information::<YesNo>(&text)
        .title(&title)
        .show()
        .unwrap();

    match response {
        YesNo::Yes => (),
        YesNo::No => return,
    }

    for (index, (prompt, mapper)) in prompts.into_iter().enumerate() {
        let title = format!("Redefaulter setup {}/{prompts_count}", index + 1);
        let response = win_msgbox::information::<YesNoCancel>(&prompt)
            .title(&title)
            .show()
            .unwrap();
        match response {
            YesNoCancel::Yes | YesNoCancel::No => {
                if let Some(mapped) = mapper(response) {
                    event_proxy.send_event(mapped.into()).unwrap();
                }
            }
            YesNoCancel::Cancel => return,
        };
    }
}
