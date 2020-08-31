mod attach;
mod detach;
mod ls;

use crate::Config;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "gdb";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manages gdb stubs")
        .subcommand(attach::command_definition())
        .subcommand(ls::command_definition())
        .subcommand(detach::command_definition())
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (attach::COMMAND_STR, Some(matches)) => attach::handle_command(conf, matches),
        (ls::COMMAND_STR, Some(matches)) => ls::handle_command(conf, matches),
        (detach::COMMAND_STR, Some(matches)) => detach::handle_command(conf, matches),
        _ => ::std::process::exit(1),
    }
}
