use crate::error::{Error, Result};

use futures::prelude::*;
use tokio::net::UnixStream;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use flow_daemon::{request, response};

use prettytable::{format, Table};

pub const SOCKET_PATH: &str = "/var/run/memflow.sock";

pub fn dispatch_request<T: Fn(&response::Message) -> Result<()>>(
    req: request::Message,
    cb: T,
) -> Result<()> {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_async(req, cb))
}

async fn dispatch_async<T: Fn(&response::Message) -> Result<()>>(
    req: request::Message,
    cb: T,
) -> Result<()> {
    // TODO: print error messages on connection failure
    let mut socket = UnixStream::connect(SOCKET_PATH)
        .await
        .map_err(|_| Error::IO)?;

    let (reader, writer) = socket.split();

    let framed_writer = FramedWrite::new(writer, LengthDelimitedCodec::new());
    let mut serializer = tokio_serde::SymmetricallyFramed::new(
        framed_writer,
        SymmetricalJson::<request::Message>::default(),
    );

    let framed_reader = FramedRead::new(reader, LengthDelimitedCodec::new());
    let mut deserializer = tokio_serde::SymmetricallyFramed::new(
        framed_reader,
        SymmetricalJson::<response::Message>::default(),
    );

    serializer.send(req).await.map_err(|_| Error::IO)?;

    'outer: while let Some(msg) = deserializer.try_next().await.unwrap() {
        match msg {
            response::Message::Log(msg) => println!("{}", msg.msg),
            response::Message::Table(msg) => {
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(msg.headers.into());
                msg.entries.iter().for_each(|e| {
                    table.add_row(e.into());
                });
                table.printstd();
            }
            _ => {
                // TODO: does this callback make sense?
                if cb(&msg).is_err() {
                    break 'outer;
                }
            }
        };
    }

    Ok(())
}
