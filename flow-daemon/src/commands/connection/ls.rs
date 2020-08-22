use crate::dispatch::*;
use crate::error::Result;
use crate::response;
use crate::state::STATE;

use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    let state = STATE.lock().await;

    send_log_warn(
        frame,
        &format!(
            "listing open connections: {} connections",
            state.connectors.len()
        ),
    )
    .await?;

    if !state.connectors.is_empty() {
        let mut table = response::Table::default();
        table.headers = vec!["id".to_string(), "name".to_string(), "args".to_string()];

        for c in state.connectors.iter() {
            let entry = vec![
                c.1.id.to_string(),
                c.1.name.to_string(),
                c.1.args.as_ref().map(|a| a.to_string()).unwrap_or_default(),
            ];
            table.entries.push(entry);
        }

        send_table(frame, table).await?;
    }

    send_eof(frame).await
}
