// Hides CMD popup when running on Release + Windows builds
// Side effect: Will not show console output, must rely on file logs
// (or comment out temporarily).
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use color_eyre::eyre::Result;
use redefaulter::{args::TopLevelCmd, run};

fn main() -> Result<()> {
    let args: TopLevelCmd = argh::from_env();
    run(args)?;

    Ok(())
}
