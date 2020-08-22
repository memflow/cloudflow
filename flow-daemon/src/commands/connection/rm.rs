use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::STATE;

use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::CloseConnection,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if state.connectors.contains_key(&msg.id) {
        state.connectors.remove(&msg.id);
        send_log_info(frame, &format!("connection {} removed", msg.id)).await?;
    } else {
        send_log_error(frame, &format!("no connection with id {} found", msg.id)).await?;
    }

    send_eof(frame).await
}
