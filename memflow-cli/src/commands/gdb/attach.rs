use crate::Config;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::{error, trace};

use memflow_client::dispatch::dispatch_request;
use memflow_daemon::memflow_rpc::GdbAttachRequest;

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
    let pid: u32 = matches
        .value_of(PROCESS_ID)
        .unwrap()
        .parse()
        .expect("integer parse failed, read size must be u32 value");
    let addr = matches.value_of(ADDRESS).unwrap();

    let result = dispatch_request(
        conf,
        GdbAttachRequest {
            conn_id: conn_id.to_string(),
            pid: pid,
            addr: addr.to_string(),
        },
    );

    match result {
        Err(e) => error!("{:#?}", e),
        Ok(r) => {
            println!("gdb stub with id {} spawned at address {}", r.id, addr);
            println!("the gdb stub will automatically be closed on disconnect");
        }
    };
}
