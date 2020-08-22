use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{new_uuid, ConnectorState, Kernel, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow_core::*;

fn create_connector(msg: &request::Connect) -> Result<ConnectorInstance> {
    let args = match &msg.args {
        Some(a) => ConnectorArgs::try_parse_str(a)
            .map_err(|_| Error::Connector("unable to parse connector string"))?,
        None => ConnectorArgs::default(),
    };

    let inventory = unsafe { ConnectorInventory::try_new() }.unwrap();
    unsafe { inventory.create_connector(&msg.name, &args) }
        .map_err(|_| Error::Connector("unable to create connector"))
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

            let uuid = new_uuid();

            let conn_state =
                ConnectorState::new(&uuid, &msg.name, msg.args.clone(), Kernel::Win32(kernel));

            state.connectors.insert(uuid.clone(), conn_state);

            send_log_info(
                frame,
                &format!(
                    "connection created: {} | {} | {:?}",
                    uuid, msg.name, msg.args
                ),
            )
            .await?;
        }
        Err(err) => {
            send_log_error(
                frame,
                &format!(
                    "could not create connector: {} | {:?} ({})",
                    msg.name, msg.args, err
                ),
            )
            .await?;
        }
    };

    send_eof(frame).await
}

pub async fn ls<S: Sink<response::Message> + Unpin>(frame: &mut S) -> Result<()> {
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

pub async fn rm<S: Sink<response::Message> + Unpin>(
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
