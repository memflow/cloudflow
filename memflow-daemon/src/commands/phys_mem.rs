use crate::dispatch::*;
use crate::dto::request;
use crate::error::{Error, Result};
use crate::response;
use crate::state::{KernelHandle, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow_core::*;

pub async fn read<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::ReadPhysicalMemory,
) -> Result<()> {
    let mut state = STATE.lock().await;

    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                if let Ok(data) = kernel.phys_mem.phys_read_raw(msg.addr.into(), msg.len) {
                    send_binary_data(frame, data).await
                //send_ok(frame).await
                } else {
                    send_err(
                        frame,
                        &format!(
                            "unable to read memory at {} with size {}",
                            msg.addr, msg.len
                        ),
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

pub async fn write<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::WritePhysicalMemory,
) -> Result<()> {
    send_ok(frame).await
}
