use argh::FromArgs;

#[derive(FromArgs, PartialEq, Debug)]
/// Top-level command.
pub struct TopLevelCmd {
    #[argh(subcommand)]
    nested: Option<SubCommands>,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SubCommands {
    List(ListSubcommand),
    Tui(Tui),
}

#[derive(FromArgs, PartialEq, Debug)]
/// Get a list of
#[argh(subcommand, name = "list")]
struct ListSubcommand {
    // #[argh(option)]
    // /// how many x
    // x: usize,
}

#[derive(FromArgs, PartialEq, Debug)]
/// Allow configuration with a TUI
#[argh(subcommand, name = "tui")]
struct Tui {
    // #[argh(switch)]
    // /// whether to fooey
    // fooey: bool,
}
