use super::{CachedWin32Process, VMFSProcessExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

pub struct VMFSProcessMemory;

impl VMFSProcessExt for VMFSProcessMemory {
    fn entry(
        &self,
        inode: u64,
        conn_id: &str,
        process: &mut CachedWin32Process,
    ) -> Result<VirtualEntry> {
        // TODO: use re-mapping from flow core here to remap modules into a range?

        // find range of mapped modules
        let modules = process.module_info_list().unwrap(); // TODO: error case
        println!("modules {} {}", process.proc_info.name, modules.len());

        let mapping_range_start = modules
            .iter()
            .min_by(|a, b| a.base().cmp(&b.base()))
            .ok_or_else(|| Error::Other("unable to find mapping base"))?;
        let mapping_range_end = modules
            .iter()
            .max_by(|a, b| a.base().cmp(&b.base()))
            .ok_or_else(|| Error::Other("unable to find mapping size"))?;

        Ok(VirtualEntry::File(VirtualFile {
            inode,
            name: "memory".to_string(),
            data_source: Box::new(VMFSProcessMemoryDS::new(
                &conn_id,
                process.proc_info.pid,
                mapping_range_start.base(),
                (mapping_range_end.base() + mapping_range_end.size()) - mapping_range_start.base(),
            )),
        }))
    }
}

struct VMFSProcessMemoryDS {
    conn_id: String,
    pid: PID,
    mapping_base: Address,
    content_length: usize,
}

impl VMFSProcessMemoryDS {
    pub fn new(conn_id: &str, pid: PID, mapping_base: Address, content_length: usize) -> Self {
        Self {
            conn_id: conn_id.to_string(),
            pid,
            mapping_base,
            content_length,
        }
    }
}

impl VirtualFileDataSource for VMFSProcessMemoryDS {
    fn content_length(&self) -> Result<u64> {
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
                    .virt_read_raw(self.mapping_base + offset as usize, len)
                    .data_part()
                    .map_err(Error::from)
            }
        }
    }
}
