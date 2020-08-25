use super::{CachedWin32Process, VMFSModuleExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

pub struct VMFSModuleMemory;

impl VMFSModuleExt for VMFSModuleMemory {
    fn entry(
        &self,
        inode: u64,
        conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
    ) -> VirtualEntry {
        VirtualEntry::File(VirtualFile {
            inode,
            name: "memory".to_string(),
            data_source: Box::new(VMFSModuleMemoryDS::new(
                &conn_id,
                process.proc_info.pid,
                mod_info.peb_entry,
            )),
        })
    }
}

struct VMFSModuleMemoryDS {
    conn_id: String,
    pid: PID,
    peb_entry: Address,
}

impl VMFSModuleMemoryDS {
    pub fn new(conn_id: &str, pid: PID, peb_entry: Address) -> Self {
        Self {
            conn_id: conn_id.to_string(),
            pid,
            peb_entry,
        }
    }
}

impl VirtualFileDataSource for VMFSModuleMemoryDS {
    fn content_length(&self) -> Result<u64> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;
                let module_info = process
                    .module_info_from_peb(self.peb_entry)
                    .map_err(Error::from)?;
                Ok(module_info.size() as u64)
            }
        }
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;
                let module_info = process
                    .module_info_from_peb(self.peb_entry)
                    .map_err(Error::from)?;

                let len = std::cmp::min(size as usize, module_info.size() - offset as usize);
                if len > 0 {
                    process
                        .virt_mem
                        .virt_read_raw(module_info.base() + offset as usize, len)
                        .data_part()
                        .map_err(Error::from)
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
}
