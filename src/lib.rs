pub mod args;

mod platform;
mod structs;
mod tray_menu;

use args::TopLevelCmd;
use platform::AudioNightmare;

use color_eyre::eyre::Result;

pub fn run(args: TopLevelCmd) -> Result<()> {
    let platform = AudioNightmare {};

    platform.init()?;

    platform.print_devices()?;

    platform.deinit()?;

    Ok(())
}
