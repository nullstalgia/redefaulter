mod app;
mod panic_handler;
mod platform;
mod processes;
mod profiles;
mod structs;
mod tray_menu;

pub mod args;
pub mod errors;

use std::{path::PathBuf, sync::mpsc, time::Duration};

use args::TopLevelCmd;
use errors::RedefaulterError;
use platform::{AudioNightmare, WindowsAudioDevice};

use color_eyre::eyre::Result;
use profiles::Profiles;

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    // TODO init logging
    let working_directory = determine_working_directory().ok_or(RedefaulterError::WorkDir)?;
    if !working_directory.exists() {
        fs_err::create_dir(&working_directory)?;
    }
    std::env::set_current_dir(&working_directory).expect("Failed to change working directory");

    let mut platform = AudioNightmare::build()?;

    if let Some(subcommand) = args.subcommand {
        match subcommand {
            args::SubCommands::List(categories) => {
                platform.print_devices(categories);
            }
            args::SubCommands::Tui(_) => {}
        }

        return Ok(());
    }

    let profiles = Profiles::build()?;

    println!("{profiles:#?}");
    // println!(
    //     "{:#?}",
    //     platform.device_by_name_fuzzy(
    //         &wasapi::Direction::Render,
    //         "Speakers (PRO X Wireless Gaming Headset)"
    //     )
    // );

    // use std::thread;

    // let (process_tx, process_rx) = mpsc::channel();

    // let thread_join_handle = thread::spawn(move || processes::process_event_loop(process_tx));

    // let mut i = 0;
    // while let Ok(item) = process_rx.recv() {
    //     println!("{item:#?}");

    //     i = i + 1;
    //     if i == 5 {
    //         break;
    //     }
    // }

    // std::mem::drop(process_rx);
    // println!("dropped");
    // let res = thread_join_handle.join();

    // platform.print_devices()?;
    // platform.set_device_test()?;
    // platform.print_one_audio_event()?;

    Ok(())
}

/// Returns the directory that logs, config, and other files should be placed in by default.
// The rules for how it determines the directory is as follows:
// If the app is built with the portable feature, it will just return it's parent directory.
// If there is a config file present adjacent to the executable, the executable's parent path is returned.
// Otherwise, it will return the `directories` `config_dir` output.
//
// Debug builds are always portable. Release builds can optionally have the "portable" feature enabled.
fn determine_working_directory() -> Option<PathBuf> {
    let portable = is_portable();
    let exe_path = std::env::current_exe().expect("Failed to get executable path");
    let exe_parent = exe_path
        .parent()
        .expect("Couldn't get parent dir of executable")
        .to_path_buf();
    let config_path = exe_path.with_extension("toml");

    if portable || config_path.exists() {
        Some(exe_parent)
    } else {
        get_user_dir()
    }
}

#[cfg(any(debug_assertions, feature = "portable"))]
fn is_portable() -> bool {
    true
}

#[cfg(not(any(debug_assertions, feature = "portable")))]
fn is_portable() -> bool {
    false
}

#[cfg(any(debug_assertions, feature = "portable"))]
fn get_user_dir() -> Option<PathBuf> {
    None
}

#[cfg(not(any(debug_assertions, feature = "portable")))]
fn get_user_dir() -> Option<PathBuf> {
    if let Some(base_dirs) = BaseDirs::new() {
        let mut config_dir = base_dirs.config_dir().to_owned();
        config_dir.push(env!("CARGO_PKG_NAME"));
        Some(config_dir)
    } else {
        None
    }
}
