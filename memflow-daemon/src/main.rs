mod error;
use error::{Error, Result};

mod dto;
use dto::*;

mod state;

mod commands;
mod dispatch;

use log::Level;

use futures::prelude::*;
use tokio::net::UnixListener;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

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

    let mut listener = UnixListener::bind("/var/run/memflow.sock").map_err(|_| Error::IO)?;

    loop {
        let (mut socket, _) = listener.accept().await.map_err(|_| Error::IO)?;

        tokio::spawn(async move {
            let (reader, writer) = socket.split();

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
                            .expect("failed to execute connect command")
                    }
                    request::Message::ListConnections => commands::connection::ls(&mut serializer)
                        .await
                        .expect("failed to execute list command"),
                    request::Message::CloseConnection(msg) => {
                        commands::connection::rm(&mut serializer, msg)
                            .await
                            .expect("failed to execute list command")
                    }

                    // TODO: make os specific
                    request::Message::FuseMount(msg) => commands::fuse::mount(&mut serializer, msg)
                        .await
                        .expect("failed to execute fuse mount command"),
                    request::Message::FuseListMounts => commands::fuse::ls(&mut serializer)
                        .await
                        .expect("failed to execute fuse ls command"),
                    request::Message::FuseUmount(msg) => {
                        commands::fuse::umount(&mut serializer, msg)
                            .await
                            .expect("failed to execute fuse umount command")
                    }

                    request::Message::ListProcesses(msg) => {
                        commands::process::ls(&mut serializer, msg)
                            .await
                            .expect("failed to execute process list command")
                    }
                    request::Message::OpenProcess(_msg) => {
                        // TODO:
                    }
                };
            }
        });
    }
}
