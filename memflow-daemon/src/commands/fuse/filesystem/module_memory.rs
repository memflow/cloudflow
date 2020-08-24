use super::{
    VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile, VirtualFileDataSource,
};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

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
    fn content_length(&self) -> u64 {
        match &self.ctx {
            VMFSScopeContext::Module {
                conn_id,
                pid,
                peb_entry,
            } => {
                let mut state = state_lock_sync();
                if let Some(conn) = state.connection_mut(&conn_id) {
                    match &mut conn.kernel {
                        KernelHandle::Win32(kernel) => {
                            if let Ok(mut process) = kernel.process_pid(*pid) {
                                if let Ok(module_info) = process.module_info_from_peb(*peb_entry) {
                                    return module_info.size() as u64;
                                }
                            }
                        }
                    }
                }
            }
            _ => (),
        }

        0
    }

    fn contents(&mut self, offset: i64, size: u32) -> Vec<u8> {
        match &self.ctx {
            VMFSScopeContext::Module {
                conn_id,
                pid,
                peb_entry,
            } => {
                let mut state = state_lock_sync();
                if let Some(conn) = state.connection_mut(&conn_id) {
                    match &mut conn.kernel {
                        KernelHandle::Win32(kernel) => {
                            if let Ok(mut process) = kernel.process_pid(*pid) {
                                if let Ok(module_info) = process.module_info_from_peb(*peb_entry) {
                                    let len = std::cmp::min(
                                        size as usize,
                                        module_info.size() - offset as usize,
                                    );
                                    return if len > 0 {
                                        process
                                            .virt_mem
                                            .virt_read_raw(
                                                module_info.base() + offset as usize,
                                                len,
                                            )
                                            .data_part()
                                            .unwrap()
                                    } else {
                                        Vec::new()
                                    };
                                }
                            }
                        }
                    }
                }
            }
            _ => (),
        };

        Vec::new()
    }
}
