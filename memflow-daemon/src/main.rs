use std::{ffi::CString, fs::File};

use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};
use log::LevelFilter;
use memflow_daemon::Config;
use memflow_rpc::memflow_server::{Memflow, MemflowServer};
use memflow_rpc::{
    CloseConnectionRequest, CloseConnectionResponse, ListConnectionsRequest,
    ListConnectionsResponse, ListProcessesRequest, ListProcessesResponse, NewConnectionRequest,
    NewConnectionResponse, PhysicalMemoryMetadataRequest, PhysicalMemoryMetadataResponse,
    ProcessInfoRequest, ProcessInfoResponse, ReadPhysicalMemoryRequest, ReadPhysicalMemoryResponse,
    ReadVirtualMemoryRequest, ReadVirtualMemoryResponse, WritePhysicalMemoryRequest,
    WritePhysicalMemoryResponse, WriteVirtualMemoryRequest, WriteVirtualMemoryResponse,
};
use simplelog::{CombinedLogger, SharedLogger, TermLogger, TerminalMode, WriteLogger};
use tonic::{transport::Server, Request, Response, Status};

mod memflow_rpc {
    tonic::include_proto!("memflow_rpc");
}

mod error;
use error::{Error, Result};

mod config;

mod state;

mod commands;

fn map_to_tonic<T>(res: Result<T>) -> core::result::Result<tonic::Response<T>, Status> {
    match res {
        Ok(val) => Ok(tonic::Response::new(val)),
        Err(err) => Err(Status::internal(format!("{:#?}", err))),
    }
}

// defining a struct for our service
#[derive(Default)]
pub struct MyMemflow {}

// implementing rpc for service defined in .proto
#[tonic::async_trait]
impl Memflow for MyMemflow {
    // our rpc impelemented as function
    async fn new_connection(
        &self,
        request: Request<NewConnectionRequest>,
    ) -> core::result::Result<Response<NewConnectionResponse>, Status> {
        let message = request.into_inner();

        map_to_tonic(commands::connection::new(&message).await)
    }

    async fn list_connections(
        &self,
        request: Request<ListConnectionsRequest>,
    ) -> core::result::Result<Response<ListConnectionsResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::connection::ls(&message).await)
    }

    async fn close_connection(
        &self,
        request: Request<CloseConnectionRequest>,
    ) -> core::result::Result<Response<CloseConnectionResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::connection::rm(&message).await)
    }

    async fn read_physical_memory(
        &self,
        request: Request<ReadPhysicalMemoryRequest>,
    ) -> std::result::Result<Response<ReadPhysicalMemoryResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::phys_mem::read(&message).await)
    }
    async fn write_physical_memory(
        &self,
        request: Request<WritePhysicalMemoryRequest>,
    ) -> std::result::Result<Response<WritePhysicalMemoryResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::phys_mem::write(&message).await)
    }
    async fn physical_memory_metadata(
        &self,
        request: Request<PhysicalMemoryMetadataRequest>,
    ) -> std::result::Result<Response<PhysicalMemoryMetadataResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::phys_mem::metadata(&message).await)
    }
    async fn read_virtual_memory(
        &self,
        request: Request<ReadVirtualMemoryRequest>,
    ) -> std::result::Result<Response<ReadVirtualMemoryResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::virt_mem::read(&message).await)
    }
    async fn write_virtual_memory(
        &self,
        request: Request<WriteVirtualMemoryRequest>,
    ) -> std::result::Result<Response<WriteVirtualMemoryResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::virt_mem::write(&message).await)
    }
    async fn list_processes(
        &self,
        request: Request<ListProcessesRequest>,
    ) -> std::result::Result<Response<ListProcessesResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::process::ls(&message).await)
    }
    async fn process_info(
        &self,
        request: Request<ProcessInfoRequest>,
    ) -> std::result::Result<Response<ProcessInfoResponse>, Status> {
        let message = request.into_inner();
        map_to_tonic(commands::process::process_info(&message).await)
    }
}

pub struct PidFile {
    _fd: i32,
}

impl PidFile {
    pub fn new(path: &str) -> Result<Self> {
        let cpath =
            CString::new(path).map_err(|_| Error::Other("unable o convert path".to_string()))?;

        let fd = unsafe {
            let fd = libc::open(cpath.as_ptr(), libc::O_WRONLY | libc::O_CREAT, 0o666);
            if fd == -1 {
                return Err(Error::Other(
                    "unable to open pidfile, is another daemon instance running?".to_string(),
                ));
            }

            if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == -1 {
                return Err(Error::Other(
                    "unable to lock pidfile, is another daemon instance running?".to_string(),
                ));
            }

            fd
        };

        Ok(Self { _fd: fd })
    }
}

#[cfg(not(target_os = "windows"))]
const CONFIG_FILE: &str = "/etc/memflow/daemon.conf";

fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .long_version(format!("version: {}", crate_version!()).as_str())
        .author(crate_authors!())
        .about("memflow cli daemon")
        .after_help(crate_description!())
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .help("the config file path")
                .takes_value(true)
                .required(false)
                .default_value(CONFIG_FILE),
        )
        .arg(
            Arg::with_name("elevate")
                .short("E")
                .long("elevate")
                .help("elevate privileges upon start")
                .takes_value(false)
                .required(false),
        )
        .get_matches();

    if matches.occurrences_of("elevate") > 0 {
        sudo::escalate_if_needed().expect("failed to elevate privileges");
    }

    // load config
    let config_path = matches.value_of("config").unwrap();
    let config_str = std::fs::read_to_string(config_path).unwrap();
    let config: Config = serde_json::from_str(&config_str).unwrap();

    // setup verbosity
    let log_filter = match config
        .verbosity
        .unwrap_or_else(|| "info".to_string())
        .as_str()
    {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Trace,
    };

    // set console verbosity
    let console_log_filter = match matches.occurrences_of("verbose") {
        0 => log_filter,
        1 => LevelFilter::Error,
        2 => LevelFilter::Warn,
        3 => LevelFilter::Info,
        4 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    // setup logging
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        console_log_filter,
        simplelog::Config::default(),
        TerminalMode::Mixed,
    )];

    if let Some(log_file) = config.log_file {
        let log_file = File::create(log_file)
            .expect("Unable to create log file. Insufficent privileges? (rerun with -E");
        loggers.push(WriteLogger::new(
            log_filter,
            simplelog::Config::default(),
            log_file,
        ));
    }

    let _ = CombinedLogger::init(loggers);

    // instantiate pid file
    let _pid_file = PidFile::new(
        &config
            .pid_file
            .unwrap_or_else(|| "/var/run/memflow.pid".to_string()),
    )
    .expect("Failed to create PID file. Insufficent privileges? (rerun with -E)");

    // setup the listening socket
    let addr = config.socket_addr.parse().unwrap();
    // todo!("Read address from config");
    let memflow = MyMemflow::default();
    let rt = tokio::runtime::Runtime::new().expect("failed to obtain a new RunTime object");

    println!("MemflowServer listening on {}", addr);

    let server = Server::builder()
        .add_service(MemflowServer::new(memflow))
        .serve(addr);
    rt.block_on(server)
        .expect("failed to run the server on tokio::runtime");
}
