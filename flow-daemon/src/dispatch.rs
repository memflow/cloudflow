use crate::dto::response;
use crate::error::{Error, Result};

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

pub async fn write_log(socket: &mut UnixStream, log_msg: &str) -> Result<()> {
    write_response(socket, response::Message::Log(response::Log{
        level: 0,
        msg: log_msg.to_string(),
    })).await
}

pub async fn write_response(socket: &mut UnixStream, msg: response::Message) -> Result<()> {
    let msgstr = serde_json::to_string(&msg).map_err(|_| Error::Serialize)?;
    socket
        .write_all(msgstr.as_bytes())
        .await
        .expect("failed to write data to socket");
    Ok(())
}
