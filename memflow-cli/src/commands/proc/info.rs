use crate::Config;
use memflow_client::dispatch::dispatch_request;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::{error, trace};

pub const COMMAND_STR: &str = "info";

const CONNECTION_ID: &str = "CONNECTION_ID";

const PID: &str = "PID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("get process info")
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connector to be used for process info")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name(PID)
                .help("pid of process to gather information")
                .index(2)
                .required(true),
        )
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();
    let pid = matches.value_of(PID).unwrap();

    let result = dispatch_request(
        conf,
        memflow_daemon::memflow_rpc::ProcessInfoRequest {
            conn_id: conn_id.to_string(),
            pid: pid
                .parse()
                .expect("integer parse failed, pid must be u32 value"),
        },
    );

    match result {
        Err(e) => error!("{:#?}", e),
        Ok(r) => println!("{:#?}", r.process),
    }
}
