use crate::dispatch::*;
use crate::error::Result;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use flow_daemon::request;

pub const COMMAND_STR: &str = "ls";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR).about("lists all open connections")
}

pub fn handle_command(_matches: &ArgMatches) {
    trace!("handling command");

    dispatch_request(
        request::Message::ListConnections(request::ListConnections {}),
        |msg| -> Result<()> {
            println!("stuff: {:?}", msg);
            Ok(()) // continue
        },
    )
    .ok();
}
