#![deny(unused_must_use)]

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

use app::{App, CustomEvent};
use args::TopLevelCmd;
use errors::RedefaulterError;
use platform::AudioNightmare;
use std::path::PathBuf;
use std::time::Instant;
use tao::event::StartCause;

use color_eyre::eyre::Result;

use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use tracing::{debug, error, info};
use tracing_subscriber::{filter, prelude::*};
use tracing_subscriber::{fmt::time::ChronoLocal, layer::SubscriberExt, util::SubscriberInitExt};

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
};

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    let working_directory = determine_working_directory().ok_or(RedefaulterError::WorkDir)?;
    if !working_directory.exists() {
        fs_err::create_dir(&working_directory)?;
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
        .with_filter(filter::LevelFilter::TRACE);
    let fmt_layer_stdout = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_stdout)
        .with_file(false)
        .with_target(true)
        .with_timer(time_fmt)
        .with_line_number(true)
        .with_filter(filter::LevelFilter::TRACE);
    let (fmt_layer_file, _reload_handle_file) =
        tracing_subscriber::reload::Layer::new(fmt_layer_file);
    let (fmt_layer_stdout, _reload_handle_stdout) =
        tracing_subscriber::reload::Layer::new(fmt_layer_stdout);
    let env_filter = tracing_subscriber::EnvFilter::new("trace");
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer_file)
        .with(fmt_layer_stdout)
        .init();

    if let Some(subcommand) = args.subcommand {
        match subcommand {
            args::SubCommands::List(categories) => {
                let platform = AudioNightmare::build(None, None)?;
                platform.print_devices(categories);
                return Ok(());
            }
            args::SubCommands::Tui(_) => todo!(),
        }
    }

    let event_loop = EventLoopBuilder::<CustomEvent>::with_user_event().build();
    let event_proxy = event_loop.create_proxy();

    info!("Starting app... v{}", env!("CARGO_PKG_VERSION"));

    let mut app = App::build(event_proxy)?;

    // Starting off at DEBUG, and setting to whatever user has defined
    // reload_handle.modify(|layer| *layer.filter_mut() = app.settings.get_log_level())?;

    // println!("{:#?}", app.generate_device_actions());

    // println!("{:#?}", app.processes);

    event_loop.run(move |event, _, control_flow| match event {
        Event::NewEvents(StartCause::Init) => {
            *control_flow = ControlFlow::Wait;
            app.change_devices_if_needed().unwrap();
        }
        Event::UserEvent(event) => {
            // println!("user event: {event:?}");
            let instant_1 = Instant::now();
            if let Err(e) = app.handle_custom_event(event, control_flow) {
                error!("Error in event loop, closing. {e}");
                *control_flow = ControlFlow::ExitWithCode(1);
            };
            let instant_2 = Instant::now();
            debug!("Event handling took {:?}", instant_2 - instant_1);
        }
        // Timeout for an audio device reaction finished waiting
        Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            debug!("Done waiting for audio endpoint timeout!");
            app.update_defaults().unwrap();
            app.change_devices_if_needed().unwrap();
            *control_flow = ControlFlow::Wait;
        }
        Event::NewEvents(StartCause::WaitCancelled {
            requested_resume, ..
        }) => {
            // We had a wait time, but something else came in before we could finish waiting,
            // so just check now
            if requested_resume.is_some() {
                app.update_defaults().unwrap();
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => *control_flow = ControlFlow::Exit,
        Event::LoopDestroyed => (),
        _ => (),
    });

    // Ok(())
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
