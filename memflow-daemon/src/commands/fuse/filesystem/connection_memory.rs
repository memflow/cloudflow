use super::{VMFSConnectionExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;

pub struct VMFSConnectionMemory;

impl VMFSConnectionExt for VMFSConnectionMemory {
    fn entry(&self, inode: u64, conn_id: &str) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            inode,
            name: "mem".to_string(),
            data_source: Box::new(VMFSConnectionMemoryDS::new(&conn_id)),
        }))
    }
}

struct VMFSConnectionMemoryDS {
    conn_id: String,
}

impl VMFSConnectionMemoryDS {
    pub fn new(conn_id: &str) -> Self {
        Self {
            conn_id: conn_id.to_string(),
        }
    }
}

impl VirtualFileDataSource for VMFSConnectionMemoryDS {
    fn content_length(&self) -> Result<u64> {
        Ok(0)
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
