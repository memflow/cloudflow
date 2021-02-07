use std::time::Instant;

use log::{info, Level};

#[macro_use]
extern crate clap;
use clap::{App, Arg};

use memflow::*;
use memflow_win32::*;

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
        .arg(
            Arg::with_name("id")
                .short("id")
                .long("id")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    simple_logger::SimpleLogger::new()
        .with_level(Level::Info.to_level_filter())
        .init()
        .unwrap();

    let host = matches.value_of("host").unwrap();
    let id = matches.value_of("id").unwrap();
    let args = ConnectorArgs::parse(&(host.to_owned() + ",id=" + id + ",host=" + host)).unwrap();
    println!("{:#?}", args);
    let mut conn = match memflow_daemon_connector::create_connector(&args) {
        Ok(br) => br,
        Err(e) => {
            info!("couldn't open memory read context: {:?}", e);
            return;
        }
    };
    let mut kernel = win32::Kernel::builder(conn.clone()).build_default_caches().build().unwrap();
    let mut proc = kernel.process("explorer.exe").expect("Could not open explorer.exe process");
    let address = proc.proc_info.section_base;

    let mut mem = vec![0; 8];
    proc.virt_mem.virt_read_raw_into(address, &mut mem)
        .unwrap();
    info!("Received memory: {:?}", mem);

    let start = Instant::now();
    let mut counter = 0;
    loop {
        let mut buf = vec![0; 0x1000];
        proc.virt_mem.virt_read_raw_into(address, &mut buf)
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
