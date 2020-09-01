mod stub;

use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::{new_uuid, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow::PID;

pub async fn attach<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::GdbAttach,
) -> Result<()> {
    let mut state = STATE.lock().await;

    // find connection and spawn gdb thread
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        let kernel = conn.kernel.clone();
        let id = new_uuid();

        send_log_info(
            frame,
            &format!("gdb stub with id {} spawned at address {}", id, &msg.addr),
        )
        .await?;
        send_log_info(
            frame,
            "the gdb stub will automatically be closed on disconnect",
        )
        .await?;

        std::thread::spawn(move || {
            stub::spawn_gdb_stub(
                &id,
                &msg.conn_id,
                msg.pid.parse::<PID>().unwrap(),
                &msg.addr,
                kernel,
            )
            .unwrap();
        });

        send_ok(frame).await
    } else {
        send_err(
            frame,
            &format!("no connection with id {} found", msg.conn_id),
        )
        .await
    }
}

pub async fn ls<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    let state = STATE.lock().await;

    send_log_info(
        frame,
        &format!("listing open gdb stubs: {} stubs", state.gdb_stubs.len()),
    )
    .await?;

    if !state.gdb_stubs.is_empty() {
        let mut table = response::Table::default();
        table.headers = vec![
            "id".to_string(),
            "connection".to_string(),
            "address".to_string(),
        ];

        for c in state.gdb_stubs.iter() {
            let entry = vec![c.1.id.clone(), c.1.conn_id.clone(), c.1.addr.clone()];
            table.entries.push(entry);
        }

        send_table(frame, table).await?;
    }

    send_ok(frame).await
}
