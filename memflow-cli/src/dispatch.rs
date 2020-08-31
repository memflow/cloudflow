use crate::error::{Error, Result};
use crate::Config;

use log::{debug, error, info, warn};
use url::Url;

use futures::prelude::*;
use tokio::net::{TcpStream, UnixStream};
use tokio_serde::formats::*;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use memflow_daemon::{request, response};

use prettytable::{format, Table};

pub fn dispatch_request(conf: &Config, req: request::Message) -> Result<()> {
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(dispatch_async(conf, req))
}

async fn dispatch_async(conf: &Config, req: request::Message) -> Result<()> {
    let url = Url::parse(&conf.host).map_err(|_| Error::Other("invalid socket address"))?;
    match url.scheme() {
        "tcp" => {
            if let Some(host_str) = url.host_str() {
                dispatch_async_tcp(&format!("{}:{}", host_str, url.port().unwrap_or(8000)), req)
                    .await
            } else {
                return Err(Error::Other("invalid tcp host address"));
            }
        }
        "unix" => dispatch_async_uds(url.path(), req).await,
        _ => return Err(Error::Other("only tcp and unix urls are supported")),
    }
}

async fn dispatch_async_tcp(addr: &str, req: request::Message) -> Result<()> {
    // TODO: print error messages on connection failure
    // TODO: setup timeouts

    let mut socket = TcpStream::connect(addr).await.map_err(|_| Error::IO)?;

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
        if !handle_message(msg) {
            break 'outer;
        }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
async fn dispatch_async_uds(path: &str, req: request::Message) -> Result<()> {
    // TODO: print error messages on connection failure
    // TODO: setup timeouts

    let mut socket = UnixStream::connect(path).await.map_err(|_| Error::IO)?;

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
        if !handle_message(msg) {
            break 'outer;
        }
    }

    Ok(())
}

fn handle_message(msg: response::Message) -> bool {
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
        response::Message::Result(msg) => {
            if !msg.success {
                error!("{}", msg.msg);
            }
            // break loop
            return false;
        }
    };

    // continue loop
    true
}
