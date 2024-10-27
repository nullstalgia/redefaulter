mod app;
mod panic_handler;
mod platform;
mod processes;
mod profiles;
mod settings;
mod structs;
mod tray_menu;

pub mod args;
pub mod errors;

use app::App;
use args::TopLevelCmd;
use dashmap::DashMap;
use errors::RedefaulterError;
use platform::AudioNightmare;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

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

    if let Some(subcommand) = args.subcommand {
        match subcommand {
            args::SubCommands::List(categories) => {
                let platform = AudioNightmare::build()?;
                platform.print_devices(categories);
                return Ok(());
            }
            args::SubCommands::Tui(_) => todo!(),
        }
    }

    let app = App::build()?;

    let instant_1 = Instant::now();
    println!("{:#?}", app.test());
    let instant_2 = Instant::now();

    println!("{:?}", instant_2 - instant_1);
    // println!("{:#?}", app.processes);

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
