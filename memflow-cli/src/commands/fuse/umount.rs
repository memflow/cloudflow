use crate::dispatch::*;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "umount";

const FUSE_ID: &str = "FUSE_ID";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(FUSE_ID)
                .help("the id of the fuse filesystem to umount")
                .index(1)
                .required(true),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let fuse_id = matches.value_of(FUSE_ID).unwrap();

    dispatch_request(request::Message::FuseUmount(request::FuseUmount {
        fuse_id: fuse_id.to_string(),
    }))
    .unwrap();
}
