mod error;
use error::{Error, Result};

mod config;
use config::Config;

mod dto;
use dto::*;

mod state;

mod commands;
mod dispatch;

use std::ffi::CString;

#[macro_use]
extern crate clap;
use clap::{App, Arg};

use log::{error, info, LevelFilter};
use simplelog::{CombinedLogger, SharedLogger, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;

use url::Url;

use futures::prelude::*;
use tokio::net::{TcpListener, UnixListener};
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

/// Spawns a TCP server and listens for incoming connections.
/// The TCP server accept framed json messages and dispatches them to the individual command handlers.
async fn run_server_tcp(addr: &str) -> Result<()> {
    let mut listener = TcpListener::bind(addr).map_err(|_| Error::IO).await?;

    info!("listening for incoming connections");
    while let Some(stream) = listener.next().await {
        match stream {
            Ok(mut stream) => {
                tokio::spawn(async move {
                    let (reader, writer) = stream.split();

                    let framed_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
                    let mut deserializer = tokio_serde::SymmetricallyFramed::new(
                        framed_reader,
                        SymmetricalJson::<request::Message>::default(),
                    );

                    let framed_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());
                    let mut serializer = tokio_serde::SymmetricallyFramed::new(
                        framed_writer,
                        SymmetricalJson::<response::Message>::default(),
                    );

                    // currently a client is only supposed to send a single request
                    while let Ok(msg_opt) = deserializer.try_next().await {
                        if let Some(msg) = msg_opt {
                            handle_message(&mut serializer, msg).await.ok();
                        }
                    }
                });
            }
            Err(e) => {
                error!("{}", e);
                return Err(Error::Other("connection attempt failed"));
            }
        }
    }

    Ok(())
}

/// This function accepts a `request::Message` and dispatches it to the appropiate command handler.
async fn handle_message<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::Message,
) -> Result<()> {
    match msg {
        request::Message::Connect(msg) => {
            commands::connection::new(frame, msg)
                .await
                .expect("failed to execute connection::new command");
        }
        request::Message::ListConnections => {
            commands::connection::ls(frame)
                .await
                .expect("failed to execute connection::ls command");
        }
        request::Message::CloseConnection(msg) => {
            commands::connection::rm(frame, msg)
                .await
                .expect("failed to execute connection::rm command");
        }

        request::Message::ReadPhysicalMemory(msg) => {
            commands::phys_mem::read(frame, msg)
                .await
                .expect("failed to execute phys_mem::read command");
        }
        request::Message::WritePhysicalMemory(msg) => {
            commands::phys_mem::write(frame, msg)
                .await
                .expect("failed to execute phys_mem::write command");
        }
        request::Message::PhysicalMemoryMetadata(msg) => {
            commands::phys_mem::metadata(frame, msg)
                .await
                .expect("failed to execute phys_mem::metadata command");
        }

        // TODO: make os specific
        request::Message::FuseMount(msg) => commands::fuse::mount(frame, msg)
            .await
            .expect("failed to execute fuse::mount command"),
        request::Message::FuseListMounts => commands::fuse::ls(frame)
            .await
            .expect("failed to execute fuse::ls command"),

        request::Message::GdbAttach(msg) => commands::gdb::attach(frame, msg)
            .await
            .expect("failed to execute gdb::attach command"),
        request::Message::GdbList => commands::gdb::ls(frame)
            .await
            .expect("failed to execute gdb::ls command"),

        request::Message::ListProcesses(msg) => {
            commands::process::ls(frame, msg)
                .await
                .expect("failed to execute process::ls command");
        }
        request::Message::OpenProcess(_msg) => {
            // TODO:
        }
    };
    Ok(())
}

#[cfg(not(target_os = "windows"))]
use libc::{chown, getgrnam};
/// Linux & macOS program entry-point
#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;

#[cfg(not(target_os = "windows"))]
const CONFIG_FILE: &str = "/etc/memflow/daemon.conf";

#[cfg(not(target_os = "windows"))]
unsafe fn get_gid_by_name(name: &str) -> Option<libc::gid_t> {
    let namestr = CString::new(name).ok()?;
    let ptr = getgrnam(namestr.as_ptr() as *const libc::c_char);
    if ptr.is_null() {
        None
    } else {
        let s = &*ptr;
        Some(s.gr_gid)
    }
}

pub struct PidFile {
    _fd: i32,
}

impl PidFile {
    pub fn new(path: &str) -> Result<Self> {
        let cpath = CString::new(path).map_err(|_| Error::Other("unable o convert path"))?;

        let fd = unsafe {
            let fd = libc::open(cpath.as_ptr(), libc::O_WRONLY | libc::O_CREAT, 0o666);
            if fd == -1 {
                return Err(Error::Other(
                    "unable to open pidfile, is another daemon instance running?",
                ));
            }

            if libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) == -1 {
                return Err(Error::Other(
                    "unable to lock pidfile, is another daemon instance running?",
                ));
            }

            fd
        };

        Ok(Self { _fd: fd })
    }
}

#[cfg(not(target_os = "windows"))]
#[tokio::main]
async fn main() -> Result<()> {
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
    let url =
        Url::parse(&config.socket_addr).map_err(|_| Error::Other("invalid socket address"))?;
    match url.scheme() {
        "tcp" => {
            if let Some(host_str) = url.host_str() {
                run_server_tcp(&format!("{}:{}", host_str, url.port().unwrap_or(8000))).await?
            } else {
                return Err(Error::Other("invalid tcp host address"));
            }
        }
        "unix" => run_server_uds(url.path()).await?,
        _ => return Err(Error::Other("only tcp and unix urls are supported")),
    };

    Ok(())
}

/// Spawns a unix file socket server and listens for incoming connections.
/// The uds server accept framed json messages and dispatches them to the individual command handlers.
#[cfg(not(target_os = "windows"))]
async fn run_server_uds(path: &str) -> Result<()> {
    // re-create socket if it already exists
    std::fs::remove_file(path).ok();
    let mut listener = UnixListener::bind(path).unwrap();

    // setup uds permissions
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o664);
    std::fs::set_permissions(path, perms).unwrap();

    // change ownership of the uds socket
    // TODO: configure group
    let gid = unsafe { get_gid_by_name("memflow") }
        .ok_or_else(|| Error::Other("unable to find memflow group"))?;
    let raw_socket_file = CString::new(path).map_err(|_| Error::Other("unable to convert path"))?;
    unsafe { chown(raw_socket_file.as_ptr(), libc::getuid(), gid) };

    info!("listening for incoming connections");
    while let Some(stream) = listener.next().await {
        match stream {
            Ok(mut stream) => {
                tokio::spawn(async move {
                    let (reader, writer) = stream.split();

                    let framed_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
                    let mut deserializer = tokio_serde::SymmetricallyFramed::new(
                        framed_reader,
                        SymmetricalJson::<request::Message>::default(),
                    );

                    let framed_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());
                    let mut serializer = tokio_serde::SymmetricallyFramed::new(
                        framed_writer,
                        SymmetricalJson::<response::Message>::default(),
                    );

                    // currently a client is only supposed to send a single request
                    while let Some(msg) = deserializer.try_next().await.unwrap() {
                        handle_message(&mut serializer, msg).await.ok();
                    }
                });
            }
            Err(e) => {
                error!("{}", e);
                return Err(Error::Other("connection attempt failed"));
            }
        }
    }

    Ok(())
}
