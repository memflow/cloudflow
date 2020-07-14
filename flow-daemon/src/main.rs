mod error;
use error::{Error, Result};

mod dto;
use dto::*;

mod connection;
mod dispatch;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<()> {
    let mut listener = UnixListener::bind("/var/run/memflow.sock").map_err(|_| Error::IO)?;

    loop {
        let (mut socket, _) = listener.accept().await.map_err(|_| Error::IO)?;

        tokio::spawn(async move {
            let mut buf = BytesMut::with_capacity(1024);

            let n = socket
                .read_buf(&mut buf)
                .await
                .expect("failed to read data from socket");

            if n == 0 {
                return;
            }

            let json = std::str::from_utf8(&buf).unwrap().to_string();
            let json_trimmed =
                json.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_ascii_control());
            //println!("json: '{}'", json_trimmed);
            let obj: request::Message = serde_json::from_str(json_trimmed).unwrap();

            match obj {
                request::Message::Connect(msg) => {
                    connection::open::handle_command(&mut socket, &msg).await.expect("failed to execute connect command")
                }
                request::Message::ListConnections(msg) => println!("list command"),
                request::Message::CloseConnection(msg) => println!("close command"),
            };
        });
    }
}
