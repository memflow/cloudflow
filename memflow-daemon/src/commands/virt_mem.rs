use crate::error::{Error, Result};
use crate::state::{KernelHandle, STATE};

use memflow::{VirtualMemory, VirtualReadData, VirtualWriteData};

use crate::memflow_rpc::{
    ReadVirtualMemoryEntryResponse, ReadVirtualMemoryRequest, ReadVirtualMemoryResponse,
    WriteVirtualMemoryRequest, WriteVirtualMemoryResponse,
};

pub async fn read(msg: &ReadVirtualMemoryRequest) -> Result<ReadVirtualMemoryResponse> {
    let mut state = STATE.lock().await;
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                // create [VirtualReadData]
                let mut result_reads = Vec::new();
                let mut read_data = Vec::new();
                for read in msg.reads.iter() {
                    result_reads.push(ReadVirtualMemoryEntryResponse {
                        data: vec![0u8; read.len as usize],
                    });
                }

                let mut process = kernel.process_pid(msg.pid)?;

                // We need u64 here, because memflow::Address does not implement Add
                let offset = if msg.base_offsets {
                    process.proc_info.section_base.as_u64()
                } else {
                    0
                };

                for read in msg.reads.iter().zip(result_reads.iter_mut()) {
                    read_data.push(VirtualReadData(
                        (offset + read.0.addr).into(),
                        &mut read.1.data[..],
                    ));
                }

                process
                    .virt_mem
                    .virt_read_raw_list(&mut read_data.as_mut_slice())?;

                Ok(ReadVirtualMemoryResponse {
                    reads: result_reads,
                })
            }
        }
    } else {
        Err(Error::Connector(format!(
            "no connection with id {} found",
            msg.conn_id
        )))
    }
}

pub async fn write(msg: &WriteVirtualMemoryRequest) -> Result<WriteVirtualMemoryResponse> {
    let mut state = STATE.lock().await;
    if let Some(conn) = state.connection_mut(&msg.conn_id) {
        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                // create [VirtualWriteData]
                let mut write_data = Vec::new();

                let mut process = kernel.process_pid(msg.pid)?;

                // We need u64 here, because memflow::Address does not implement Add
                let offset = if msg.base_offsets {
                    process.proc_info.section_base.as_u64()
                } else {
                    0
                };

                for write in msg.writes.iter() {
                    write_data.push(VirtualWriteData(
                        (offset + write.addr).into(),
                        &write.data.as_slice(),
                    ));
                }

                process
                    .virt_mem
                    .virt_write_raw_list(&write_data.as_slice())?;

                Ok(WriteVirtualMemoryResponse {})
            }
        }
    } else {
        Err(Error::Connector(format!(
            "no connection with id {} found",
            msg.conn_id
        )))
    }
}
