use crate::dispatch::*;
use crate::Config;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;
use std::fs;

use memflow_daemon::request;

pub const COMMAND_STR: &str = "mount";

const CONNECTION_ID: &str = "CONNECTION_ID";
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

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();
    let mount_point = matches.value_of(MOUNT_POINT).unwrap();

    // TODO: give meaningful error messages here
    let canonical_path = fs::canonicalize(mount_point).unwrap();
    let full_path = canonical_path.to_str().unwrap();

    dispatch_request(
        conf,
        request::Message::FuseMount(request::FuseMount {
            conn_id: conn_id.to_string(),
            mount_point: full_path.to_string(),
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
        }),
    )
    .unwrap();
}
