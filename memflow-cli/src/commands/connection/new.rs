use crate::Config;
use memflow_client::dispatch::dispatch_request;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::{error, trace};

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

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let name = matches.value_of(CONNECTOR_NAME).unwrap();
    let args = matches.value_of(CONNECTOR_ARGS);
    let alias = matches.value_of(CONNECTOR_ALIAS);

    let result = dispatch_request(
        conf,
        memflow_daemon::memflow_rpc::NewConnectionRequest {
            name: name.to_string(),
            args: args.unwrap_or_default().to_string(),
            alias: alias.unwrap_or_default().to_string(),
        },
    );

    match result {
        Err(e) => error!("{:#?}", e),
        Ok(r) => println!("New connection id: {}", r.conn_id),
    }
}
