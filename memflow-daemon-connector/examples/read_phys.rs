use std::time::Instant;

use log::{info, Level};

#[macro_use]
extern crate clap;
use clap::{App, Arg};

use memflow::*;

fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .long_version(format!("version: {}", crate_version!()).as_str())
        .author(crate_authors!())
        .about("memflow daemon connector example")
        .after_help(crate_description!())
        .arg(
            Arg::with_name("host")
                .short("h")
                .long("host")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    simple_logger::init_with_level(Level::Debug).unwrap();

    let host = matches.value_of("host").unwrap();
    let args = ConnectorArgs::parse(host).unwrap();
    let mut conn = match memflow_daemon_connector::create_connector(&args) {
        Ok(br) => br,
        Err(e) => {
            info!("couldn't open memory read context: {:?}", e);
            return;
        }
    };

    let mut mem = vec![0; 8];
    conn.phys_read_raw_into(Address::from(0x1000).into(), &mut mem)
        .unwrap();
    info!("Received memory: {:?}", mem);

    let start = Instant::now();
    let mut counter = 0;
    loop {
        let mut buf = vec![0; 0x1000];
        conn.phys_read_raw_into(Address::from(0x1000).into(), &mut buf)
            .unwrap();

        counter += 1;
        if (counter % 10000) == 0 {
            let elapsed = start.elapsed().as_millis() as f64;
            if elapsed > 0.0 {
                info!("{} reads/sec", (f64::from(counter)) / elapsed * 1000.0);
                info!("{} ms/read", elapsed / (f64::from(counter)));
            }
        }
    }
}
