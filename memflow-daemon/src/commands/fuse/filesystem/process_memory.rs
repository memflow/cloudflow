use super::{CachedWin32Process, VMFSProcessExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;

pub struct VMFSProcessMemory;

impl VMFSProcessExt for VMFSProcessMemory {
    fn entry(
        &self,
        conn_id: &str,
        process: &mut CachedWin32Process,
        _add_child: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            name: "mem".to_string(),
            data_source: Box::new(VMFSProcessMemoryDS::new(&conn_id, process.proc_info.pid)),
        }))
    }
}

struct VMFSProcessMemoryDS {
    conn_id: String,
    pid: PID,
}

impl VMFSProcessMemoryDS {
    pub fn new(conn_id: &str, pid: PID) -> Self {
        Self {
            conn_id: conn_id.to_string(),
            pid,
        }
    }
}

impl VirtualFileDataSource for VMFSProcessMemoryDS {
    fn content_length(&mut self) -> Result<u64> {
        Ok(0)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;
                process
                    .virt_mem
                    .virt_read_raw((offset as u64).into(), size as usize)
                    .data_part()
                    .map_err(Error::from)
            }
        }
    }
}
