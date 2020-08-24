use super::{VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

pub struct VMFSModuleMemory;

impl VMFSModuleMemory {
    pub fn content_length(ctx: &VMFSScopeContext) -> u64 {
        match ctx {
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

    pub fn contents(ctx: &VMFSScopeContext, offset: i64, size: u32) -> Vec<u8> {
        match ctx {
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

impl VMFSModule for VMFSModuleMemory {
    fn scope(&self) -> VMFSModuleScope {
        VMFSModuleScope::Module
    }

    // TODO: cache contents size ?
    fn entry(&self, inode: u64, ctx: VMFSScopeContext) -> VirtualEntry {
        let ctx_clone = ctx.clone();
        VirtualEntry::File(VirtualFile {
            inode,
            name: "memory".to_string(),
            content_length: Box::new(move || Self::content_length(&ctx)),
            contents: Box::new(move |offset, size| Self::contents(&ctx_clone, offset, size)),
        })
    }
}
