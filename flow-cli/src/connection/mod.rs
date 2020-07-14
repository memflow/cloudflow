mod open;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &'static str = "connection";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manages machine connections")
        .subcommand(open::command_definition())
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (open::COMMAND_STR, Some(matches)) => open::handle_command(matches),
        _ => {
            //term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    }
}
