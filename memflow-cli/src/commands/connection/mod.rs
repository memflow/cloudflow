mod ls;
mod new;
mod rm;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "connection";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manages machine connections")
        .subcommand(new::command_definition())
        .subcommand(ls::command_definition())
        .subcommand(rm::command_definition())
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (new::COMMAND_STR, Some(matches)) => new::handle_command(matches),
        (ls::COMMAND_STR, Some(matches)) => ls::handle_command(matches),
        (rm::COMMAND_STR, Some(matches)) => rm::handle_command(matches),
        _ => {
            //term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    }
}
