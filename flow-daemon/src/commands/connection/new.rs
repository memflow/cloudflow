use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{new_uuid, ConnectorState, Kernel, STATE};

use log::info;

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
    let mut output = String::new();

    match create_connector(&msg) {
        Ok(conn) => {
            // TODO: add os argument
            // TODO: redirect log to client
            // TODO: add cache options

            // initialize kernel
            let kernel = memflow_win32::Kernel::builder(conn)
                .build_default_caches()
                .build()
                .map_err(|_| Error::Connector("unable to find kernel"))?;

            output.push_str(&format!("found win32 kernel",));

            if let Ok(mut state) = STATE.lock() {
                let uuid = new_uuid();

                let conn_state =
                    ConnectorState::new(&uuid, &msg.name, msg.args.clone(), Kernel::Win32(kernel));

                state.connectors.insert(uuid.clone(), conn_state);

                output.push_str(&format!(
                    "connection created: {}\t{}\t{:?}",
                    uuid, msg.name, msg.args
                ));
            } else {
                output.push_str(&format!(
                    "error: could not create connector: {}\t{:?}",
                    msg.name, msg.args
                ));
            }
        }
        Err(err) => {
            output.push_str(&format!(
                "error: could not create connector: {}\t{:?} ({})",
                msg.name, msg.args, err
            ));
        }
    };

    info!("{}", output);
    write_log(frame, &output).await
}
