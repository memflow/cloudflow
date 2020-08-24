use crate::dispatch::*;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "rm";

const CONNECTION_ID: &str = "CONNECTION_ID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connector to be used for the new connection")
                .index(1)
                .required(true),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();

    dispatch_request(request::Message::CloseConnection(
        request::CloseConnection {
            conn_id: conn_id.to_string(),
        },
    ))
    .unwrap();
}
