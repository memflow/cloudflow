use crate::Config;

use clap::{App, ArgMatches, SubCommand};

use log::{error, trace};

use memflow_client::dispatch::dispatch_request;
use memflow_daemon::memflow_rpc::GdbListRequest;

pub const COMMAND_STR: &str = "ls";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR).about("lists all gdb stubs")
}

pub fn handle_command(conf: &Config, _matches: &ArgMatches) {
    trace!("handling command");

    let result = dispatch_request(conf, GdbListRequest {});

    match result {
        Err(e) => error!("{:#?}", e),
        Ok(r) => println!("{:#?}", r.stubs),
    }
}
