use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::STATE;

use log::info;

use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::CloseConnection,
) -> Result<()> {
    let mut output = String::new();

    if let Ok(mut state) = STATE.lock() {
        if state.connectors.contains_key(&msg.id) {
            output.push_str(&format!("connection {} removed", msg.id));
            state.connectors.remove(&msg.id);
        } else {
            output.push_str(&format!("error: connection {} not found", msg.id));
        }
    }

    info!("{}", output);
    write_log(frame, &output).await
}
