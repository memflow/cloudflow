use crate::error::{Error, Result};
use crate::response;
use crate::state::STATE;

use futures::prelude::*;
use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    let mut table = response::Table::default();

    table.headers = vec!["id".to_string(), "name".to_string(), "args".to_string()];

    if let Ok(state) = STATE.lock() {
        for c in state.connectors.iter() {
            let entry = vec![
                c.1.id.to_string(),
                c.1.name.to_string(),
                c.1.args.as_ref().map(|a| a.to_string()).unwrap_or_default(),
            ];
            table.entries.push(entry);
        }
    }

    frame
        .send(response::Message::Table(table))
        .await
        .map_err(|_| Error::IO)
}
