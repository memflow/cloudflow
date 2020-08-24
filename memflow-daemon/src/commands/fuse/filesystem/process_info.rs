use super::{
    VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile, VirtualFileDataSource,
};
use crate::state::{state_lock_sync, KernelHandle};

pub struct VMFSProcessInfo;

impl VMFSModule for VMFSProcessInfo {
    fn scope(&self) -> VMFSModuleScope {
        VMFSModuleScope::Process
    }

    // TODO: cache contents size ?
    fn entry(&self, inode: u64, ctx: VMFSScopeContext) -> VirtualEntry {
        VirtualEntry::File(VirtualFile {
            inode,
            name: "process_info".to_string(),
            data_source: Box::new(VMFSProcessInfoDS::new(ctx)),
        })
    }
}

struct VMFSProcessInfoDS {
    ctx: VMFSScopeContext,
}

impl VMFSProcessInfoDS {
    pub fn new(ctx: VMFSScopeContext) -> Self {
        Self { ctx }
    }

    fn contents_raw(&self) -> Vec<u8> {
        let mut info = String::new();

        match &self.ctx {
            VMFSScopeContext::Process { conn_id, pid } => {
                let mut state = state_lock_sync();
                if let Some(conn) = state.connection_mut(&conn_id) {
                    match &mut conn.kernel {
                        KernelHandle::Win32(kernel) => {
                            if let Ok(process_info) = kernel.process_info_pid(*pid) {
                                info.push_str(&format!("{:?}", process_info)); // TODO: impl custom Display for pi
                            }
                        }
                    }
                }
            }
            _ => (),
        };

        info.as_bytes().to_vec()
    }
}

impl VirtualFileDataSource for VMFSProcessInfoDS {
    fn content_length(&self) -> u64 {
        self.contents_raw().len() as u64
    }

    fn contents(&mut self, offset: i64, size: u32) -> Vec<u8> {
        let info = self.contents_raw();
        let end = std::cmp::min((offset + size as i64) as usize, info.len());
        info.as_slice()[offset as usize..end].to_vec()
    }
}
