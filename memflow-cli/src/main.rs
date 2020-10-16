mod error;

mod commands;
mod dispatch;

#[macro_use]
extern crate clap;
use clap::{App, Arg};

use log::Level;

pub struct Config {
    pub host: String,
}

fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .long_version(format!("version: {}", crate_version!()).as_str())
        .author(crate_authors!())
        .about("memflow command line interface")
        .after_help(crate_description!())
        .arg(
            Arg::with_name("host")
                .short("h")
                .long("host")
                .takes_value(true)
                .required(false)
                .default_value("unix:/var/run/memflow.sock"),
        )
        .subcommand(commands::connection::command_definition())
        .subcommand(commands::fuse::command_definition())
        .subcommand(commands::proc::command_definition())
        .subcommand(commands::gdb::command_definition())
        .get_matches();

    simple_logger::SimpleLogger::new()
        .with_level(Level::Debug.to_level_filter())
        .init()
        .unwrap();

    let host = matches.value_of("host").unwrap().to_string();
    let conf = Config { host };

    match matches.subcommand() {
        (commands::connection::COMMAND_STR, Some(subargv)) => {
            commands::connection::handle_command(&conf, subargv)
        }
        (commands::fuse::COMMAND_STR, Some(subargv)) => {
            commands::fuse::handle_command(&conf, subargv)
        }
        (commands::proc::COMMAND_STR, Some(subargv)) => {
            commands::proc::handle_command(&conf, subargv)
        }
        (commands::gdb::COMMAND_STR, Some(subargv)) => {
            commands::gdb::handle_command(&conf, subargv)
        }
        _ => ::std::process::exit(1),
    }
}
