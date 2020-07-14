use crate::dto::request;
use crate::dispatch::*;
use crate::error::{Error, Result};

use tokio::net::UnixStream;

pub async fn handle_command(socket: &mut UnixStream, msg: &request::Connect) -> Result<()> {
    write_log(socket, "opening connection").await
}
