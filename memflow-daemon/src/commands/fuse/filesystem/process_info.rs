use super::{VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile};
use crate::state::{state_lock_sync, KernelHandle};

pub struct VMFSProcessInfo;

impl VMFSProcessInfo {
    pub fn content_length(ctx: &VMFSScopeContext) -> u64 {
        Self::contents(ctx).len() as u64
    }

    // TODO: allow result
    pub fn contents(ctx: &VMFSScopeContext) -> Vec<u8> {
        let mut info = String::new();

        match ctx {
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

impl VMFSModule for VMFSProcessInfo {
    fn scope(&self) -> VMFSModuleScope {
        VMFSModuleScope::Process
    }

    // TODO: cache contents size ?
    fn entry(&self, inode: u64, ctx: VMFSScopeContext) -> VirtualEntry {
        let ctx_clone = ctx.clone();
        VirtualEntry::File(VirtualFile {
            inode,
            name: "process_info".to_string(),
            content_length: Box::new(move || Self::content_length(&ctx)),
            contents: Box::new(move || Self::contents(&ctx_clone)),
        })
    }
}
