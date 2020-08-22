use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{new_uuid, ConnectorState, STATE};

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
        Ok(c) => {
            if let Ok(mut state) = STATE.lock() {
                let uuid = new_uuid();

                state.connectors.insert(
                    uuid.clone(),
                    ConnectorState::new(&uuid, &msg.name, msg.args.clone(), c),
                );

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

    write_log(frame, &output).await
}
