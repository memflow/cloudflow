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
    _msg: request::ListConnections,
) -> Result<()> {
    let mut output = String::new();
    output.push_str("id\t\tname\t\targs\n");

    if let Ok(state) = STATE.lock() {
        for c in state.connectors.iter() {
            info!("{}\t{}\t{:?}", c.1.id, c.1.name, c.1.args);
            output.push_str(&format!("{}\t{}\t{:?}\n", c.1.id, c.1.name, c.1.args));
        }
    }

    write_log(frame, &output).await
}
