mod error;
use error::{Error, Result};

mod dto;
use dto::*;

mod state;

mod commands;
mod dispatch;

use log::{error, info, Level};
use std::ffi::CString;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;

use libc::{chown, getgrnam};

use futures::prelude::*;
use tokio::net::UnixListener;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

// TODO: different main for different os
const PID_FILE: &str = "/var/run/memflow.pid";
const SOCKET_FILE: &str = "/var/run/memflow.sock";
const LOG_FILE: &str = "/var/log/memflow.log";

pub unsafe fn get_gid_by_name(name: &str) -> Option<libc::gid_t> {
    let namestr = CString::new(name).ok()?;
    let ptr = getgrnam(namestr.as_ptr() as *const libc::c_char);
    if ptr.is_null() {
        None
    } else {
        let s = &*ptr;
        Some(s.gr_gid)
    }
}

async fn run_server(mut listener: UnixListener) -> Result<()> {
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
                        match msg {
                            request::Message::Connect(msg) => {
                                commands::connection::new(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command");
                            }
                            request::Message::ListConnections => {
                                commands::connection::ls(&mut serializer)
                                    .await
                                    .expect("failed to execute command");
                            }
                            request::Message::CloseConnection(msg) => {
                                commands::connection::rm(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command");
                            }

                            // TODO: make os specific
                            request::Message::FuseMount(msg) => {
                                commands::fuse::mount(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command")
                            }

                            request::Message::FuseListMounts => commands::fuse::ls(&mut serializer)
                                .await
                                .expect("failed to execute command"),

                            request::Message::FuseUmount(msg) => {
                                commands::fuse::umount(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command");
                            }

                            request::Message::GDBAttach(msg) => {
                                commands::gdb::attach(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command")
                            }
                            request::Message::GDBList => commands::gdb::ls(&mut serializer)
                                .await
                                .expect("failed to execute command"),
                            request::Message::GDBDetach(msg) => {
                                commands::gdb::detach(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command")
                            }
                            request::Message::ListProcesses(msg) => {
                                commands::process::ls(&mut serializer, msg)
                                    .await
                                    .expect("failed to execute command");
                            }
                            request::Message::OpenProcess(_msg) => {
                                // TODO:
                            }
                        };
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

#[tokio::main]
async fn main() -> Result<()> {
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

    std::fs::remove_file(SOCKET_FILE).ok();
    let listener = UnixListener::bind(SOCKET_FILE).unwrap();

    // change permissions
    let mut perms = std::fs::metadata(SOCKET_FILE).unwrap().permissions();
    perms.set_mode(0o664);
    std::fs::set_permissions(SOCKET_FILE, perms).unwrap();

    // change ownership
    let gid = unsafe { get_gid_by_name("memflow") }
        .ok_or_else(|| Error::Other("unable to find memflow group"))?;
    let raw_socket_file =
        CString::new(SOCKET_FILE).map_err(|_| Error::Other("unable to convert path"))?;
    unsafe { chown(raw_socket_file.as_ptr(), libc::getuid(), gid) };

    run_server(listener).await
}
