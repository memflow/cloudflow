use super::{CachedWin32Process, VMFSModuleExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

pub struct VMFSModuleMemory;

impl VMFSModuleExt for VMFSModuleMemory {
    fn entry(
        &self,
        conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
        _add_entry: &mut dyn FnMut(VirtualEntry) -> u64,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            name: "dump".to_string(),
            data_source: Box::new(VMFSModuleMemoryDS::new(
                &conn_id,
                process.proc_info.pid,
                mod_info.base(),
                mod_info.size(),
            )),
        }))
    }
}

struct VMFSModuleMemoryDS {
    conn_id: String,
    pid: PID,
    module_base: Address,
    content_length: usize,
}

impl VMFSModuleMemoryDS {
    pub fn new(conn_id: &str, pid: PID, module_base: Address, content_length: usize) -> Self {
        Self {
            conn_id: conn_id.to_string(),
            pid,
            module_base,
            content_length,
        }
    }
}

impl VirtualFileDataSource for VMFSModuleMemoryDS {
    fn content_length(&mut self) -> Result<u64> {
        Ok(self.content_length as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;
                let len = std::cmp::min(size as usize, self.content_length - offset as usize);
                process
                    .virt_mem
                    .virt_read_raw(self.module_base + offset as usize, len)
                    .data_part()
                    .map_err(Error::from)
            }
        }
    }
}
