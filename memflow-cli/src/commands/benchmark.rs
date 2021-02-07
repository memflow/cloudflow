use crate::Config;
use memflow_client::dispatch::benchmark;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::trace;

pub const COMMAND_STR: &str = "benchmark";

const CONNECTION_ID: &str = "CONNECTION_ID";

const READ_SIZE: &str = "READ_SIZE";

const ASYNC_MODE: &str = "ASYNC_MODE";

const PHYSICAL_MODE: &str = "PHYSICAL_MODE";

pub fn command_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(COMMAND_STR)
        .about(
            "benchmark physical and virtual memory access in asynchronous mode or synchronous mode",
        )
        .arg(
            Arg::with_name(CONNECTION_ID)
                .help("the connector to be used for process listing")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name(READ_SIZE)
                .help("read size per operation")
                .index(2)
                .required(false)
                .default_value("64"),
        )
        .arg(
            Arg::with_name(ASYNC_MODE)
                .help("asynchronous mode")
                .index(3)
                .required(false)
                .default_value("true"),
        )
        .arg(
            Arg::with_name(PHYSICAL_MODE)
                .help("physical mode")
                .index(4)
                .required(false)
                .default_value("true"),
        )
}

pub fn handle_command(conf: &Config, matches: &ArgMatches) {
    trace!("handling command");

    let conn_id = matches.value_of(CONNECTION_ID).unwrap();
    let read_size: u64 = matches
        .value_of(READ_SIZE)
        .unwrap_or("64")
        .parse()
        .expect("integer parse failed, read size must be u64 value");
    let async_mode: bool = matches
        .value_of(ASYNC_MODE)
        .unwrap_or("true")
        .parse()
        .expect("bool parse failed, async must be true or false");
    let physical_mode: bool = matches
        .value_of(PHYSICAL_MODE)
        .unwrap_or("true")
        .parse()
        .expect("bool parse failed, pysical must be true or false");

    benchmark(conf, physical_mode, conn_id, read_size, async_mode)
}
