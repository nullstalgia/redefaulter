use argh::FromArgs;

#[derive(FromArgs, PartialEq, Debug)]
/// Top-level command.
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
/// Get a list of
#[argh(subcommand, name = "list")]
pub struct ListSubcommand {
    #[argh(switch, short = 'p')]
    /// show playback devices
    pub playback: bool,
    #[argh(switch, short = 'r')]
    /// show recording devices
    pub recording: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
/// Allow configuration with a TUI
#[argh(subcommand, name = "tui")]
pub struct Tui {
    // #[argh(switch)]
    // /// whether to fooey
    // fooey: bool,
}
