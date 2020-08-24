use super::{
    VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile, VirtualFileDataSource,
};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;

pub struct VMFSModuleMemory;

impl VMFSModule for VMFSModuleMemory {
    fn scope(&self) -> VMFSModuleScope {
        VMFSModuleScope::Module
    }

    // TODO: cache contents size ?
    fn entry(&self, inode: u64, ctx: VMFSScopeContext) -> VirtualEntry {
        VirtualEntry::File(VirtualFile {
            inode,
            name: "memory".to_string(),
            data_source: Box::new(VMFSModuleMemoryDS::new(ctx)),
        })
    }
}

struct VMFSModuleMemoryDS {
    ctx: VMFSScopeContext,
}

impl VMFSModuleMemoryDS {
    pub fn new(ctx: VMFSScopeContext) -> Self {
        Self { ctx }
    }
}

impl VirtualFileDataSource for VMFSModuleMemoryDS {
    fn content_length(&self) -> Result<u64> {
        if let VMFSScopeContext::Module {
            conn_id,
            pid,
            peb_entry,
        } = &self.ctx
        {
            let mut state = state_lock_sync();
            let conn = state
                .connection_mut(&conn_id)
                .ok_or_else(|| Error::Other("connection not found"))?;

            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    let mut process = kernel.process_pid(*pid).map_err(Error::from)?;
                    let module_info = process
                        .module_info_from_peb(*peb_entry)
                        .map_err(Error::from)?;
                    Ok(module_info.size() as u64)
                }
            }
        } else {
            Err(Error::Other("no process context supplied"))
        }
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        if let VMFSScopeContext::Module {
            conn_id,
            pid,
            peb_entry,
        } = &self.ctx
        {
            let mut state = state_lock_sync();
            let conn = state
                .connection_mut(&conn_id)
                .ok_or_else(|| Error::Other("connection not found"))?;

            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    let mut process = kernel.process_pid(*pid).map_err(Error::from)?;
                    let module_info = process
                        .module_info_from_peb(*peb_entry)
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
        } else {
            Err(Error::Other("no process context supplied"))
        }
    }
}
