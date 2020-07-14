use crate::error::{Error, Result};

use bytes::BytesMut;
use log::info;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use flow_daemon::{request, response};

pub const SOCKET_PATH: &'static str = "/var/run/memflow.sock";

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
    let mut socket = UnixStream::connect(SOCKET_PATH)
        .await
        .map_err(|_| Error::IO)?;

    write_request(&mut socket, req).await?;

    'outer: loop {
        let mut buf = BytesMut::with_capacity(1024);
        socket
            .read_buf(&mut buf)
            .await
            .map_err(|_| Error::SocketRead)?;

        let strbuf = String::from_utf8(buf.to_vec()).map_err(|_| Error::Deserialize)?;
        let resp: response::Message =
            serde_json::from_str(&strbuf).map_err(|_| Error::Deserialize)?;

        match resp {
            response::Message::Log(msg) => info!("{}", msg.msg),
            _ => {
                if cb(&resp).is_err() {
                    break 'outer;
                }
            }
        }
    }

    Ok(())
}

async fn write_request(socket: &mut UnixStream, msg: request::Message) -> Result<()> {
    let msgstr = serde_json::to_string(&msg).map_err(|_| Error::Serialize)?;
    socket
        .write_all(msgstr.as_bytes())
        .await
        .expect("failed to write data to socket");
    Ok(())
}
