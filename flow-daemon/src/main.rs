mod error;
use error::{Error, Result};

mod dto;
use dto::*;

mod connection;
mod dispatch;

use futures::prelude::*;
use tokio::net::UnixListener;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

#[tokio::main]
async fn main() -> Result<()> {
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

            if let Some(msg) = deserializer.try_next().await.unwrap() {
                match msg {
                    request::Message::Connect(msg) => {
                        connection::open::handle_command(&mut serializer, &msg)
                            .await
                            .expect("failed to execute connect command")
                    }
                    request::Message::ListConnections(_msg) => println!("list command"),
                    request::Message::CloseConnection(_msg) => println!("close command"),
                };

                // currently a client is only supposed to send a single request
            }
        });
    }
}
