mod ls;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "proc";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manage processes")
        .subcommand(ls::command_definition())
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (ls::COMMAND_STR, Some(matches)) => ls::handle_command(matches),
        _ => {
            //term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    }
}
