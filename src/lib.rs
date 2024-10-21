pub mod args;

mod panic_handler;
mod platform;
mod structs;
mod tray_menu;

pub mod errors;

use args::TopLevelCmd;
use platform::AudioNightmare;

use color_eyre::eyre::Result;

pub fn run(args: TopLevelCmd) -> Result<()> {
    panic_handler::initialize_panic_handler()?;
    let mut platform = AudioNightmare::build().unwrap();

    platform.print_devices()?;
    platform.set_device_test()?;

    Ok(())
}
