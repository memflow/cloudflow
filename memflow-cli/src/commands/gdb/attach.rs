use crate::dispatch::*;
use crate::Config;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "attach";

const CONNECTION_ID: &str = "CONNECTION_ID";
const PROCESS_ID: &str = "PROCESS_ID";
const ADDRESS: &str = "ADDRESS";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("spawns a new gdb stub and attaches it to a process")
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connection id of the process to be attached")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name(PROCESS_ID)
                .help("the process id of the process to be attached")
                .index(2)
                .required(true),
        )
        .arg(
            Arg::with_name(ADDRESS)
                .help("the address of the gdb stub")
                .index(3)
                .required(true),
        )
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();
    let pid = matches.value_of(PROCESS_ID).unwrap();
    let addr = matches.value_of(ADDRESS).unwrap();

    dispatch_request(
        conf,
        request::Message::GDBAttach(request::GDBAttach {
            conn_id: conn_id.to_string(),
            pid: pid.to_string(),
            addr: addr.to_string(),
        }),
    )
    .unwrap();
}
