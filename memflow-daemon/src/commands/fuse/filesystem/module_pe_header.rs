use super::{CachedWin32Process, VMFSModuleExt, VirtualEntry, VirtualFile, VirtualFileDataSource};
use crate::error::{Error, Result};
use crate::state::{state_lock_sync, KernelHandle};

use memflow_core::*;
use memflow_win32::*;

use pelite::pe64::*;

pub struct VMFSModulePEHeader;

// TODO: only enable this for windows targets
impl VMFSModuleExt for VMFSModulePEHeader {
    fn entry(
        &self,
        inode: u64,
        conn_id: &str,
        process: &mut CachedWin32Process,
        mod_info: &Win32ModuleInfo,
    ) -> Result<VirtualEntry> {
        Ok(VirtualEntry::File(VirtualFile {
            inode,
            name: "pe_header".to_string(),
            data_source: Box::new(VMFSModulePEHeaderDS::new(
                &conn_id,
                process.proc_info.pid,
                mod_info,
            )),
        }))
    }
}

struct VMFSModulePEHeaderDS {
    conn_id: String,
    pid: PID,
    mod_base: Address,
    mod_size: usize,
    data: Option<String>,
}

impl VMFSModulePEHeaderDS {
    pub fn new(conn_id: &str, pid: PID, mod_info: &Win32ModuleInfo) -> Self {
        Self {
            conn_id: conn_id.to_string(),
            pid,
            mod_base: mod_info.base(),
            mod_size: mod_info.size(),
            data: None,
        }
    }

    fn update_data(&mut self) -> Result<()> {
        if self.data.is_some() {
            return Ok(());
        }

        let mut state = state_lock_sync();
        let conn = state
            .connection_mut(&self.conn_id)
            .ok_or_else(|| Error::Other("connection not found"))?;

        match &mut conn.kernel {
            KernelHandle::Win32(kernel) => {
                let mut process = kernel.process_pid(self.pid).map_err(Error::from)?;

                let image = process
                    .virt_mem
                    .virt_read_raw(self.mod_base, self.mod_size)
                    .data_part()?;
                let pe = PeView::from_bytes(&image).map_err(Error::PE)?;

                // TODO: once serde works use a lazy pe view here and not a static ds
                /*
                let pectx = MemoryPeViewContext::new(&mut ctx.process.virt_mem, mod_info.base())
                    .map_err(Error::PE)?;
                let pe = pe32::MemoryPeView::new(&pectx).map_err(Error::PE)?;
                */

                let pestr = serde_json::to_string_pretty(&pe).map_err(|_| Error::Serialize)?;

                self.data = Some(pestr);
                Ok(())
            }
        }
    }
}

impl VirtualFileDataSource for VMFSModulePEHeaderDS {
    fn content_length(&mut self) -> Result<u64> {
        self.update_data()?;

        Ok(self.data.as_ref().unwrap().len() as u64)
    }

    fn contents(&mut self, offset: i64, size: u32) -> Result<Vec<u8>> {
        self.update_data()?;

        let contents = self.data.as_ref().unwrap().as_bytes();
        let end = std::cmp::min((offset + size as i64) as usize, contents.len());
        Ok(contents[offset as usize..end].to_vec())
    }
}
