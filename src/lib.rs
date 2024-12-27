#![deny(unused_must_use)]

mod app;
mod panic_handler;
mod platform;
mod popups;
mod processes;
mod profiles;
mod settings;
mod structs;
mod tray_menu;
mod updates;

pub mod args;
pub mod errors;

use app::{App, CustomEvent};
use args::TopLevelCmd;
use errors::RedefaulterError;
use fs_err::{self as fs};
use platform::AudioNightmare;
use popups::fatal_error_popup;

use std::path::PathBuf;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

use color_eyre::eyre::Result;

use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use tracing::*;
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{fmt::time::ChronoLocal, layer::SubscriberExt, util::SubscriberInitExt};

use tao::event_loop::EventLoopBuilder;

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    let ansi_support = enable_ansi_support::enable_ansi_support().is_ok();
    let working_directory = determine_working_directory().ok_or(RedefaulterError::WorkDir)?;
    if !working_directory.exists() {
        fs::create_dir(&working_directory)?;
    }
    std::env::set_current_dir(&working_directory).expect("Failed to change working directory");
    let log_name = std::env::current_exe()?
        .with_extension("log")
        .file_name()
        .expect("Couldn't build log path!")
        .to_owned();
    // let console = console_subscriber::spawn();
    let file_appender = BasicRollingFileAppender::new(
        log_name,
        RollingConditionBasic::new().max_size(1024 * 1024 * 5),
        2,
    )
    .unwrap();
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);
    let (non_blocking_stdout, _stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    let time_fmt = ChronoLocal::new("%Y-%m-%d %H:%M:%S%.6f".to_owned());
    let fmt_layer_file = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_file)
        .with_file(false)
        .with_ansi(false)
        .with_target(true)
        .with_timer(time_fmt.clone())
        .with_line_number(true)
        .with_filter(filter::LevelFilter::DEBUG);
    let fmt_layer_stdout = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_stdout)
        .with_file(false)
        .with_ansi(ansi_support)
        .with_target(true)
        .with_timer(time_fmt)
        .with_line_number(true)
        .with_filter(filter::LevelFilter::DEBUG);
    let (fmt_layer_file, reload_handle_file) =
        tracing_subscriber::reload::Layer::new(fmt_layer_file);
    let (fmt_layer_stdout, reload_handle_stdout) =
        tracing_subscriber::reload::Layer::new(fmt_layer_stdout);
    let env_filter = tracing_subscriber::EnvFilter::new("trace");
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer_file)
        .with(fmt_layer_stdout)
        .init();

    // TODO Command to print running process the way WMI sees them?
    if let Some(subcommand) = args.subcommand {
        match subcommand {
            args::SubCommands::List(categories) => {
                let platform = AudioNightmare::build(None, None)?;
                platform.print_devices(&categories);
                return Ok(());
            }
            args::SubCommands::Tui(_) => todo!(),
        }
    }

    let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();

    panic_handler::initialize_ctrl_c_handler(event_proxy.clone())?;

    info!("Starting app... v{}", env!("CARGO_PKG_VERSION"));

    // Might need to catch more than just App::build's errors, but this is good enough for now.
    let mut app = match App::build(event_proxy) {
        Ok(app) => app,
        Err(e) => {
            error!("Failed to build App: {e}");
            fatal_error_popup(e, None);
        }
    };

    // The only event we really care to have our own reaction for is
    // middle-clicking the tray icon in order to open the "Sounds" menu.
    // If we need to do more, then I'll expand this.
    #[cfg(windows)]
    TrayIconEvent::set_event_handler(Some(|event| {
        // debug!("Tray Event: {event:?}");

        // On middle-click, open the device selection menu, called "Sounds" by newer
        // versions of Windows.
        if let TrayIconEvent::Click {
            button: MouseButton::Middle,
            button_state: MouseButtonState::Down,
            ..
        } = event
        {
            let spawn_result = std::process::Command::new("control.exe")
                .arg("mmsys.cpl")
                .spawn();

            if let Err(e) = spawn_result {
                eprintln!("Failed to open Sound settings menu: {}", e);
            }
        }
    }));

    let menu_channel = MenuEvent::receiver();
    // Starting off at DEBUG, and setting to whatever user has defined
    reload_handle_file.modify(|layer| *layer.filter_mut() = app.settings.get_log_level())?;
    reload_handle_stdout.modify(|layer| *layer.filter_mut() = app.settings.get_log_level())?;

    event_loop.run(move |event, _, control_flow| {
        if let Err(e) = app.handle_tao_event(event, control_flow, menu_channel) {
            error!("Fatal error! {e}");
            // If we get an error, try to gracefully hide the tray icon and go back to normal default devices.
            _ = app.kill_tray_menu();
            _ = app.back_to_default();
            fatal_error_popup(e, Some(app.lock_file.take()));
        };
    });
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
    if let Some(base_dirs) = directories::BaseDirs::new() {
        let mut config_dir = base_dirs.config_dir().to_owned();
        config_dir.push(env!("CARGO_PKG_NAME"));
        Some(config_dir)
    } else {
        None
    }
}
