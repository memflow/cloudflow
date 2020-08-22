mod error;

mod connection;
mod dispatch;

#[macro_use]
extern crate clap;
use clap::App;

use log::Level;

fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .long_version(format!("version: {}", crate_version!()).as_str())
        .author(crate_authors!())
        .about("memflow command line interface")
        .after_help(crate_description!())
        .subcommand(connection::command_definition())
        .get_matches();

    /*
    match matches.occurrences_of("verbose") {
        1 => simple_logger::init_with_level(Level::Warn).unwrap(),
        2 => simple_logger::init_with_level(Level::Info).unwrap(),
        3 => simple_logger::init_with_level(Level::Debug).unwrap(),
        4 => simple_logger::init_with_level(Level::Trace).unwrap(),
        _ => simple_logger::init_with_level(Level::Error).unwrap(),
    }
    */
    simple_logger::init_with_level(Level::Debug).unwrap();

    match matches.subcommand() {
        (connection::COMMAND_STR, Some(subargv)) => connection::handle_command(subargv),
        _ => {
            // term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    }
}
