mod app;
mod panic_handler;
mod platform;
mod profiles;
mod structs;
mod tray_menu;

pub mod args;
pub mod errors;

use args::TopLevelCmd;
use platform::AudioNightmare;

use color_eyre::eyre::Result;

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    let mut platform = AudioNightmare::build()?;

    // platform.print_devices()?;
    // platform.set_device_test()?;
    // platform.print_one_audio_event()?;

    Ok(())
}
