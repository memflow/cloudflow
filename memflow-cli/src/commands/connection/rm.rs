use crate::Config;
use memflow_client::dispatch::dispatch_request;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::{error, trace};

pub const COMMAND_STR: &str = "rm";

const CONNECTION_ID: &str = "CONNECTION_ID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connector to be removed")
                .index(1)
                .required(true),
        )
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();

    let result = dispatch_request(
        conf,
        memflow_daemon::memflow_rpc::CloseConnectionRequest {
            conn_id: conn_id.to_string(),
        },
    );

    match result {
        Err(e) => error!("{:#?}", e),
        Ok(_) => println!("Connection closed"),
    }
}
