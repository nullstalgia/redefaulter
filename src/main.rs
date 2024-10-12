use color_eyre::eyre::Result;
use redefaulter::{args::TopLevelCmd, run};

// load config
// get current audio devices
// check against devices and change any outliers if possible
// check for violation of unify comms
// set up windows callback channels
// event loop listens for changes in audio devices (connect/disconnect/default change)
// event loop listens for specified processes opening/closing (debounce?)
// regenerate menu each time? or on click...
fn main() -> Result<()> {
    let args: TopLevelCmd = argh::from_env();
    run(args)?;

    Ok(())
}
