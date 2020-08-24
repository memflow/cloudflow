use super::{
    VMFSModule, VMFSModuleScope, VMFSScopeContext, VirtualEntry, VirtualFile, VirtualFileDataSource,
};
use crate::error::{Error, Result};
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

    fn contents_raw(&self) -> Result<Vec<u8>> {
        if let VMFSScopeContext::Process { conn_id, pid } = &self.ctx {
            let mut state = state_lock_sync();
            let conn = state
                .connection_mut(&conn_id)
                .ok_or_else(|| Error::Other("connection not found"))?;

            match &mut conn.kernel {
                KernelHandle::Win32(kernel) => {
                    let process_info = kernel.process_info_pid(*pid).map_err(Error::from)?;
                    // TODO: impl custom Display for pi
                    Ok(format!("{:?}", process_info).as_bytes().to_vec())
                }
            }
        } else {
            Err(Error::Other("no process context supplied"))
        }
    }
}

impl VirtualFileDataSource for VMFSProcessInfoDS {
    fn content_length(&self) -> Result<u64> {
        Ok(self.contents_raw()?.len() as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        let info = self.contents_raw()?;
        let end = std::cmp::min((offset + size as i64) as usize, info.len());
        Ok(info.as_slice()[offset as usize..end].to_vec())
    }
}
