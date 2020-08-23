use crate::dispatch::*;
use crate::error::Result;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "mount";

const CONNECTION_ID: &str = "CONNECTOR_ID";
const MOUNT_POINT: &str = "MOUNT_POINT";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about("opens up a new connection to a machine")
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connection id to be mounted")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name(MOUNT_POINT)
                .help("the target mount point of the filesystem")
                .index(2)
                .required(true),
        )
}

pub fn handle_command(matches: &ArgMatches) {
    trace!("handling command");

    let id = matches.value_of(CONNECTION_ID).unwrap();
    let mount_point = matches.value_of(MOUNT_POINT).unwrap();

    dispatch_request(request::Message::FuseMount(request::FuseMount {
        id: id.to_string(),
        mount_point: mount_point.to_string(),
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
    }))
    .unwrap();
}
