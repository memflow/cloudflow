use crate::dispatch::*;
use crate::dto::request;
use crate::error::Result;
use crate::response;
use crate::state::{KernelHandle, STATE};

use futures::Sink;
use std::marker::Unpin;

use memflow::*;

pub async fn read<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::ReadPhysicalMemory,
) -> Result<()> {
    let mut state = STATE.lock().await;
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                if let Ok(data) = kernel.phys_mem.phys_read_raw(msg.addr, msg.len) {
                    send_binary_data(frame, data).await
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
    let mut state = STATE.lock().await;
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                if kernel
                    .phys_mem
                    .phys_write_raw(msg.addr, msg.data.as_slice())
                    .is_ok()
                {
                    send_ok(frame).await
                } else {
                    send_err(
                        frame,
                        &format!(
                            "unable to write memory at {} with size {}",
                            msg.addr,
                            msg.data.len()
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

pub async fn metadata<S: Sink<response::Message> + Unpin>(
    frame: &mut S,
    msg: request::PhysicalMemoryMetadata,
) -> Result<()> {
    let mut state = STATE.lock().await;
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let metadata = kernel.phys_mem.metadata();
                send_phys_mem_metadata(frame, metadata).await
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
