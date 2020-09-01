use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{KernelHandle, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow::*;

fn create_connector(msg: &request::Connect) -> Result<ConnectorInstance> {
    let args = match &msg.args {
        Some(a) => ConnectorArgs::parse(a)
            .map_err(|_| Error::Connector("unable to parse connector string"))?,
        None => ConnectorArgs::default(),
    };

    let inventory = unsafe { ConnectorInventory::try_new() }.map_err(Error::from)?;
    unsafe { inventory.create_connector(&msg.name, &args) }.map_err(Error::from)
}

pub async fn new<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::Connect,
) -> Result<()> {
    match create_connector(&msg) {
        Ok(conn) => {
            // TODO: add os argument
            // TODO: redirect log to client
            // TODO: add cache options

            send_log_info(frame, "connector created").await?;

            // initialize kernel
            let kernel = memflow_win32::Kernel::builder(conn)
                .build_default_caches()
                .build()
                .map_err(|_| Error::Connector("unable to find kernel"))?;

            send_log_info(frame, "found win32 kernel").await?;

            let mut state = STATE.lock().await;

            match state.connection_add(
                &msg.name,
                msg.args.clone(),
                msg.alias,
                KernelHandle::Win32(kernel),
            ) {
                Ok(id) => {
                    send_log_info(
                        frame,
                        &format!("connection created: {} | {} | {:?}", id, msg.name, msg.args),
                    )
                    .await?;
                    send_ok(frame).await
                }
                Err(err) => {
                    send_err(
                        frame,
                        &format!(
                            "could not create connector: {} | {:?} ({})",
                            msg.name, msg.args, err
                        ),
                    )
                    .await
                }
            }
        }
        Err(err) => {
            send_err(
                frame,
                &format!(
                    "could not create connector: {} | {:?} ({})",
                    msg.name, msg.args, err
                ),
            )
            .await
        }
    }
}

pub async fn ls<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
    let state = STATE.lock().await;

    send_log_info(
        frame,
        &format!(
            "listing open connections: {} connections",
            state.connections.len()
        ),
    )
    .await?;

    if !state.connections.is_empty() {
        let mut table = response::Table::default();
        table.headers = vec![
            "id".to_string(),
            "alias".to_string(),
            "refs".to_string(),
            "name".to_string(),
            "args".to_string(),
        ];

        for c in state.connections.iter() {
            let entry = vec![
                c.1.id.to_string(),
                c.1.alias
                    .as_ref()
                    .map(|a| a.to_string())
                    .unwrap_or_default(),
                c.1.refcount.to_string(),
                c.1.name.to_string(),
                c.1.args.as_ref().map(|a| a.to_string()).unwrap_or_default(),
            ];
            table.entries.push(entry);
        }

        send_table(frame, table).await?;
    }

    send_ok(frame).await
}

pub async fn rm<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::CloseConnection,
) -> Result<()> {
    let mut state = STATE.lock().await;

    match state.connection_remove(&msg.conn_id) {
        Ok(_) => {
            send_log_info(frame, &format!("connection {} removed", msg.conn_id)).await?;
            send_ok(frame).await
        }
        Err(err) => {
            send_err(
                frame,
                &format!("unable to remove connection {}: {}", msg.conn_id, err),
            )
            .await
        }
    }
}
