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

pub async fn handle_command<S: Sink<response::Message> + Unpin>(
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
