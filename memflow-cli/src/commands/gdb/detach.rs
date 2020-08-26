use crate::dispatch::*;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "detach";

const GDB_ID: &str = "GDB_ID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("detaches a gdb stub")
        .arg(
            Arg::with_name(GDB_ID)
                .help("the id of the gdb stub to detach")
                .index(1)
                .required(true),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let gdb_id = matches.value_of(GDB_ID).unwrap();

    dispatch_request(request::Message::GDBDetach(request::GDBDetach {
        gdb_id: gdb_id.to_string(),
    }))
    .unwrap();
}
