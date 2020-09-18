mod ls;
mod new;
mod rm;

use crate::Config;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "conn";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manages machine connections")
        .subcommand(new::command_definition())
        .subcommand(ls::command_definition())
        .subcommand(rm::command_definition())
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (new::COMMAND_STR, Some(matches)) => new::handle_command(conf, matches),
        (ls::COMMAND_STR, Some(matches)) => ls::handle_command(conf, matches),
        (rm::COMMAND_STR, Some(matches)) => rm::handle_command(conf, matches),
        _ => {
            command_definition().print_help().ok();
            println!();
            ::std::process::exit(1)
        }
    }
}
