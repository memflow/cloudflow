use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::Kernel;
use crate::state::STATE;

use log::info;

use futures::prelude::*;
use futures::Sink;
use std::marker::Unpin;

pub async fn handle_command<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::ListProcesses,
) -> Result<()> {
    let mut output = String::new();

    let mut table = response::Table::default();
    table.headers = vec![
        "pid".to_string(),
        "name".to_string(),
        "arch".to_string(),
        "peb".to_string(),
    ];

    if let Ok(mut state) = STATE.lock() {
        if let Some(conn) = state.connectors.get_mut(&msg.id) {
            match &mut conn.kernel {
                Kernel::Win32(kernel) => {
                    if let Ok(processes) = kernel.process_info_list() {
                        output.push_str(&format!(
                            "listing {} processes for connection {}",
                            processes.len(),
                            msg.id
                        ));

                        for process in processes.iter() {
                            table.entries.push(vec![
                                format!("{}", process.pid),
                                process.name.clone(),
                                if process.wow64.is_null() {
                                    "64".to_string()
                                } else {
                                    "32".to_string()
                                },
                                format!("0x{:X}", process.peb),
                            ]);
                        }
                    } else {
                        output.push_str(&format!(
                            "error: could not get processes on connection {}",
                            msg.id
                        ));
                    }
                }
            }
        } else {
            output.push_str(&format!("error: connection {} not found", msg.id));
        }
    }

    info!("{}", output);
    write_log(frame, &output).await?;

    frame
        .send(response::Message::Table(table))
        .await
        .map_err(|_| Error::IO)
}
