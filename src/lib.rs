mod app;
mod panic_handler;
mod platform;
mod processes;
mod profiles;
mod structs;
mod tray_menu;

pub mod args;
pub mod errors;

use std::{sync::mpsc, time::Duration};

use args::TopLevelCmd;
use platform::AudioNightmare;

use color_eyre::eyre::Result;

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    let mut platform = AudioNightmare::build()?;

    use std::thread;

    let (process_tx, process_rx) = mpsc::channel();

    let thread_join_handle = thread::spawn(move || processes::process_event_loop(process_tx));

    let mut i = 0;
    while let Ok(item) = process_rx.recv() {
        println!("{item:#?}");

        i = i + 1;
        if i == 5 {
            break;
        }
    }

    std::mem::drop(process_rx);
    println!("dropped");
    let res = thread_join_handle.join();

    // platform.print_devices()?;
    // platform.set_device_test()?;
    // platform.print_one_audio_event()?;

    Ok(())
}
