use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::Kernel;
use crate::state::STATE;

use futures::Sink;
use std::marker::Unpin;

pub async fn ls<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::ListProcesses,
) -> Result<()> {
    let mut table = response::Table::default();
    table.headers = vec![
        "pid".to_string(),
        "name".to_string(),
        "bits".to_string(),
        "dtb".to_string(),
        "teb".to_string(),
        "peb".to_string(),
    ];

    let mut state = STATE.lock().await;

    if let Some(conn) = state.connectors.get_mut(&msg.id) {
        match &mut conn.kernel {
            Kernel::Win32(kernel) => {
                if let Ok(processes) = kernel.process_info_list() {
                    send_log_info(
                        frame,
                        &format!(
                            "listing processes for connection {}: {} processes\n",
                            msg.id,
                            processes.len(),
                        ),
                    )
                    .await?;

                    for process in processes.iter() {
                        table.entries.push(vec![
                            format!("{}", process.pid),
                            process.name.clone(),
                            format!("{}", process.proc_arch.bits()),
                            format!("0x{:X}", process.dtb),
                            format!("0x{:X}", process.teb),
                            format!("0x{:X}", process.peb),
                        ]);
                    }
                } else {
                    send_log_error(
                        frame,
                        &format!("could not get processes on connection {}", msg.id),
                    )
                    .await?;
                }
            }
        }
    } else {
        send_log_error(frame, &format!("no connection with id {} found", msg.id)).await?;
    }

    send_table(frame, table).await?;
    send_eof(frame).await
}
