mod error;

mod commands;
mod dispatch;

#[macro_use]
extern crate clap;
use clap::{App, Arg};

use log::debug;
use log::Level;

#[cfg(not(target_os = "windows"))]
const CONFIG_FILE: &str = "/etc/memflow/daemon.conf";
#[cfg(target_os = "windows")]
const CONFIG_FILE: &str = "daemon.conf";

pub struct Config {
    pub host: String,
}

fn main() {
    let long_version = format!("version: {}", crate_version!());
    let mut app = App::new(crate_name!())
        .version(crate_version!())
        .long_version(long_version.as_str())
        .author(crate_authors!())
        .about("memflow command line interface")
        .after_help(crate_description!())
        .arg(
            Arg::with_name("host")
                .short("H")
                .long("host")
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .help("the config file path")
                .takes_value(true)
                .required(false)
                .default_value(CONFIG_FILE),
        )
        .subcommand(commands::connection::command_definition())
        .subcommand(commands::proc::command_definition())
        .subcommand(commands::gdb::command_definition());
    #[cfg(not(target_os = "windows"))]
    app.subcommand(commands::fuse::command_definition());

    let matches = app.clone().get_matches();

    simple_logger::SimpleLogger::new()
        .with_level(Level::Debug.to_level_filter())
        .init()
        .unwrap();

    // we first check the 'host' argument
    // in case 'host' is empty we check the 'config' argument
    let conf = if let Some(host) = matches.value_of("host") {
        Config {
            host: host.to_string(),
        }
    } else {
        let config_path = matches.value_of("config").unwrap();
        debug!("loading host from configuration file: {}", config_path);
        let config_str = std::fs::read_to_string(config_path).unwrap();
        let config: memflow_daemon::Config = serde_json::from_str(&config_str).unwrap();
        Config {
            host: config.socket_addr,
        }
    };
    debug!("memflow host: {}", conf.host);

    match matches.subcommand() {
        (commands::connection::COMMAND_STR, Some(subargv)) => {
            commands::connection::handle_command(&conf, subargv)
        }
        #[cfg(not(target_os = "windows"))]
        (commands::fuse::COMMAND_STR, Some(subargv)) => {
            commands::fuse::handle_command(&conf, subargv)
        }
        (commands::proc::COMMAND_STR, Some(subargv)) => {
            commands::proc::handle_command(&conf, subargv)
        }
        (commands::gdb::COMMAND_STR, Some(subargv)) => {
            commands::gdb::handle_command(&conf, subargv)
        }
        _ => {
            app.print_help().ok();
            println!();
            ::std::process::exit(1);
        }
    }
}
