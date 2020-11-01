mod ls;
mod mount;

use crate::Config;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "fuse";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("manages fuse virtual filesystem mount")
        .subcommand(mount::command_definition())
        .subcommand(ls::command_definition())
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    match matches.subcommand() {
        (mount::COMMAND_STR, Some(matches)) => mount::handle_command(conf, matches),
        (ls::COMMAND_STR, Some(matches)) => ls::handle_command(conf, matches),
        _ => {
            command_definition().print_help().ok();
            println!();
            ::std::process::exit(1)
        }
    }
}
