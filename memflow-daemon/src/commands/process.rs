use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::KernelHandle;
use crate::state::STATE;

use futures::Sink;
use std::marker::Unpin;

pub async fn ls<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::ListProcesses,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                if let Ok(processes) = kernel.process_info_list() {
                    send_log_info(
                        frame,
                        &format!(
                            "listing processes for connection {}: {} processes\n",
                            msg.conn_id,
                            processes.len(),
                        ),
                    )
                    .await?;

                    let mut table = response::Table::default();
                    table.headers = vec![
                        "pid".to_string(),
                        "name".to_string(),
                        "bits".to_string(),
                        "dtb".to_string(),
                        "teb".to_string(),
                        "peb".to_string(),
                    ];

                    for process in processes.iter() {
                        table.entries.push(vec![
                            process.pid.to_string(),
                            process.name.clone(),
                            process.proc_arch.bits().to_string(),
                            format!("0x{:X}", process.dtb),
                            format!("0x{:X}", process.teb.unwrap_or_default()),
                            format!("0x{:X}", process.peb()),
                        ]);
                    }

                    send_table(frame, table).await?;
                    send_ok(frame).await
                } else {
                    send_err(
                        frame,
                        &format!("could not get processes on connection {}", msg.conn_id),
                    )
                    .await
                }
            }
        }
    } else {
        send_err(
            frame,
            &format!("no connection with id {} found", msg.conn_id),
        )
        .await
    }
}

/*
pub async fn open<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::Connect,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connections.get_mut(&msg.id) {
        match &mut conn.kernel {
            Kernel::Win32(kernel) => {

                kernel.process_info(name)

            }
        }
    } else {
        send_log_error(frame, &format!("no connection with id {} found", msg.id)).await?;
    }

    send_eof(frame).await




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

            let opened_connection =
                OpenedConnection::new(&uuid, &msg.name, msg.args.clone(), Kernel::Win32(kernel));

            state.connections.insert(uuid.clone(), opened_connection);

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
*/
