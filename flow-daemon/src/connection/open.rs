use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;

use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    _msg: &request::Connect,
) -> Result<()> {
    write_log(frame, "opening connection").await?;
    write_log(frame, "opening connection2").await?;
    write_log(frame, "opening connection3").await?;
    write_log(frame, "opening connection4").await?;
    write_log(frame, "opening connection5").await
}
