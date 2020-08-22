use crate::dispatch::*;
use crate::error::Result;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use flow_daemon::request;

pub const COMMAND_STR: &str = "rm";

const CONNECTOR_ID: &str = "CONNECTOR_ID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(CONNECTOR_ID)
                .help("the connector to be used for the new connection")
                .index(1)
                .required(true),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let id = matches.value_of(CONNECTOR_ID).unwrap();

    dispatch_request(request::Message::CloseConnection(
        request::CloseConnection { id: id.to_string() },
    ))
    .unwrap();
}
