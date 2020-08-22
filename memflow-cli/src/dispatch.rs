use crate::error::{Error, Result};

use futures::prelude::*;
use log::{debug, error, info, warn};
use tokio::net::UnixStream;
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use memflow_daemon::{request, response};

use prettytable::{format, Table};

pub const SOCKET_PATH: &str = "/var/run/memflow.sock";

pub fn dispatch_request(req: request::Message) -> Result<()> {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_async(req))
}

async fn dispatch_async(req: request::Message) -> Result<()> {
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
            response::Message::Log(msg) => {
                match msg.level {
                    1 /* Debug::Error */ => error!("{}", msg.msg),
                    2 /* Debug::Warn */ => warn!("{}", msg.msg),
                    3 /* Debug::Info */ => info!("{}", msg.msg),
                    4 /* Debug::Debug */ => debug!("{}", msg.msg),
                    _ => (),
                }
            }
            response::Message::Table(msg) => {
                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                table.set_titles(msg.headers.into());
                msg.entries.iter().for_each(|e| {
                    table.add_row(e.into());
                });
                table.printstd();
            }
            response::Message::EOF => {
                break 'outer;
            }
        };
    }

    Ok(())
}
