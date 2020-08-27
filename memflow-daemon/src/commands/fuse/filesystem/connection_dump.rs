use super::{VMFSConnectionExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;

pub struct VMFSConnectionDump;

impl VMFSConnectionExt for VMFSConnectionDump {
    fn entry(
        &self,
        conn_id: &str,
        _add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            name: "dump".to_string(),
            data_source: Box::new(ConnectionDumpDS::new(&conn_id)),
        }))
    }
}

struct ConnectionDumpDS {
    conn_id: String,
}

impl ConnectionDumpDS {
    pub fn new(conn_id: &str) -> Self {
        Self {
            conn_id: conn_id.to_string(),
        }
    }
}

impl VirtualFileDataSource for ConnectionDumpDS {
    fn content_length(&mut self) -> Result<u64> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let metadata = kernel.phys_mem.metadata();
                Ok(metadata.size as u64)
            }
        }
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => kernel
                .phys_mem
                .phys_read_raw((offset as u64).into(), size as usize)
                .map_err(Error::from),
        }
    }
}
