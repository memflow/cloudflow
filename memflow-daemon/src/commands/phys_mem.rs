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
                // create [PhysicalReadData]
                let mut reads = Vec::new();
                let mut read_data = Vec::new();
                for read in msg.reads.iter() {
                    reads.push(response::PhysicalMemoryReadEntry {
                        data: vec![0u8; read.len],
                    });
                }

                for read in msg.reads.iter().zip(reads.iter_mut()) {
                    read_data.push(PhysicalReadData(read.0.addr, &mut read.1.data[..]));
                }

                if kernel
                    .phys_mem
                    .phys_read_raw_list(&mut read_data.as_mut_slice())
                    .is_ok()
                {
                    send_phys_mem_read(frame, reads).await
                } else {
                    send_err(frame, &format!("unable to read memory: {:?}", msg.reads)).await
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
                // create [PhysicalWriteData]
                let mut write_data = Vec::new();
                for write in msg.writes.iter() {
                    write_data.push(PhysicalWriteData(write.addr, &write.data.as_slice()));
                }

                if kernel
                    .phys_mem
                    .phys_write_raw_list(&write_data.as_slice())
                    .is_ok()
                {
                    send_ok(frame).await
                } else {
                    send_err(frame, "unable to write memory").await
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
