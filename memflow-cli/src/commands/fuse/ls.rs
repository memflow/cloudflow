use crate::dispatch::*;

use clap::{App, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "ls";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR).about("lists all mounted file systems")
}

pub fn handle_command(_matches: &ArgMatches) {
    trace!("handling command");

    dispatch_request(request::Message::FuseListMounts).unwrap();
}
