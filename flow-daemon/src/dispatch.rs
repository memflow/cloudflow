use crate::dto::response;
use crate::error::{Error, Result};

use futures::prelude::*;
use futures::Sink;
use std::marker::Unpin;

pub async fn write_log<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    log_msg: &str,
) -> Result<()> {
    frame
        .send(response::Message::Log(response::Log {
            level: 0,
            msg: log_msg.to_string(),
        }))
        .await
        .map_err(|_| Error::IO)
}
