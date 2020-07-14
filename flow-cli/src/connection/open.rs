use crate::dispatch::*;
use crate::error::Result;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use flow_daemon::request;

pub const COMMAND_STR: &'static str = "open";

const CONNECTOR_NAME: &'static str = "CONNECTOR_NAME";
const CONNECTOR_ARGS: &'static str = "CONNECTOR_ARGS";

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
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let name = matches.value_of(CONNECTOR_NAME).unwrap();
    let arg = matches.value_of(CONNECTOR_ARGS);

    dispatch_request(
        request::Message::Connect(request::Connect {
            name: name.to_string(),
            arg: None, // TODO:
        }),
        |msg| -> Result<()> {
            println!("stuff: {:?}", msg);
            Ok(()) // continue
        },
    )
    .ok();
}
