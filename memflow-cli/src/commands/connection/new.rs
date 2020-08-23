use crate::dispatch::*;
use crate::error::Result;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "new";

const CONNECTOR_NAME: &str = "CONNECTOR_NAME";
const CONNECTOR_ARGS: &str = "CONNECTOR_ARGS";
const CONNECTOR_ALIAS: &str = "CONNECTOR_ALIAS";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(CONNECTOR_NAME)
                .help("the connector to be used for the new connection")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name(CONNECTOR_ARGS)
                .help("additional arguments to be fed into the connector")
                .index(2)
                .required(false),
        )
        .arg(
            Arg::with_name(CONNECTOR_ALIAS)
                .help("alias for the connection")
                .long("alias")
                .short("a")
                .takes_value(true)
                .required(false),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let name = matches.value_of(CONNECTOR_NAME).unwrap();
    let args = matches.value_of(CONNECTOR_ARGS);
    let alias = matches.value_of(CONNECTOR_ALIAS);

    dispatch_request(request::Message::Connect(request::Connect {
        name: name.to_string(),
        args: args.map(|s| s.to_string()),
        alias: alias.map(|a| a.to_string()),
    }))
    .unwrap();
}
