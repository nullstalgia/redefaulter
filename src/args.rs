use argh::FromArgs;

// TODO Command for checking overrides once then exiting

#[derive(FromArgs, PartialEq, Debug)]
/// Command-line actions with Redefaulter
pub struct TopLevelCmd {
    #[argh(subcommand)]
    pub subcommand: Option<SubCommands>,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum SubCommands {
    List(ListSubcommand),
    Tui(Tui),
}

#[derive(FromArgs, PartialEq, Debug)]
/// Get list of audio devices and their GUIDs
#[argh(subcommand, name = "list")]
pub struct ListSubcommand {
    #[argh(switch, short = 'p')]
    /// show playback devices
    pub playback: bool,
    #[argh(switch, short = 'r')]
    /// show recording devices
    pub recording: bool,
    #[argh(switch, short = 's')]
    /// print devices in the format used in profiles
    pub profile_format: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
/// Allow configuration with a TUI
#[argh(subcommand, name = "tui")]
pub struct Tui {
    // #[argh(switch)]
    // /// whether to fooey
    // fooey: bool,
}
