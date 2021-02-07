use crate::Config;

use clap::{App, Arg, ArgMatches, SubCommand};

use log::{error, trace};
use memflow_client::dispatch::{
    create_client, create_client_async, dispatch_request, dispatch_request_async_client,
    dispatch_request_client,
};

use memflow_daemon::memflow_rpc::{
    ProcessInfoRequest, ReadPhysicalMemoryEntryRequest, ReadPhysicalMemoryRequest,
    ReadVirtualMemoryEntryRequest, ReadVirtualMemoryRequest,
};

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

pub fn benchmark(
    conf: &Config,
    physical_mode: bool,
    conn_id: &str,
    read_size: u64,
    async_mode: bool,
) {
    if async_mode {
        benchmark_async(conf, physical_mode, conn_id, read_size);
    } else {
        benchmark_sync(conf, physical_mode, conn_id, read_size);
    }
}

fn benchmark_sync(conf: &Config, physical_mode: bool, conn_id: &str, read_size: u64) {
    let pid = 0;
    let address = dispatch_request(
        conf,
        ProcessInfoRequest {
            conn_id: conn_id.to_string(),
            pid: pid,
        },
    )
    .expect("could not access process info")
    .process
    .unwrap()
    .address;

    let entry = ReadVirtualMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let req = ReadVirtualMemoryRequest {
        conn_id: conn_id.to_string(),
        pid: pid,
        base_offsets: false,
        reads: vec![entry],
    };
    let phys_entry = ReadPhysicalMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let phys_req = ReadPhysicalMemoryRequest {
        conn_id: conn_id.to_string(),
        reads: vec![phys_entry],
    };

    let (mut client, rt) = create_client(conf);

    let start_time = std::time::Instant::now();
    let mut total_runs = 0;
    loop {
        total_runs += 1;

        let response = if !physical_mode {
            dispatch_request_client(conf, req.clone(), &mut client, &rt).map(|_| ())
        } else {
            dispatch_request_client(conf, phys_req.clone(), &mut client, &rt).map(|_| ())
        };
        match response {
            Err(e) => error!("{:#?}", e),
            Ok(_) => (),
        }

        if (std::time::Instant::now() - start_time).as_secs() > 10 {
            break;
        }
    }
    let end_time = std::time::Instant::now();

    let total_sec = (end_time - start_time).as_secs_f64();
    println!(
        "Total: {} s, Total: {}, Each: {} ms",
        total_sec,
        total_runs,
        total_sec * 1000.0 / total_runs as f64
    );
}

fn benchmark_async(conf: &Config, physical_mode: bool, conn_id: &str, read_size: u64) {
    let pid = 0;
    let address = dispatch_request(
        conf,
        ProcessInfoRequest {
            conn_id: conn_id.to_string(),
            pid: pid,
        },
    )
    .expect("could not access process info")
    .process
    .unwrap()
    .address;

    let entry = ReadVirtualMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let req = ReadVirtualMemoryRequest {
        conn_id: conn_id.to_string(),
        pid: pid,
        base_offsets: false,
        reads: vec![entry],
    };
    let phys_entry = ReadPhysicalMemoryEntryRequest {
        addr: address,
        len: read_size,
    };
    let phys_req = ReadPhysicalMemoryRequest {
        conn_id: conn_id.to_string(),
        reads: vec![phys_entry],
    };

    let rt = tokio::runtime::Runtime::new().unwrap();

    let start_time = std::time::Instant::now();
    let mut total_runs = 0;

    let bench = async {
        let client = create_client_async(conf).await;
        let mut responses = vec![];
        loop {
            total_runs += 1;

            let response = async {
                if !physical_mode {
                    let mut client_cp = client.clone();
                    dispatch_request_async_client(conf, req.clone(), &mut client_cp)
                        .await
                        .map(|_| ())
                } else {
                    let mut client_cp = client.clone();
                    dispatch_request_async_client(conf, phys_req.clone(), &mut client_cp)
                        .await
                        .map(|_| ())
                }
            };
            responses.push(response);

            if (std::time::Instant::now() - start_time).as_secs() > 10 || total_runs >= 20000 {
                break;
            }
        }
        let results = futures::future::join_all(responses).await;
        for res in results {
            match res {
                Err(e) => error!("{:#?}", e),
                Ok(_) => (),
            }
        }
    };
    rt.block_on(bench);
    let end_time = std::time::Instant::now();

    let total_sec = (end_time - start_time).as_secs_f64();
    println!(
        "Total: {} s, Total: {}, Each: {} ms",
        total_sec,
        total_runs,
        total_sec * 1000.0 / total_runs as f64
    );
}
